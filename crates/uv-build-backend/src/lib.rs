mod metadata;
mod serde_verbatim;
mod source_dist;
mod wheel;

pub use metadata::{check_direct_build, PyProjectToml};
pub use source_dist::{build_source_dist, list_source_dist};
pub use wheel::{build_editable, build_wheel, list_wheel, metadata};

use crate::metadata::ValidationError;
use std::fs::FileType;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::debug;
use uv_fs::Simplified;
use uv_globfilter::PortableGlobError;
use uv_pypi_types::IdentifierParseError;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Invalid pyproject.toml")]
    Toml(#[from] toml::de::Error),
    #[error("Invalid pyproject.toml")]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Identifier(#[from] IdentifierParseError),
    #[error("Unsupported glob expression in: `{field}`")]
    PortableGlob {
        field: String,
        #[source]
        source: PortableGlobError,
    },
    /// <https://github.com/BurntSushi/ripgrep/discussions/2927>
    #[error("Glob expressions caused to large regex in: `{field}`")]
    GlobSetTooLarge {
        field: String,
        #[source]
        source: globset::Error,
    },
    #[error("`pyproject.toml` must not be excluded from source distribution build")]
    PyprojectTomlExcluded,
    #[error("Failed to walk source tree: `{}`", root.user_display())]
    WalkDir {
        root: PathBuf,
        #[source]
        err: walkdir::Error,
    },
    #[error("Unsupported file type {:?}: `{}`", _1, _0.user_display())]
    UnsupportedFileType(PathBuf, FileType),
    #[error("Failed to write wheel zip archive")]
    Zip(#[from] zip::result::ZipError),
    #[error("Failed to write RECORD file")]
    Csv(#[from] csv::Error),
    #[error("Expected a Python module with an `__init__.py` at: `{}`", _0.user_display())]
    MissingModule(PathBuf),
    #[error("Absolute module root is not allowed: `{}`", _0.display())]
    AbsoluteModuleRoot(PathBuf),
    #[error("Inconsistent metadata between prepare and build step: `{0}`")]
    InconsistentSteps(&'static str),
    #[error("Failed to write to {}", _0.user_display())]
    TarWrite(PathBuf, #[source] io::Error),
}

/// Dispatcher between writing to a directory, writing to a zip, writing to a `.tar.gz` and
/// listing files.
///
/// All paths are string types instead of path types since wheels are portable between platforms.
///
/// Contract: You must call close before dropping to obtain a valid output (dropping is fine in the
/// error case).
trait DirectoryWriter {
    /// Add a file with the given content.
    ///
    /// Files added through the method are considered generated when listing included files.
    fn write_bytes(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error>;

    /// Add a local file.
    fn write_file(&mut self, path: &str, file: &Path) -> Result<(), Error>;

    /// Create a directory.
    fn write_directory(&mut self, directory: &str) -> Result<(), Error>;

    /// Write the `RECORD` file and if applicable, the central directory.
    fn close(self, dist_info_dir: &str) -> Result<(), Error>;
}

/// Name of the file in the archive and path outside, if it wasn't generated.
pub(crate) type FileList = Vec<(String, Option<PathBuf>)>;

/// A dummy writer to collect the file names that would be included in a build.
pub(crate) struct ListWriter<'a> {
    files: &'a mut FileList,
}

impl<'a> ListWriter<'a> {
    /// Convert the writer to the collected file names.
    pub(crate) fn new(files: &'a mut FileList) -> Self {
        Self { files }
    }
}

impl DirectoryWriter for ListWriter<'_> {
    fn write_bytes(&mut self, path: &str, _bytes: &[u8]) -> Result<(), Error> {
        self.files.push((path.to_string(), None));
        Ok(())
    }

    fn write_file(&mut self, path: &str, file: &Path) -> Result<(), Error> {
        self.files
            .push((path.to_string(), Some(file.to_path_buf())));
        Ok(())
    }

    fn write_directory(&mut self, _directory: &str) -> Result<(), Error> {
        Ok(())
    }

    fn close(self, _dist_info_dir: &str) -> Result<(), Error> {
        Ok(())
    }
}

/// PEP 517 requires that the metadata directory from the prepare metadata call is identical to the
/// build wheel call. This method performs a prudence check that `METADATA` and `entry_points.txt`
/// match.
fn check_metadata_directory(
    source_tree: &Path,
    metadata_directory: Option<&Path>,
    pyproject_toml: &PyProjectToml,
) -> Result<(), Error> {
    let Some(metadata_directory) = metadata_directory else {
        return Ok(());
    };

    debug!(
        "Checking metadata directory {}",
        metadata_directory.user_display()
    );

    // `METADATA` is a mandatory file.
    let current = pyproject_toml
        .to_metadata(source_tree)?
        .core_metadata_format();
    let previous = fs_err::read_to_string(metadata_directory.join("METADATA"))?;
    if previous != current {
        return Err(Error::InconsistentSteps("METADATA"));
    }

    // `entry_points.txt` is not written if it would be empty.
    let entrypoints_path = metadata_directory.join("entry_points.txt");
    match pyproject_toml.to_entry_points()? {
        None => {
            if entrypoints_path.is_file() {
                return Err(Error::InconsistentSteps("entry_points.txt"));
            }
        }
        Some(entrypoints) => {
            if fs_err::read_to_string(&entrypoints_path)? != entrypoints {
                return Err(Error::InconsistentSteps("entry_points.txt"));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::bufread::GzDecoder;
    use fs_err::File;
    use indoc::indoc;
    use insta::assert_snapshot;
    use itertools::Itertools;
    use std::io::{BufReader, Read};
    use tempfile::TempDir;
    use uv_fs::{copy_dir_all, relative_to};

    /// Test that source tree -> source dist -> wheel includes the right files and is stable and
    /// deterministic in dependent of the build path.
    #[test]
    fn built_by_uv_building() {
        let built_by_uv = Path::new("../../scripts/packages/built-by-uv");
        let src = TempDir::new().unwrap();
        for dir in [
            "src",
            "tests",
            "data-dir",
            "third-party-licenses",
            "assets",
            "header",
            "scripts",
        ] {
            copy_dir_all(built_by_uv.join(dir), src.path().join(dir)).unwrap();
        }
        for dir in [
            "pyproject.toml",
            "README.md",
            "uv.lock",
            "LICENSE-APACHE",
            "LICENSE-MIT",
        ] {
            fs_err::copy(built_by_uv.join(dir), src.path().join(dir)).unwrap();
        }

        // Add some files to be excluded
        let module_root = src.path().join("src").join("built_by_uv");
        fs_err::create_dir_all(module_root.join("__pycache__")).unwrap();
        File::create(module_root.join("__pycache__").join("compiled.pyc")).unwrap();
        File::create(module_root.join("arithmetic").join("circle.pyc")).unwrap();

        // Build a wheel from the source tree
        let direct_output_dir = TempDir::new().unwrap();
        let (_name, wheel_list_files) = list_wheel(src.path(), "1.0.0+test").unwrap();
        build_wheel(src.path(), direct_output_dir.path(), None, "1.0.0+test").unwrap();

        let wheel = zip::ZipArchive::new(
            File::open(
                direct_output_dir
                    .path()
                    .join("built_by_uv-0.1.0-py3-none-any.whl"),
            )
            .unwrap(),
        )
        .unwrap();
        let mut direct_wheel_contents: Vec<_> = wheel.file_names().collect();
        direct_wheel_contents.sort_unstable();

        // List file and build a source dist from the source tree
        let source_dist_dir = TempDir::new().unwrap();
        let (_name, source_dist_list_files) = list_source_dist(src.path(), "1.0.0+test").unwrap();
        build_source_dist(src.path(), source_dist_dir.path(), "1.0.0+test").unwrap();

        // Build a wheel from the source dist
        let sdist_tree = TempDir::new().unwrap();
        let source_dist_path = source_dist_dir.path().join("built_by_uv-0.1.0.tar.gz");
        let sdist_reader = BufReader::new(File::open(&source_dist_path).unwrap());
        let mut source_dist = tar::Archive::new(GzDecoder::new(sdist_reader));
        let mut source_dist_contents: Vec<_> = source_dist
            .entries()
            .unwrap()
            .map(|entry| entry.unwrap().path().unwrap().to_str().unwrap().to_string())
            .collect();
        source_dist_contents.sort();
        // Reset the reader and unpack
        let sdist_reader = BufReader::new(File::open(&source_dist_path).unwrap());
        let mut source_dist = tar::Archive::new(GzDecoder::new(sdist_reader));
        source_dist.unpack(sdist_tree.path()).unwrap();
        drop(source_dist_dir);

        let indirect_output_dir = TempDir::new().unwrap();
        build_wheel(
            &sdist_tree.path().join("built_by_uv-0.1.0"),
            indirect_output_dir.path(),
            None,
            "1.0.0+test",
        )
        .unwrap();
        let wheel = zip::ZipArchive::new(
            File::open(
                indirect_output_dir
                    .path()
                    .join("built_by_uv-0.1.0-py3-none-any.whl"),
            )
            .unwrap(),
        )
        .unwrap();
        let mut indirect_wheel_contents: Vec<_> = wheel.file_names().collect();
        indirect_wheel_contents.sort_unstable();
        assert_eq!(indirect_wheel_contents, direct_wheel_contents);

        let format_file_list = |file_list: FileList| {
            file_list
                .into_iter()
                .map(|(path, source)| {
                    let path = path.replace('\\', "/");
                    if let Some(source) = source {
                        let source = relative_to(source, src.path())
                            .unwrap()
                            .portable_display()
                            .to_string();
                        format!("{path} ({source})")
                    } else {
                        format!("{path} (generated)")
                    }
                })
                .join("\n")
        };

        // Check the contained files and directories
        assert_snapshot!(source_dist_contents.iter().map(|path| path.replace('\\', "/")).join("\n"), @r###"
        built_by_uv-0.1.0/
        built_by_uv-0.1.0/LICENSE-APACHE
        built_by_uv-0.1.0/LICENSE-MIT
        built_by_uv-0.1.0/PKG-INFO
        built_by_uv-0.1.0/README.md
        built_by_uv-0.1.0/assets
        built_by_uv-0.1.0/assets/data.csv
        built_by_uv-0.1.0/header
        built_by_uv-0.1.0/header/built_by_uv.h
        built_by_uv-0.1.0/pyproject.toml
        built_by_uv-0.1.0/scripts
        built_by_uv-0.1.0/scripts/whoami.sh
        built_by_uv-0.1.0/src
        built_by_uv-0.1.0/src/built_by_uv
        built_by_uv-0.1.0/src/built_by_uv/__init__.py
        built_by_uv-0.1.0/src/built_by_uv/arithmetic
        built_by_uv-0.1.0/src/built_by_uv/arithmetic/__init__.py
        built_by_uv-0.1.0/src/built_by_uv/arithmetic/circle.py
        built_by_uv-0.1.0/src/built_by_uv/arithmetic/pi.txt
        built_by_uv-0.1.0/src/built_by_uv/build-only.h
        built_by_uv-0.1.0/src/built_by_uv/cli.py
        built_by_uv-0.1.0/third-party-licenses
        built_by_uv-0.1.0/third-party-licenses/PEP-401.txt
        "###);
        assert_snapshot!(format_file_list(source_dist_list_files), @r###"
        built_by_uv-0.1.0/LICENSE-APACHE (LICENSE-APACHE)
        built_by_uv-0.1.0/LICENSE-MIT (LICENSE-MIT)
        built_by_uv-0.1.0/PKG-INFO (generated)
        built_by_uv-0.1.0/README.md (README.md)
        built_by_uv-0.1.0/assets/data.csv (assets/data.csv)
        built_by_uv-0.1.0/header/built_by_uv.h (header/built_by_uv.h)
        built_by_uv-0.1.0/pyproject.toml (pyproject.toml)
        built_by_uv-0.1.0/scripts/whoami.sh (scripts/whoami.sh)
        built_by_uv-0.1.0/src/built_by_uv/__init__.py (src/built_by_uv/__init__.py)
        built_by_uv-0.1.0/src/built_by_uv/arithmetic/__init__.py (src/built_by_uv/arithmetic/__init__.py)
        built_by_uv-0.1.0/src/built_by_uv/arithmetic/circle.py (src/built_by_uv/arithmetic/circle.py)
        built_by_uv-0.1.0/src/built_by_uv/arithmetic/pi.txt (src/built_by_uv/arithmetic/pi.txt)
        built_by_uv-0.1.0/src/built_by_uv/build-only.h (src/built_by_uv/build-only.h)
        built_by_uv-0.1.0/src/built_by_uv/cli.py (src/built_by_uv/cli.py)
        built_by_uv-0.1.0/third-party-licenses/PEP-401.txt (third-party-licenses/PEP-401.txt)
        "###);

        assert_snapshot!(indirect_wheel_contents.iter().map(|path| path.replace('\\', "/")).join("\n"), @r###"
        built_by_uv-0.1.0.data/data/
        built_by_uv-0.1.0.data/data/data.csv
        built_by_uv-0.1.0.data/headers/
        built_by_uv-0.1.0.data/headers/built_by_uv.h
        built_by_uv-0.1.0.data/scripts/
        built_by_uv-0.1.0.data/scripts/whoami.sh
        built_by_uv-0.1.0.dist-info/
        built_by_uv-0.1.0.dist-info/METADATA
        built_by_uv-0.1.0.dist-info/RECORD
        built_by_uv-0.1.0.dist-info/WHEEL
        built_by_uv-0.1.0.dist-info/entry_points.txt
        built_by_uv-0.1.0.dist-info/licenses/
        built_by_uv-0.1.0.dist-info/licenses/LICENSE-APACHE
        built_by_uv-0.1.0.dist-info/licenses/LICENSE-MIT
        built_by_uv-0.1.0.dist-info/licenses/third-party-licenses/
        built_by_uv-0.1.0.dist-info/licenses/third-party-licenses/PEP-401.txt
        built_by_uv/
        built_by_uv/__init__.py
        built_by_uv/arithmetic/
        built_by_uv/arithmetic/__init__.py
        built_by_uv/arithmetic/circle.py
        built_by_uv/arithmetic/pi.txt
        built_by_uv/cli.py
        "###);

        assert_snapshot!(format_file_list(wheel_list_files), @r###"
        built_by_uv-0.1.0.data/data/data.csv (assets/data.csv)
        built_by_uv-0.1.0.data/headers/built_by_uv.h (header/built_by_uv.h)
        built_by_uv-0.1.0.data/scripts/whoami.sh (scripts/whoami.sh)
        built_by_uv-0.1.0.dist-info/METADATA (generated)
        built_by_uv-0.1.0.dist-info/WHEEL (generated)
        built_by_uv-0.1.0.dist-info/entry_points.txt (generated)
        built_by_uv-0.1.0.dist-info/licenses/LICENSE-APACHE (LICENSE-APACHE)
        built_by_uv-0.1.0.dist-info/licenses/LICENSE-MIT (LICENSE-MIT)
        built_by_uv-0.1.0.dist-info/licenses/third-party-licenses/PEP-401.txt (third-party-licenses/PEP-401.txt)
        built_by_uv/__init__.py (src/built_by_uv/__init__.py)
        built_by_uv/arithmetic/__init__.py (src/built_by_uv/arithmetic/__init__.py)
        built_by_uv/arithmetic/circle.py (src/built_by_uv/arithmetic/circle.py)
        built_by_uv/arithmetic/pi.txt (src/built_by_uv/arithmetic/pi.txt)
        built_by_uv/cli.py (src/built_by_uv/cli.py)
        "###);

        // Check that we write deterministic wheels.
        let wheel_filename = "built_by_uv-0.1.0-py3-none-any.whl";
        assert_eq!(
            fs_err::read(direct_output_dir.path().join(wheel_filename)).unwrap(),
            fs_err::read(indirect_output_dir.path().join(wheel_filename)).unwrap()
        );
    }

    /// Test that `license = { file = "LICENSE" }` is supported.
    #[test]
    fn license_file_pre_pep639() {
        let src = TempDir::new().unwrap();
        fs_err::write(
            src.path().join("pyproject.toml"),
            indoc! {r#"
            [project]
            name = "pep-pep639-license"
            version = "1.0.0"
            license = { file = "license.txt" }

            [build-system]
            requires = ["uv_build>=0.5.15,<0.6"]
            build-backend = "uv_build"
        "#
            },
        )
        .unwrap();
        fs_err::create_dir_all(src.path().join("src").join("pep_pep639_license")).unwrap();
        File::create(
            src.path()
                .join("src")
                .join("pep_pep639_license")
                .join("__init__.py"),
        )
        .unwrap();
        fs_err::write(
            src.path().join("license.txt"),
            "Copy carefully.\nSincerely, the authors",
        )
        .unwrap();

        // Build a wheel from a source distribution
        let output_dir = TempDir::new().unwrap();
        build_source_dist(src.path(), output_dir.path(), "0.5.15").unwrap();
        let sdist_tree = TempDir::new().unwrap();
        let source_dist_path = output_dir.path().join("pep_pep639_license-1.0.0.tar.gz");
        let sdist_reader = BufReader::new(File::open(&source_dist_path).unwrap());
        let mut source_dist = tar::Archive::new(GzDecoder::new(sdist_reader));
        source_dist.unpack(sdist_tree.path()).unwrap();
        build_wheel(
            &sdist_tree.path().join("pep_pep639_license-1.0.0"),
            output_dir.path(),
            None,
            "0.5.15",
        )
        .unwrap();
        let wheel = output_dir
            .path()
            .join("pep_pep639_license-1.0.0-py3-none-any.whl");
        let mut wheel = zip::ZipArchive::new(File::open(wheel).unwrap()).unwrap();

        let mut metadata = String::new();
        wheel
            .by_name("pep_pep639_license-1.0.0.dist-info/METADATA")
            .unwrap()
            .read_to_string(&mut metadata)
            .unwrap();

        assert_snapshot!(metadata, @r###"
        Metadata-Version: 2.3
        Name: pep-pep639-license
        Version: 1.0.0
        License: Copy carefully.
                 Sincerely, the authors
        "###);
    }

    /// Test that `build_wheel` works after the `prepare_metadata_for_build_wheel` hook.
    #[test]
    fn prepare_metadata_then_build_wheel() {
        let src = TempDir::new().unwrap();
        fs_err::write(
            src.path().join("pyproject.toml"),
            indoc! {r#"
            [project]
            name = "two-step-build"
            version = "1.0.0"

            [build-system]
            requires = ["uv_build>=0.5.15,<0.6"]
            build-backend = "uv_build"
        "#
            },
        )
        .unwrap();
        fs_err::create_dir_all(src.path().join("src").join("two_step_build")).unwrap();
        File::create(
            src.path()
                .join("src")
                .join("two_step_build")
                .join("__init__.py"),
        )
        .unwrap();

        // Prepare the metadata.
        let metadata_dir = TempDir::new().unwrap();
        let dist_info_dir = metadata(src.path(), metadata_dir.path(), "0.5.15").unwrap();
        let metadata_prepared =
            fs_err::read_to_string(metadata_dir.path().join(&dist_info_dir).join("METADATA"))
                .unwrap();

        // Build the wheel, using the prepared metadata directory.
        let output_dir = TempDir::new().unwrap();
        build_wheel(
            src.path(),
            output_dir.path(),
            Some(&metadata_dir.path().join(&dist_info_dir)),
            "0.5.15",
        )
        .unwrap();
        let wheel = output_dir
            .path()
            .join("two_step_build-1.0.0-py3-none-any.whl");
        let mut wheel = zip::ZipArchive::new(File::open(wheel).unwrap()).unwrap();

        let mut metadata_wheel = String::new();
        wheel
            .by_name("two_step_build-1.0.0.dist-info/METADATA")
            .unwrap()
            .read_to_string(&mut metadata_wheel)
            .unwrap();

        assert_eq!(metadata_prepared, metadata_wheel);

        assert_snapshot!(metadata_wheel, @r###"
        Metadata-Version: 2.3
        Name: two-step-build
        Version: 1.0.0
        "###);
    }
}

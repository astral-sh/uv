mod metadata;
mod serde_verbatim;
mod settings;
mod source_dist;
mod wheel;

pub use metadata::{PyProjectToml, check_direct_build};
pub use settings::{BuildBackendSettings, WheelDataIncludes};
pub use source_dist::{build_source_dist, list_source_dist};
pub use wheel::{build_editable, build_wheel, list_wheel, metadata};

use std::fs::FileType;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use thiserror::Error;
use tracing::debug;

use uv_fs::Simplified;
use uv_globfilter::PortableGlobError;
use uv_normalize::PackageName;
use uv_pypi_types::IdentifierParseError;

use crate::metadata::ValidationError;
use crate::settings::ModuleName;

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
    #[error(
        "Missing source directory at: `{}`",
        _0.user_display()
    )]
    MissingSrc(PathBuf),
    #[error(
        "Expected a Python module directory at: `{}`",
        _0.user_display()
    )]
    MissingInitPy(PathBuf),
    #[error(
        "Missing module directory for `{}` in `{}`. Found: `{}`",
        module_name,
        src_root.user_display(),
        dir_listing.join("`, `")
    )]
    MissingModuleDir {
        module_name: String,
        src_root: PathBuf,
        dir_listing: Vec<String>,
    },
    /// Either an absolute path or a parent path through `..`.
    #[error("Module root must be inside the project: `{}`", _0.user_display())]
    InvalidModuleRoot(PathBuf),
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

/// Resolve the source root, module root and the module name.
fn find_roots(
    source_tree: &Path,
    pyproject_toml: &PyProjectToml,
    relative_module_root: &Path,
    module_name: Option<&ModuleName>,
) -> Result<(PathBuf, PathBuf), Error> {
    let relative_module_root = uv_fs::normalize_path(relative_module_root);
    let src_root = source_tree.join(&relative_module_root);
    if !src_root.starts_with(source_tree) {
        return Err(Error::InvalidModuleRoot(relative_module_root.to_path_buf()));
    }
    let src_root = source_tree.join(&relative_module_root);
    let module_root = find_module_root(&src_root, module_name, pyproject_toml.name())?;
    Ok((src_root, module_root))
}

/// Match the module name to its module directory with potentially different casing.
///
/// Some target platforms have case-sensitive filesystems, while others have case-insensitive
/// filesystems and we always lower case the package name, our default for the module, while some
/// users want uppercase letters in their module names. For example, the package name is `pil_util`,
/// but the module `PIL_util`.
///
/// By default, the dist-info-normalized package name is the module name. For
/// dist-info-normalization, the rules are lowercasing, replacing `.` with `_` and
/// replace `-` with `_`. Since `.` and `-` are not allowed in identifiers, we can use a string
/// comparison with the module name.
///
/// To make the behavior as consistent as possible across platforms as possible, we require that an
/// upper case name is given explicitly through `tool.uv.module-name`.
///
/// Returns the module root path, the directory below which the `__init__.py` lives.
fn find_module_root(
    src_root: &Path,
    module_name: Option<&ModuleName>,
    package_name: &PackageName,
) -> Result<PathBuf, Error> {
    let (module_name, stubs) = if let Some(module_name) = module_name {
        // This name can be uppercase.
        match module_name {
            ModuleName::Identifier(module_name) => (module_name.to_string(), false),
            ModuleName::Stubs(module_name) => (module_name.to_string(), true),
        }
    } else {
        // Infer stubs packages from package name alone. There are potential false positives if
        // someone had a regular package with `-stubs`.
        if let Some(stem) = package_name.to_string().strip_suffix("-stubs") {
            debug!("Building stubs package instead of a regular package");
            let module_name = PackageName::from_str(stem)
                .expect("non-empty package name prefix must be valid package name")
                .as_dist_info_name()
                .to_string();
            (format!("{module_name}-stubs"), true)
        } else {
            // This name is always lowercase.
            (package_name.as_dist_info_name().to_string(), false)
        }
    };

    let dir = match fs_err::read_dir(src_root) {
        Ok(dir_iterator) => dir_iterator.collect::<Result<Vec<_>, _>>()?,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Err(Error::MissingSrc(src_root.to_path_buf()));
        }
        Err(err) => return Err(Error::Io(err)),
    };
    let module_root = dir.iter().find_map(|entry| {
        // TODO(konsti): Do we ever need to check if `dir/{module_name}/__init__.py` exists because
        // the wrong casing may be recorded on disk?
        if entry
            .file_name()
            .to_str()
            .is_some_and(|file_name| file_name == module_name)
        {
            Some(entry.path())
        } else {
            None
        }
    });
    let init_py = if stubs { "__init__.pyi" } else { "__init__.py" };
    let module_root = if let Some(module_root) = module_root {
        if module_root.join(init_py).is_file() {
            module_root.clone()
        } else {
            return Err(Error::MissingInitPy(module_root.join(init_py)));
        }
    } else {
        return Err(Error::MissingModuleDir {
            module_name,
            src_root: src_root.to_path_buf(),
            dir_listing: dir
                .into_iter()
                .filter_map(|entry| Some(entry.file_name().to_str()?.to_string()))
                .collect(),
        });
    };

    debug!("Module name: `{}`", module_name);
    Ok(module_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::bufread::GzDecoder;
    use fs_err::File;
    use indoc::indoc;
    use insta::assert_snapshot;
    use itertools::Itertools;
    use sha2::Digest;
    use std::io::{BufReader, Read};
    use std::iter;
    use tempfile::TempDir;
    use uv_distribution_filename::{SourceDistFilename, WheelFilename};
    use uv_fs::{copy_dir_all, relative_to};

    fn format_err(err: &Error) -> String {
        let context = iter::successors(std::error::Error::source(&err), |&err| err.source())
            .map(|err| format!("  Caused by: {err}"))
            .join("\n");
        err.to_string() + "\n" + &context
    }

    /// File listings, generated archives and archive contents for both a build with
    /// source tree -> wheel
    /// and a build with
    /// source tree -> source dist -> wheel.
    #[derive(Debug, PartialEq, Eq)]
    struct BuildResults {
        source_dist_list_files: FileList,
        source_dist_filename: SourceDistFilename,
        source_dist_contents: Vec<String>,
        wheel_list_files: FileList,
        wheel_filename: WheelFilename,
        wheel_contents: Vec<String>,
    }

    /// Run both a direct wheel build and an indirect wheel build through a source distribution,
    /// while checking that directly built wheel and indirectly built wheel are the same.
    fn build(source_root: &Path, dist: &Path) -> Result<BuildResults, Error> {
        // Build a direct wheel, capture all its properties to compare it with the indirect wheel
        // latest and remove it since it has the same filename as the indirect wheel.
        let (_name, direct_wheel_list_files) = list_wheel(source_root, "1.0.0+test")?;
        let direct_wheel_filename = build_wheel(source_root, dist, None, "1.0.0+test")?;
        let direct_wheel_path = dist.join(direct_wheel_filename.to_string());
        let direct_wheel_contents = wheel_contents(&direct_wheel_path);
        let direct_wheel_hash = sha2::Sha256::digest(fs_err::read(&direct_wheel_path)?);
        fs_err::remove_file(&direct_wheel_path)?;

        // Build a source distribution.
        let (_name, source_dist_list_files) = list_source_dist(source_root, "1.0.0+test")?;
        // TODO(konsti): This should run in the unpacked source dist tempdir, but we need to
        // normalize the path.
        let (_name, wheel_list_files) = list_wheel(source_root, "1.0.0+test")?;
        let source_dist_filename = build_source_dist(source_root, dist, "1.0.0+test")?;
        let source_dist_path = dist.join(source_dist_filename.to_string());
        let source_dist_contents = sdist_contents(&source_dist_path);

        // Unpack the source distribution and build a wheel from it.
        let sdist_tree = TempDir::new()?;
        let sdist_reader = BufReader::new(File::open(&source_dist_path)?);
        let mut source_dist = tar::Archive::new(GzDecoder::new(sdist_reader));
        source_dist.unpack(sdist_tree.path())?;
        let sdist_top_level_directory = sdist_tree.path().join(format!(
            "{}-{}",
            source_dist_filename.name.as_dist_info_name(),
            source_dist_filename.version
        ));
        let wheel_filename = build_wheel(&sdist_top_level_directory, dist, None, "1.0.0+test")?;
        let wheel_contents = wheel_contents(&dist.join(wheel_filename.to_string()));

        // Check that direct and indirect wheels are identical.
        assert_eq!(direct_wheel_filename, wheel_filename);
        assert_eq!(direct_wheel_contents, wheel_contents);
        assert_eq!(direct_wheel_list_files, wheel_list_files);
        assert_eq!(
            direct_wheel_hash,
            sha2::Sha256::digest(fs_err::read(dist.join(wheel_filename.to_string()))?)
        );

        Ok(BuildResults {
            source_dist_list_files,
            source_dist_filename,
            source_dist_contents,
            wheel_list_files,
            wheel_filename,
            wheel_contents,
        })
    }

    fn sdist_contents(source_dist_path: &Path) -> Vec<String> {
        let sdist_reader = BufReader::new(File::open(source_dist_path).unwrap());
        let mut source_dist = tar::Archive::new(GzDecoder::new(sdist_reader));
        let mut source_dist_contents: Vec<_> = source_dist
            .entries()
            .unwrap()
            .map(|entry| {
                entry
                    .unwrap()
                    .path()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .replace('\\', "/")
            })
            .collect();
        source_dist_contents.sort();
        source_dist_contents
    }

    fn wheel_contents(direct_output_dir: &Path) -> Vec<String> {
        let wheel = zip::ZipArchive::new(File::open(direct_output_dir).unwrap()).unwrap();
        let mut wheel_contents: Vec<_> = wheel
            .file_names()
            .map(|path| path.replace('\\', "/"))
            .collect();
        wheel_contents.sort_unstable();
        wheel_contents
    }

    fn format_file_list(file_list: FileList, src: &Path) -> String {
        file_list
            .into_iter()
            .map(|(path, source)| {
                let path = path.replace('\\', "/");
                if let Some(source) = source {
                    let source = relative_to(source, src)
                        .unwrap()
                        .portable_display()
                        .to_string();
                    format!("{path} ({source})")
                } else {
                    format!("{path} (generated)")
                }
            })
            .join("\n")
    }

    /// Tests that builds are stable and include the right files and.
    ///
    /// Tests that both source tree -> source dist -> wheel and source tree -> wheel include the
    /// right files. Also checks that the resulting archives are byte-by-byte identical
    /// independent of the build path or platform, with the caveat that we cannot serialize an
    /// executable bit on Window. This ensures reproducible builds and best-effort
    /// platform-independent deterministic builds.
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

        // Clear executable bit on Unix to build the same archive between Unix and Windows.
        // This is a caveat to the determinism of the uv build backend: When a file has the
        // executable in the source repository, it only has the executable bit on Unix, as Windows
        // does not have the concept of the executable bit.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = src.path().join("scripts").join("whoami.sh");
            let metadata = fs_err::metadata(&path).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(perms.mode() & !0o111);
            fs_err::set_permissions(&path, perms).unwrap();
        }

        // Add some files to be excluded
        let module_root = src.path().join("src").join("built_by_uv");
        fs_err::create_dir_all(module_root.join("__pycache__")).unwrap();
        File::create(module_root.join("__pycache__").join("compiled.pyc")).unwrap();
        File::create(module_root.join("arithmetic").join("circle.pyc")).unwrap();

        // Perform both the direct and the indirect build.
        let dist = TempDir::new().unwrap();
        let build = build(src.path(), dist.path()).unwrap();

        let source_dist_path = dist.path().join(build.source_dist_filename.to_string());
        assert_eq!(
            build.source_dist_filename.to_string(),
            "built_by_uv-0.1.0.tar.gz"
        );
        // Check that the source dist is reproducible across platforms.
        assert_snapshot!(
            format!("{:x}", sha2::Sha256::digest(fs_err::read(&source_dist_path).unwrap())),
            @"dab46bcc4d66960a11cfdc19604512a8e1a3241a67536f7e962166760e9c575c"
        );
        // Check both the files we report and the actual files
        assert_snapshot!(format_file_list(build.source_dist_list_files, src.path()), @r"
        built_by_uv-0.1.0/PKG-INFO (generated)
        built_by_uv-0.1.0/LICENSE-APACHE (LICENSE-APACHE)
        built_by_uv-0.1.0/LICENSE-MIT (LICENSE-MIT)
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
        ");
        assert_snapshot!(build.source_dist_contents.iter().join("\n"), @r"
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
        ");

        let wheel_path = dist.path().join(build.wheel_filename.to_string());
        assert_eq!(
            build.wheel_filename.to_string(),
            "built_by_uv-0.1.0-py3-none-any.whl"
        );
        // Check that the wheel is reproducible across platforms.
        assert_snapshot!(
            format!("{:x}", sha2::Sha256::digest(fs_err::read(&wheel_path).unwrap())),
            @"ac3f68ac448023bca26de689d80401bff57f764396ae802bf4666234740ffbe3"
        );
        assert_snapshot!(build.wheel_contents.join("\n"), @r"
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
        ");
        assert_snapshot!(format_file_list(build.wheel_list_files, src.path()), @r"
        built_by_uv/__init__.py (src/built_by_uv/__init__.py)
        built_by_uv/arithmetic/__init__.py (src/built_by_uv/arithmetic/__init__.py)
        built_by_uv/arithmetic/circle.py (src/built_by_uv/arithmetic/circle.py)
        built_by_uv/arithmetic/pi.txt (src/built_by_uv/arithmetic/pi.txt)
        built_by_uv/cli.py (src/built_by_uv/cli.py)
        built_by_uv-0.1.0.dist-info/licenses/LICENSE-APACHE (LICENSE-APACHE)
        built_by_uv-0.1.0.dist-info/licenses/LICENSE-MIT (LICENSE-MIT)
        built_by_uv-0.1.0.dist-info/licenses/third-party-licenses/PEP-401.txt (third-party-licenses/PEP-401.txt)
        built_by_uv-0.1.0.data/headers/built_by_uv.h (header/built_by_uv.h)
        built_by_uv-0.1.0.data/scripts/whoami.sh (scripts/whoami.sh)
        built_by_uv-0.1.0.data/data/data.csv (assets/data.csv)
        built_by_uv-0.1.0.dist-info/WHEEL (generated)
        built_by_uv-0.1.0.dist-info/entry_points.txt (generated)
        built_by_uv-0.1.0.dist-info/METADATA (generated)
        ");
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

    /// Check that non-normalized paths for `module-root` work with the glob inclusions.
    #[test]
    fn test_glob_path_normalization() {
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

            [tool.uv.build-backend]
            module-root = "./"
            "#
            },
        )
        .unwrap();

        fs_err::create_dir_all(src.path().join("two_step_build")).unwrap();
        File::create(src.path().join("two_step_build").join("__init__.py")).unwrap();

        let dist = TempDir::new().unwrap();
        let build1 = build(src.path(), dist.path()).unwrap();

        assert_snapshot!(build1.source_dist_contents.join("\n"), @r"
        two_step_build-1.0.0/
        two_step_build-1.0.0/PKG-INFO
        two_step_build-1.0.0/pyproject.toml
        two_step_build-1.0.0/two_step_build
        two_step_build-1.0.0/two_step_build/__init__.py
        ");

        assert_snapshot!(build1.wheel_contents.join("\n"), @r"
        two_step_build-1.0.0.dist-info/
        two_step_build-1.0.0.dist-info/METADATA
        two_step_build-1.0.0.dist-info/RECORD
        two_step_build-1.0.0.dist-info/WHEEL
        two_step_build/
        two_step_build/__init__.py
        ");

        // A path with a parent reference.
        fs_err::write(
            src.path().join("pyproject.toml"),
            indoc! {r#"
            [project]
            name = "two-step-build"
            version = "1.0.0"

            [build-system]
            requires = ["uv_build>=0.5.15,<0.6"]
            build-backend = "uv_build"

            [tool.uv.build-backend]
            module-root = "two_step_build/.././"
            "#
            },
        )
        .unwrap();

        let dist = TempDir::new().unwrap();
        let build2 = build(src.path(), dist.path()).unwrap();
        assert_eq!(build1, build2);
    }

    /// Check that upper case letters in module names work.
    #[test]
    fn test_camel_case() {
        let src = TempDir::new().unwrap();
        let pyproject_toml = indoc! {r#"
            [project]
            name = "camelcase"
            version = "1.0.0"

            [build-system]
            requires = ["uv_build>=0.5.15,<0.6"]
            build-backend = "uv_build"

            [tool.uv.build-backend]
            module-name = "camelCase"
            "#
        };
        fs_err::write(src.path().join("pyproject.toml"), pyproject_toml).unwrap();

        fs_err::create_dir_all(src.path().join("src").join("camelCase")).unwrap();
        File::create(src.path().join("src").join("camelCase").join("__init__.py")).unwrap();

        let dist = TempDir::new().unwrap();
        let build1 = build(src.path(), dist.path()).unwrap();

        assert_snapshot!(build1.wheel_contents.join("\n"), @r"
        camelCase/
        camelCase/__init__.py
        camelcase-1.0.0.dist-info/
        camelcase-1.0.0.dist-info/METADATA
        camelcase-1.0.0.dist-info/RECORD
        camelcase-1.0.0.dist-info/WHEEL
        ");

        // Check that an explicit wrong casing fails to build.
        fs_err::write(
            src.path().join("pyproject.toml"),
            pyproject_toml.replace("camelCase", "camel_case"),
        )
        .unwrap();
        let build_err = build(src.path(), dist.path()).unwrap_err();
        let err_message = format_err(&build_err)
            .replace(&src.path().user_display().to_string(), "[TEMP_PATH]")
            .replace('\\', "/");
        assert_snapshot!(
            err_message,
            @"Missing module directory for `camel_case` in `[TEMP_PATH]/src`. Found: `camelCase`"
        );
    }

    #[test]
    fn invalid_stubs_name() {
        let src = TempDir::new().unwrap();
        let pyproject_toml = indoc! {r#"
            [project]
            name = "camelcase"
            version = "1.0.0"

            [build-system]
            requires = ["uv_build>=0.5.15,<0.6"]
            build-backend = "uv_build"

            [tool.uv.build-backend]
            module-name = "django@home-stubs"
            "#
        };
        fs_err::write(src.path().join("pyproject.toml"), pyproject_toml).unwrap();

        let dist = TempDir::new().unwrap();
        let build_err = build(src.path(), dist.path()).unwrap_err();
        let err_message = format_err(&build_err);
        assert_snapshot!(
            err_message,
            @r#"
        Invalid pyproject.toml
          Caused by: TOML parse error at line 10, column 15
           |
        10 | module-name = "django@home-stubs"
           |               ^^^^^^^^^^^^^^^^^^^
        Invalid character `@` at position 7 for identifier `django@home`, expected an underscore or an alphanumeric character
        "#
        );
    }

    /// Stubs packages use a special name and `__init__.pyi`.
    #[test]
    fn stubs_package() {
        let src = TempDir::new().unwrap();
        let pyproject_toml = indoc! {r#"
            [project]
            name = "stuffed-bird-stubs"
            version = "1.0.0"

            [build-system]
            requires = ["uv_build>=0.5.15,<0.6"]
            build-backend = "uv_build"
            "#
        };
        fs_err::write(src.path().join("pyproject.toml"), pyproject_toml).unwrap();
        fs_err::create_dir_all(src.path().join("src").join("stuffed_bird-stubs")).unwrap();
        // That's the wrong file, we're expecting a `__init__.pyi`.
        let regular_init_py = src
            .path()
            .join("src")
            .join("stuffed_bird-stubs")
            .join("__init__.py");
        File::create(&regular_init_py).unwrap();

        let dist = TempDir::new().unwrap();
        let build_err = build(src.path(), dist.path()).unwrap_err();
        let err_message = format_err(&build_err)
            .replace(&src.path().user_display().to_string(), "[TEMP_PATH]")
            .replace('\\', "/");
        assert_snapshot!(
            err_message,
            @"Expected a Python module directory at: `[TEMP_PATH]/src/stuffed_bird-stubs/__init__.pyi`"
        );

        // Create the correct file
        fs_err::remove_file(regular_init_py).unwrap();
        File::create(
            src.path()
                .join("src")
                .join("stuffed_bird-stubs")
                .join("__init__.pyi"),
        )
        .unwrap();

        let build1 = build(src.path(), dist.path()).unwrap();
        assert_snapshot!(build1.wheel_contents.join("\n"), @r"
        stuffed_bird-stubs/
        stuffed_bird-stubs/__init__.pyi
        stuffed_bird_stubs-1.0.0.dist-info/
        stuffed_bird_stubs-1.0.0.dist-info/METADATA
        stuffed_bird_stubs-1.0.0.dist-info/RECORD
        stuffed_bird_stubs-1.0.0.dist-info/WHEEL
        ");

        // Check that setting the name manually works equally.
        let pyproject_toml = indoc! {r#"
            [project]
            name = "stuffed-bird-stubs"
            version = "1.0.0"

            [build-system]
            requires = ["uv_build>=0.5.15,<0.6"]
            build-backend = "uv_build"

            [tool.uv.build-backend]
            module-name = "stuffed_bird-stubs"
            "#
        };
        fs_err::write(src.path().join("pyproject.toml"), pyproject_toml).unwrap();

        let build2 = build(src.path(), dist.path()).unwrap();
        assert_eq!(build1.wheel_contents, build2.wheel_contents);
    }
}

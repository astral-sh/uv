mod metadata;
mod wheel;

pub use metadata::PyProjectToml;
pub use wheel::{build_editable, build_wheel, metadata};

use crate::metadata::{BuildBackendSettings, ValidationError, DEFAULT_EXCLUDES};
use crate::wheel::build_exclude_matcher;
use flate2::write::GzEncoder;
use flate2::Compression;
use fs_err::File;
use globset::{Glob, GlobSet};
use std::fs::FileType;
use std::io;
use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf, StripPrefixError};
use tar::{EntryType, Header};
use thiserror::Error;
use tracing::{debug, trace};
use uv_distribution_filename::{SourceDistExtension, SourceDistFilename};
use uv_fs::Simplified;
use uv_globfilter::{parse_portable_glob, GlobDirFilter, PortableGlobError};
use uv_warnings::warn_user_once;
use walkdir::WalkDir;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Invalid pyproject.toml")]
    Toml(#[from] toml::de::Error),
    #[error("Invalid pyproject.toml")]
    Validation(#[from] ValidationError),
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
    /// [`globset::Error`] shows the glob that failed to parse.
    #[error("Unsupported glob expression in: `{field}`")]
    GlobSet {
        field: String,
        #[source]
        err: globset::Error,
    },
    #[error("`pyproject.toml` must not be excluded from source distribution build")]
    PyprojectTomlExcluded,
    #[error("Failed to walk source tree: `{}`", root.user_display())]
    WalkDir {
        root: PathBuf,
        #[source]
        err: walkdir::Error,
    },
    #[error("Failed to walk source tree")]
    StripPrefix(#[from] StripPrefixError),
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
    fn write_bytes(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error>;

    /// Add a local file.
    fn write_file(&mut self, path: &str, file: &Path) -> Result<(), Error>;

    /// Create a directory.
    fn write_directory(&mut self, directory: &str) -> Result<(), Error>;

    /// Write the `RECORD` file and if applicable, the central directory.
    fn close(self, dist_info_dir: &str) -> Result<(), Error>;
}

struct TarGzWriter {
    path: PathBuf,
    tar: tar::Builder<GzEncoder<File>>,
}

impl TarGzWriter {
    fn new(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        let file = File::create(&path)?;
        let enc = GzEncoder::new(file, Compression::default());
        let tar = tar::Builder::new(enc);
        Ok(Self { path, tar })
    }
}

impl DirectoryWriter for TarGzWriter {
    fn write_bytes(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error> {
        let mut header = Header::new_gnu();
        header.set_size(bytes.len() as u64);
        // Reasonable default to avoid 0o000 permissions, the user's umask will be applied on
        // unpacking.
        header.set_mode(0o644);
        header.set_cksum();
        self.tar
            .append_data(&mut header, path, Cursor::new(bytes))
            .map_err(|err| Error::TarWrite(self.path.clone(), err))?;
        Ok(())
    }

    fn write_file(&mut self, path: &str, file: &Path) -> Result<(), Error> {
        let metadata = fs_err::metadata(file)?;
        let mut header = Header::new_gnu();
        #[cfg(unix)]
        {
            // Preserve for example an executable bit.
            header.set_mode(std::os::unix::fs::MetadataExt::mode(&metadata));
        }
        #[cfg(not(unix))]
        {
            // Reasonable default to avoid 0o000 permissions, the user's umask will be applied on
            // unpacking.
            header.set_mode(0o644);
        }
        header.set_size(metadata.len());
        header.set_cksum();
        let reader = BufReader::new(File::open(file)?);
        self.tar
            .append_data(&mut header, path, reader)
            .map_err(|err| Error::TarWrite(self.path.clone(), err))?;
        Ok(())
    }

    fn write_directory(&mut self, directory: &str) -> Result<(), Error> {
        let mut header = Header::new_gnu();
        // Directories are always executable, which means they can be listed.
        header.set_mode(0o755);
        header.set_entry_type(EntryType::Directory);
        header
            .set_path(directory)
            .map_err(|err| Error::TarWrite(self.path.clone(), err))?;
        header.set_size(0);
        header.set_cksum();
        self.tar
            .append(&header, io::empty())
            .map_err(|err| Error::TarWrite(self.path.clone(), err))?;
        Ok(())
    }

    fn close(mut self, _dist_info_dir: &str) -> Result<(), Error> {
        self.tar
            .finish()
            .map_err(|err| Error::TarWrite(self.path.clone(), err))?;
        Ok(())
    }
}

/// Build a source distribution from the source tree and place it in the output directory.
pub fn build_source_dist(
    source_tree: &Path,
    source_dist_directory: &Path,
    uv_version: &str,
) -> Result<SourceDistFilename, Error> {
    let contents = fs_err::read_to_string(source_tree.join("pyproject.toml"))?;
    let pyproject_toml = PyProjectToml::parse(&contents)?;
    let filename = SourceDistFilename {
        name: pyproject_toml.name().clone(),
        version: pyproject_toml.version().clone(),
        extension: SourceDistExtension::TarGz,
    };
    let source_dist_path = source_dist_directory.join(filename.to_string());
    let writer = TarGzWriter::new(&source_dist_path)?;
    write_source_dist(source_tree, writer, uv_version)?;
    Ok(filename)
}

/// Shared implementation for building and listing a source distribution.
fn write_source_dist(
    source_tree: &Path,
    mut writer: impl DirectoryWriter,
    uv_version: &str,
) -> Result<SourceDistFilename, Error> {
    let contents = fs_err::read_to_string(source_tree.join("pyproject.toml"))?;
    let pyproject_toml = PyProjectToml::parse(&contents)?;
    for warning in pyproject_toml.check_build_system(uv_version) {
        warn_user_once!("{warning}");
    }
    let settings = pyproject_toml
        .settings()
        .cloned()
        .unwrap_or_else(BuildBackendSettings::default);

    let filename = SourceDistFilename {
        name: pyproject_toml.name().clone(),
        version: pyproject_toml.version().clone(),
        extension: SourceDistExtension::TarGz,
    };

    let top_level = format!(
        "{}-{}",
        pyproject_toml.name().as_dist_info_name(),
        pyproject_toml.version()
    );

    let metadata = pyproject_toml.to_metadata(source_tree)?;
    let metadata_email = metadata.core_metadata_format();

    writer.write_bytes(
        &Path::new(&top_level)
            .join("PKG-INFO")
            .portable_display()
            .to_string(),
        metadata_email.as_bytes(),
    )?;

    let (include_matcher, exclude_matcher) = source_dist_matcher(&pyproject_toml, settings)?;

    let mut files_visited = 0;
    for entry in WalkDir::new(source_tree).into_iter().filter_entry(|entry| {
        // TODO(konsti): This should be prettier.
        let relative = entry
            .path()
            .strip_prefix(source_tree)
            .expect("walkdir starts with root");

        // Fast path: Don't descend into a directory that can't be included. This is the most
        // important performance optimization, it avoids descending into directories such as
        // `.venv`. While walkdir is generally cheap, we still avoid traversing large data
        // directories that often exist on the top level of a project. This is especially noticeable
        // on network file systems with high latencies per operation (while contiguous reading may
        // still be fast).
        include_matcher.match_directory(relative) && !exclude_matcher.is_match(relative)
    }) {
        let entry = entry.map_err(|err| Error::WalkDir {
            root: source_tree.to_path_buf(),
            err,
        })?;

        files_visited += 1;
        if files_visited > 10000 {
            warn_user_once!(
                "Visited more than 10,000 files for source distribution build. \
                Consider using more constrained includes or more excludes."
            );
        }
        // TODO(konsti): This should be prettier.
        let relative = entry
            .path()
            .strip_prefix(source_tree)
            .expect("walkdir starts with root");

        if !include_matcher.match_path(relative) || exclude_matcher.is_match(relative) {
            trace!("Excluding: `{}`", relative.user_display());
            continue;
        };

        debug!("Including {}", relative.user_display());
        if entry.file_type().is_dir() {
            writer.write_directory(
                &Path::new(&top_level)
                    .join(relative)
                    .portable_display()
                    .to_string(),
            )?;
        } else if entry.file_type().is_file() {
            writer.write_file(
                &Path::new(&top_level)
                    .join(relative)
                    .portable_display()
                    .to_string(),
                entry.path(),
            )?;
        } else {
            return Err(Error::UnsupportedFileType(
                relative.to_path_buf(),
                entry.file_type(),
            ));
        }
    }
    debug!("Visited {files_visited} files for source dist build");

    writer.close(&top_level)?;

    Ok(filename)
}

/// Build includes and excludes for source tree walking for source dists.
fn source_dist_matcher(
    pyproject_toml: &PyProjectToml,
    settings: BuildBackendSettings,
) -> Result<(GlobDirFilter, GlobSet), Error> {
    // File and directories to include in the source directory
    let mut include_globs = Vec::new();
    let mut includes: Vec<String> = settings.source_include;
    // pyproject.toml is always included.
    includes.push(globset::escape("pyproject.toml"));
    // The wheel must not include any files included by the source distribution (at least until we
    // have files generated in the source dist -> wheel build step).
    let import_path = &settings
        .module_root
        .join(pyproject_toml.name().as_dist_info_name().as_ref())
        .portable_display()
        .to_string();
    includes.push(format!("{}/**", globset::escape(import_path)));
    for include in includes {
        let glob = parse_portable_glob(&include).map_err(|err| Error::PortableGlob {
            field: "tool.uv.build-backend.source-include".to_string(),
            source: err,
        })?;
        include_globs.push(glob.clone());
    }

    // Include the Readme
    if let Some(readme) = pyproject_toml
        .readme()
        .as_ref()
        .and_then(|readme| readme.path())
    {
        trace!("Including readme at: `{}`", readme.user_display());
        include_globs.push(
            Glob::new(&globset::escape(&readme.portable_display().to_string()))
                .expect("escaped globset is parseable"),
        );
    }

    // Include the license files
    for license_files in pyproject_toml.license_files().into_iter().flatten() {
        trace!("Including license files at: `{license_files}`");
        let glob = parse_portable_glob(license_files).map_err(|err| Error::PortableGlob {
            field: "project.license-files".to_string(),
            source: err,
        })?;
        include_globs.push(glob);
    }

    // Include the data files
    for (name, directory) in settings.data.iter() {
        let glob =
            parse_portable_glob(&format!("{}/**", globset::escape(directory))).map_err(|err| {
                Error::PortableGlob {
                    field: format!("tool.uv.build-backend.data.{name}"),
                    source: err,
                }
            })?;
        trace!("Including data ({name}) at: `{directory}`");
        include_globs.push(glob);
    }

    let include_matcher =
        GlobDirFilter::from_globs(&include_globs).map_err(|err| Error::GlobSetTooLarge {
            field: "tool.uv.build-backend.source-include".to_string(),
            source: err,
        })?;

    let mut excludes: Vec<String> = Vec::new();
    if settings.default_excludes {
        excludes.extend(DEFAULT_EXCLUDES.iter().map(ToString::to_string));
    }
    for exclude in settings.source_exclude {
        // Avoid duplicate entries.
        if !excludes.contains(&exclude) {
            excludes.push(exclude);
        }
    }
    debug!("Source dist excludes: {:?}", excludes);
    let exclude_matcher = build_exclude_matcher(excludes)?;
    if exclude_matcher.is_match("pyproject.toml") {
        return Err(Error::PyprojectTomlExcluded);
    }
    Ok((include_matcher, exclude_matcher))
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

    let dist_info_dir = format!(
        "{}-{}.dist-info",
        pyproject_toml.name().as_dist_info_name(),
        pyproject_toml.version()
    );

    // `METADATA` is a mandatory file.
    let current = pyproject_toml
        .to_metadata(source_tree)?
        .core_metadata_format();
    let previous =
        fs_err::read_to_string(metadata_directory.join(&dist_info_dir).join("METADATA"))?;
    if previous != current {
        return Err(Error::InconsistentSteps("METADATA"));
    }

    // `entry_points.txt` is not written if it would be empty.
    let entrypoints_path = metadata_directory
        .join(&dist_info_dir)
        .join("entry_points.txt");
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
    use insta::assert_snapshot;
    use itertools::Itertools;
    use tempfile::TempDir;
    use uv_fs::copy_dir_all;

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

        // Build a source dist from the source tree
        let source_dist_dir = TempDir::new().unwrap();
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
        built_by_uv-0.1.0/third-party-licenses
        built_by_uv-0.1.0/third-party-licenses/PEP-401.txt
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
        "###);

        // Check that we write deterministic wheels.
        let wheel_filename = "built_by_uv-0.1.0-py3-none-any.whl";
        assert_eq!(
            fs_err::read(direct_output_dir.path().join(wheel_filename)).unwrap(),
            fs_err::read(indirect_output_dir.path().join(wheel_filename)).unwrap()
        );
    }
}

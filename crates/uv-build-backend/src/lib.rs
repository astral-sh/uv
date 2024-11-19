mod metadata;

use crate::metadata::{PyProjectToml, ValidationError};
use flate2::write::GzEncoder;
use flate2::Compression;
use fs_err::File;
use globset::{Glob, GlobSetBuilder};
use itertools::Itertools;
use sha2::{Digest, Sha256};
use std::fs::FileType;
use std::io::{BufReader, Cursor, Read, Write};
use std::path::{Path, PathBuf, StripPrefixError};
use std::{io, mem};
use tar::{EntryType, Header};
use thiserror::Error;
use tracing::{debug, trace};
use uv_distribution_filename::{SourceDistExtension, SourceDistFilename, WheelFilename};
use uv_fs::Simplified;
use uv_globfilter::{parse_portable_glob, GlobDirFilter, PortableGlobError};
use walkdir::{DirEntry, WalkDir};
use zip::{CompressionMethod, ZipWriter};

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

/// Allow dispatching between writing to a directory, writing to zip and writing to a `.tar.gz`.
///
/// All paths are string types instead of path types since wheel are portable between platforms.
///
/// Contract: You must call close before dropping to obtain a valid output (dropping is fine in the
/// error case).
trait DirectoryWriter {
    /// Add a file with the given content.
    fn write_bytes(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error>;

    /// Add a file with the given name and return a writer for it.
    fn new_writer<'slf>(&'slf mut self, path: &str) -> Result<Box<dyn Write + 'slf>, Error>;

    /// Add a local file.
    fn write_file(&mut self, path: &str, file: &Path) -> Result<(), Error>;

    /// Create a directory.
    fn write_directory(&mut self, directory: &str) -> Result<(), Error>;

    /// Write the `RECORD` file and if applicable, the central directory.
    fn close(self, dist_info_dir: &str) -> Result<(), Error>;
}

/// Zip archive (wheel) writer.
struct ZipDirectoryWriter {
    writer: ZipWriter<File>,
    compression: CompressionMethod,
    /// The entries in the `RECORD` file.
    record: Vec<RecordEntry>,
}

impl ZipDirectoryWriter {
    /// A wheel writer with deflate compression.
    fn new_wheel(file: File) -> Self {
        Self {
            writer: ZipWriter::new(file),
            compression: CompressionMethod::Deflated,
            record: Vec::new(),
        }
    }

    /// A wheel writer with no (stored) compression.
    ///
    /// Since editables are temporary, we save time be skipping compression and decompression.
    #[expect(dead_code)]
    fn new_editable(file: File) -> Self {
        Self {
            writer: ZipWriter::new(file),
            compression: CompressionMethod::Stored,
            record: Vec::new(),
        }
    }
}

impl DirectoryWriter for ZipDirectoryWriter {
    fn write_bytes(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error> {
        trace!("Adding {}", path);
        let options = zip::write::FileOptions::default().compression_method(self.compression);
        self.writer.start_file(path, options)?;
        self.writer.write_all(bytes)?;

        let hash = format!("{:x}", Sha256::new().chain_update(bytes).finalize());
        self.record.push(RecordEntry {
            path: path.to_string(),
            hash,
            size: bytes.len(),
        });

        Ok(())
    }

    fn new_writer<'slf>(&'slf mut self, path: &str) -> Result<Box<dyn Write + 'slf>, Error> {
        // TODO(konsti): We need to preserve permissions, at least the executable bit.
        self.writer.start_file(
            path,
            zip::write::FileOptions::default().compression_method(self.compression),
        )?;
        Ok(Box::new(&mut self.writer))
    }

    fn write_file(&mut self, path: &str, file: &Path) -> Result<(), Error> {
        trace!("Adding {} from {}", path, file.user_display());
        let mut reader = BufReader::new(File::open(file)?);
        let mut writer = self.new_writer(path)?;
        let record = write_hashed(path, &mut reader, &mut writer)?;
        drop(writer);
        self.record.push(record);
        Ok(())
    }

    fn write_directory(&mut self, directory: &str) -> Result<(), Error> {
        trace!("Adding directory {}", directory);
        let options = zip::write::FileOptions::default().compression_method(self.compression);
        Ok(self.writer.add_directory(directory, options)?)
    }

    /// Write the `RECORD` file and the central directory.
    fn close(mut self, dist_info_dir: &str) -> Result<(), Error> {
        let record_path = format!("{dist_info_dir}/RECORD");
        trace!("Adding {record_path}");
        let record = mem::take(&mut self.record);
        write_record(&mut self.new_writer(&record_path)?, dist_info_dir, record)?;

        trace!("Adding central directory");
        self.writer.finish()?;
        Ok(())
    }
}

struct FilesystemWrite {
    /// The virtualenv or metadata directory that add file paths are relative to.
    root: PathBuf,
    /// The entries in the `RECORD` file.
    record: Vec<RecordEntry>,
}

impl FilesystemWrite {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            record: Vec::new(),
        }
    }
}

/// File system writer.
impl DirectoryWriter for FilesystemWrite {
    fn write_bytes(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error> {
        trace!("Adding {}", path);
        let hash = format!("{:x}", Sha256::new().chain_update(bytes).finalize());
        self.record.push(RecordEntry {
            path: path.to_string(),
            hash,
            size: bytes.len(),
        });

        Ok(fs_err::write(self.root.join(path), bytes)?)
    }

    fn new_writer<'slf>(&'slf mut self, path: &str) -> Result<Box<dyn Write + 'slf>, Error> {
        trace!("Adding {}", path);
        Ok(Box::new(File::create(self.root.join(path))?))
    }

    fn write_file(&mut self, path: &str, file: &Path) -> Result<(), Error> {
        trace!("Adding {} from {}", path, file.user_display());
        let mut reader = BufReader::new(File::open(file)?);
        let mut writer = self.new_writer(path)?;
        let record = write_hashed(path, &mut reader, &mut writer)?;
        drop(writer);
        self.record.push(record);
        Ok(())
    }

    fn write_directory(&mut self, directory: &str) -> Result<(), Error> {
        trace!("Adding directory {}", directory);
        Ok(fs_err::create_dir(self.root.join(directory))?)
    }

    /// Write the `RECORD` file.
    fn close(mut self, dist_info_dir: &str) -> Result<(), Error> {
        let record = mem::take(&mut self.record);
        write_record(
            &mut self.new_writer(&format!("{dist_info_dir}/RECORD"))?,
            dist_info_dir,
            record,
        )?;

        Ok(())
    }
}

/// An entry in the `RECORD` file.
///
/// <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#the-record-file>
struct RecordEntry {
    /// The path to the file relative to the package root.
    ///
    /// While the spec would allow backslashes, we always use portable paths with forward slashes.
    path: String,
    /// The SHA256 of the files.
    hash: String,
    /// The size of the file in bytes.
    size: usize,
}

/// Read the input file and write it both to the hasher and the target file.
///
/// We're implementing this tee-ing manually since there is no sync `InspectReader` or std tee
/// function.
fn write_hashed(
    path: &str,
    reader: &mut dyn Read,
    writer: &mut dyn Write,
) -> Result<RecordEntry, io::Error> {
    let mut hasher = Sha256::new();
    let mut size = 0;
    // 8KB is the default defined in `std::sys_common::io`.
    let mut buffer = vec![0; 8 * 1024];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(read) => read,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };
        if read == 0 {
            // End of file
            break;
        }
        hasher.update(&buffer[..read]);
        writer.write_all(&buffer[..read])?;
        size += read;
    }
    Ok(RecordEntry {
        path: path.to_string(),
        hash: format!("{:x}", hasher.finalize()),
        size,
    })
}

/// Build a wheel from the source tree and place it in the output directory.
pub fn build_wheel(
    source_tree: &Path,
    wheel_dir: &Path,
    metadata_directory: Option<&Path>,
    uv_version: &str,
) -> Result<WheelFilename, Error> {
    let contents = fs_err::read_to_string(source_tree.join("pyproject.toml"))?;
    let pyproject_toml = PyProjectToml::parse(&contents)?;
    pyproject_toml.check_build_system("1.0.0+test");

    check_metadata_directory(source_tree, metadata_directory, &pyproject_toml)?;

    let filename = WheelFilename {
        name: pyproject_toml.name().clone(),
        version: pyproject_toml.version().clone(),
        build_tag: None,
        python_tag: vec!["py3".to_string()],
        abi_tag: vec!["none".to_string()],
        platform_tag: vec!["any".to_string()],
    };

    let wheel_path = wheel_dir.join(filename.to_string());
    debug!("Writing wheel at {}", wheel_path.user_display());
    let mut wheel_writer = ZipDirectoryWriter::new_wheel(File::create(&wheel_path)?);

    debug!("Adding content files to {}", wheel_path.user_display());
    let module_root = pyproject_toml
        .wheel_settings()
        .and_then(|wheel_settings| wheel_settings.module_root.as_deref())
        .unwrap_or_else(|| Path::new("src"));
    if module_root.is_absolute() {
        return Err(Error::AbsoluteModuleRoot(module_root.to_path_buf()));
    }
    let strip_root = source_tree.join(module_root);
    let module_root = strip_root.join(pyproject_toml.name().as_dist_info_name().as_ref());
    if !module_root.join("__init__.py").is_file() {
        return Err(Error::MissingModule(module_root));
    }
    for entry in WalkDir::new(module_root) {
        let entry = entry.map_err(|err| Error::WalkDir {
            root: source_tree.to_path_buf(),
            err,
        })?;

        let relative_path = entry
            .path()
            .strip_prefix(&strip_root)
            .expect("walkdir starts with root")
            .user_display()
            .to_string();

        debug!("Adding to wheel: `{relative_path}`");

        if entry.file_type().is_dir() {
            wheel_writer.write_directory(&relative_path)?;
        } else if entry.file_type().is_file() {
            wheel_writer.write_file(&relative_path, entry.path())?;
        } else {
            // TODO(konsti): We may want to support symlinks, there is support for installing them.
            return Err(Error::UnsupportedFileType(
                entry.path().to_path_buf(),
                entry.file_type(),
            ));
        }
    }

    // Add the license files
    if let Some(license_files) = &pyproject_toml.license_files() {
        debug!("Adding license files");
        let license_dir = format!(
            "{}-{}.dist-info/licenses/",
            pyproject_toml.name().as_dist_info_name(),
            pyproject_toml.version()
        );

        wheel_subdir_from_globs(
            source_tree,
            &license_dir,
            license_files,
            &mut wheel_writer,
            "project.license-files",
        )?;
    }

    // Add the data files
    for (name, directory) in pyproject_toml
        .wheel_settings()
        .and_then(|wheel_settings| wheel_settings.data.clone())
        .unwrap_or_default()
        .iter()
    {
        debug!("Adding {name} data files from: `{directory}`");
        let data_dir = format!(
            "{}-{}.data/{}/",
            pyproject_toml.name().as_dist_info_name(),
            pyproject_toml.version(),
            name
        );

        wheel_subdir_from_globs(
            &source_tree.join(directory),
            &data_dir,
            &["**".to_string()],
            &mut wheel_writer,
            &format!("tool.uv.wheel.data.{name}"),
        )?;
    }

    debug!("Adding metadata files to: `{}`", wheel_path.user_display());
    let dist_info_dir = write_dist_info(
        &mut wheel_writer,
        &pyproject_toml,
        &filename,
        source_tree,
        uv_version,
    )?;
    wheel_writer.close(&dist_info_dir)?;

    Ok(filename)
}

/// Build a wheel from the source tree and place it in the output directory.
pub fn build_editable(
    source_tree: &Path,
    wheel_dir: &Path,
    metadata_directory: Option<&Path>,
    uv_version: &str,
) -> Result<WheelFilename, Error> {
    let contents = fs_err::read_to_string(source_tree.join("pyproject.toml"))?;
    let pyproject_toml = PyProjectToml::parse(&contents)?;
    pyproject_toml.check_build_system("1.0.0+test");

    check_metadata_directory(source_tree, metadata_directory, &pyproject_toml)?;

    let filename = WheelFilename {
        name: pyproject_toml.name().clone(),
        version: pyproject_toml.version().clone(),
        build_tag: None,
        python_tag: vec!["py3".to_string()],
        abi_tag: vec!["none".to_string()],
        platform_tag: vec!["any".to_string()],
    };

    let wheel_path = wheel_dir.join(filename.to_string());
    debug!("Writing wheel at {}", wheel_path.user_display());
    let mut wheel_writer = ZipDirectoryWriter::new_wheel(File::create(&wheel_path)?);

    debug!("Adding pth file to {}", wheel_path.user_display());
    let module_root = pyproject_toml
        .wheel_settings()
        .and_then(|wheel_settings| wheel_settings.module_root.as_deref())
        .unwrap_or_else(|| Path::new("src"));
    if module_root.is_absolute() {
        return Err(Error::AbsoluteModuleRoot(module_root.to_path_buf()));
    }
    let src_root = source_tree.join(module_root);
    let module_root = src_root.join(pyproject_toml.name().as_dist_info_name().as_ref());
    if !module_root.join("__init__.py").is_file() {
        return Err(Error::MissingModule(module_root));
    }
    wheel_writer.write_bytes(
        &format!("{}.pth", pyproject_toml.name().as_dist_info_name()),
        src_root.as_os_str().as_encoded_bytes(),
    )?;

    debug!("Adding metadata files to: `{}`", wheel_path.user_display());
    let dist_info_dir = write_dist_info(
        &mut wheel_writer,
        &pyproject_toml,
        &filename,
        source_tree,
        uv_version,
    )?;
    wheel_writer.close(&dist_info_dir)?;

    Ok(filename)
}

/// Add the files and directories matching from the source tree matching any of the globs in the
/// wheel subdirectory.
fn wheel_subdir_from_globs(
    src: &Path,
    target: &str,
    globs: &[String],
    wheel_writer: &mut ZipDirectoryWriter,
    // For error messages
    globs_field: &str,
) -> Result<(), Error> {
    let license_files_globs: Vec<_> = globs
        .iter()
        .map(|license_files| {
            trace!("Including license files at: `{license_files}`");
            parse_portable_glob(license_files)
        })
        .collect::<Result<_, _>>()
        .map_err(|err| Error::PortableGlob {
            field: globs_field.to_string(),
            source: err,
        })?;
    let license_files_matcher =
        GlobDirFilter::from_globs(&license_files_globs).map_err(|err| Error::GlobSetTooLarge {
            field: globs_field.to_string(),
            source: err,
        })?;

    wheel_writer.write_directory(target)?;

    for entry in WalkDir::new(src).into_iter().filter_entry(|entry| {
        // TODO(konsti): This should be prettier.
        let relative = entry
            .path()
            .strip_prefix(src)
            .expect("walkdir starts with root");

        // Fast path: Don't descend into a directory that can't be included.
        license_files_matcher.match_directory(relative)
    }) {
        let entry = entry.map_err(|err| Error::WalkDir {
            root: src.to_path_buf(),
            err,
        })?;
        // TODO(konsti): This should be prettier.
        let relative = entry
            .path()
            .strip_prefix(src)
            .expect("walkdir starts with root");

        if !license_files_matcher.match_path(relative) {
            trace!("Excluding {}", relative.user_display());
            continue;
        };

        let relative_licenses = Path::new(target)
            .join(relative)
            .portable_display()
            .to_string();

        if entry.file_type().is_dir() {
            wheel_writer.write_directory(&relative_licenses)?;
        } else if entry.file_type().is_file() {
            debug!("Adding {} file: `{}`", globs_field, relative.user_display());
            wheel_writer.write_file(&relative_licenses, entry.path())?;
        } else {
            // TODO(konsti): We may want to support symlinks, there is support for installing them.
            return Err(Error::UnsupportedFileType(
                entry.path().to_path_buf(),
                entry.file_type(),
            ));
        }
    }
    Ok(())
}

/// TODO(konsti): Wire this up with actual settings and remove this struct.
///
/// To select which files to include in the source distribution, we first add the includes, then
/// remove the excludes from that.
pub struct SourceDistSettings {
    /// Glob expressions which files and directories to include in the source distribution.
    ///
    /// Includes are anchored, which means that `pyproject.toml` includes only
    /// `<project root>/pyproject.toml`. Use for example `assets/**/sample.csv` to include for all
    /// `sample.csv` files in `<project root>/assets` or any child directory. To recursively include
    /// all files under a directory, use a `/**` suffix, e.g. `src/**`. For performance and
    /// reproducibility, avoid unanchored matches such as `**/sample.csv`.
    ///
    /// The glob syntax is the reduced portable glob from
    /// [PEP 639](https://peps.python.org/pep-0639/#add-license-FILES-key).
    include: Vec<String>,
    /// Glob expressions which files and directories to exclude from the previous source
    /// distribution includes.
    ///
    /// Excludes are not anchored, which means that `__pycache__` excludes all directories named
    /// `__pycache__` and it's children anywhere. To anchor a directory, use a `/` prefix, e.g.,
    /// `/dist` will exclude only `<project root>/dist`.
    ///
    /// The glob syntax is the reduced portable glob from
    /// [PEP 639](https://peps.python.org/pep-0639/#add-license-FILES-key).
    exclude: Vec<String>,
}

impl Default for SourceDistSettings {
    fn default() -> Self {
        Self {
            include: vec!["src/**".to_string(), "pyproject.toml".to_string()],
            exclude: vec![
                "__pycache__".to_string(),
                "*.pyc".to_string(),
                "*.pyo".to_string(),
            ],
        }
    }
}

/// Build a source distribution from the source tree and place it in the output directory.
pub fn build_source_dist(
    source_tree: &Path,
    source_dist_directory: &Path,
    settings: SourceDistSettings,
    uv_version: &str,
) -> Result<SourceDistFilename, Error> {
    let contents = fs_err::read_to_string(source_tree.join("pyproject.toml"))?;
    let pyproject_toml = PyProjectToml::parse(&contents)?;
    pyproject_toml.check_build_system(uv_version);

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

    let source_dist_path = source_dist_directory.join(filename.to_string());
    let tar_gz = File::create(&source_dist_path)?;
    let enc = GzEncoder::new(tar_gz, Compression::default());
    let mut tar = tar::Builder::new(enc);

    let metadata = pyproject_toml.to_metadata(source_tree)?;
    let metadata_email = metadata.core_metadata_format();

    let mut header = Header::new_gnu();
    header.set_size(metadata_email.bytes().len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(
        &mut header,
        Path::new(&top_level).join("PKG-INFO"),
        Cursor::new(metadata_email),
    )
    .map_err(|err| Error::TarWrite(source_dist_path.clone(), err))?;

    // The user (or default) includes
    let mut include_globs = Vec::new();
    for include in settings.include {
        let glob = parse_portable_glob(&include).map_err(|err| Error::PortableGlob {
            field: "tool.uv.source-dist.include".to_string(),
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
    for (name, directory) in pyproject_toml
        .wheel_settings()
        .and_then(|wheel_settings| wheel_settings.data.clone())
        .unwrap_or_default()
        .iter()
    {
        let glob =
            parse_portable_glob(&format!("{}/**", globset::escape(directory))).map_err(|err| {
                Error::PortableGlob {
                    field: format!("tool.uv.wheel.data.{name}"),
                    source: err,
                }
            })?;
        trace!("Including data ({name}) at: `{directory}`");
        include_globs.push(glob);
    }

    let include_matcher =
        GlobDirFilter::from_globs(&include_globs).map_err(|err| Error::GlobSetTooLarge {
            field: "tool.uv.source-dist.include".to_string(),
            source: err,
        })?;

    let mut exclude_builder = GlobSetBuilder::new();
    for exclude in settings.exclude {
        // Excludes are unanchored
        let exclude = if let Some(exclude) = exclude.strip_prefix("/") {
            exclude.to_string()
        } else {
            format!("**/{exclude}").to_string()
        };
        let glob = parse_portable_glob(&exclude).map_err(|err| Error::PortableGlob {
            field: "tool.uv.source-dist.exclude".to_string(),
            source: err,
        })?;
        exclude_builder.add(glob);
    }
    let exclude_matcher = exclude_builder
        .build()
        .map_err(|err| Error::GlobSetTooLarge {
            field: "tool.uv.source-dist.exclude".to_string(),
            source: err,
        })?;

    // TODO(konsti): Add files linked by pyproject.toml

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
        // TODO(konsti): This should be prettier.
        let relative = entry
            .path()
            .strip_prefix(source_tree)
            .expect("walkdir starts with root");

        if !include_matcher.match_path(relative) || exclude_matcher.is_match(relative) {
            trace!("Excluding {}", relative.user_display());
            continue;
        };

        add_source_dist_entry(&mut tar, &entry, &top_level, &source_dist_path, relative)?;
    }

    tar.finish()
        .map_err(|err| Error::TarWrite(source_dist_path.clone(), err))?;

    Ok(filename)
}

/// Add a file or a directory to a source distribution.
fn add_source_dist_entry(
    tar: &mut tar::Builder<GzEncoder<File>>,
    entry: &DirEntry,
    top_level: &str,
    source_dist_path: &Path,
    relative: &Path,
) -> Result<(), Error> {
    debug!("Including {}", relative.user_display());

    let metadata = fs_err::metadata(entry.path())?;
    let mut header = Header::new_gnu();
    #[cfg(unix)]
    {
        header.set_mode(std::os::unix::fs::MetadataExt::mode(&metadata));
    }
    #[cfg(not(unix))]
    {
        header.set_mode(0o644);
    }

    if entry.file_type().is_dir() {
        header.set_entry_type(EntryType::Directory);
        header
            .set_path(Path::new(&top_level).join(relative))
            .map_err(|err| Error::TarWrite(source_dist_path.to_path_buf(), err))?;
        header.set_size(0);
        header.set_cksum();
        tar.append(&header, io::empty())
            .map_err(|err| Error::TarWrite(source_dist_path.to_path_buf(), err))?;
        Ok(())
    } else if entry.file_type().is_file() {
        header.set_size(metadata.len());
        header.set_cksum();
        tar.append_data(
            &mut header,
            Path::new(&top_level).join(relative),
            BufReader::new(File::open(entry.path())?),
        )
        .map_err(|err| Error::TarWrite(source_dist_path.to_path_buf(), err))?;
        Ok(())
    } else {
        Err(Error::UnsupportedFileType(
            relative.to_path_buf(),
            entry.file_type(),
        ))
    }
}

/// Write the dist-info directory to the output directory without building the wheel.
pub fn metadata(
    source_tree: &Path,
    metadata_directory: &Path,
    uv_version: &str,
) -> Result<String, Error> {
    let contents = fs_err::read_to_string(source_tree.join("pyproject.toml"))?;
    let pyproject_toml = PyProjectToml::parse(&contents)?;
    pyproject_toml.check_build_system(uv_version);

    let filename = WheelFilename {
        name: pyproject_toml.name().clone(),
        version: pyproject_toml.version().clone(),
        build_tag: None,
        python_tag: vec!["py3".to_string()],
        abi_tag: vec!["none".to_string()],
        platform_tag: vec!["any".to_string()],
    };

    debug!(
        "Writing metadata files to {}",
        metadata_directory.user_display()
    );
    let mut wheel_writer = FilesystemWrite::new(metadata_directory);
    let dist_info_dir = write_dist_info(
        &mut wheel_writer,
        &pyproject_toml,
        &filename,
        source_tree,
        uv_version,
    )?;
    wheel_writer.close(&dist_info_dir)?;

    Ok(dist_info_dir)
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

/// Add `METADATA` and `entry_points.txt` to the dist-info directory.
///
/// Returns the name of the dist-info directory.
fn write_dist_info(
    writer: &mut dyn DirectoryWriter,
    pyproject_toml: &PyProjectToml,
    filename: &WheelFilename,
    root: &Path,
    uv_version: &str,
) -> Result<String, Error> {
    let dist_info_dir = format!(
        "{}-{}.dist-info",
        pyproject_toml.name().as_dist_info_name(),
        pyproject_toml.version()
    );

    writer.write_directory(&dist_info_dir)?;

    // Add `WHEEL`.
    let wheel_info = wheel_info(filename, uv_version);
    writer.write_bytes(&format!("{dist_info_dir}/WHEEL"), wheel_info.as_bytes())?;

    // Add `entry_points.txt`.
    if let Some(entrypoint) = pyproject_toml.to_entry_points()? {
        writer.write_bytes(
            &format!("{dist_info_dir}/entry_points.txt"),
            entrypoint.as_bytes(),
        )?;
    }

    // Add `METADATA`.
    let metadata = pyproject_toml.to_metadata(root)?.core_metadata_format();
    writer.write_bytes(&format!("{dist_info_dir}/METADATA"), metadata.as_bytes())?;

    // `RECORD` is added on closing.

    Ok(dist_info_dir)
}

/// Returns the `WHEEL` file contents.
fn wheel_info(filename: &WheelFilename, uv_version: &str) -> String {
    // https://packaging.python.org/en/latest/specifications/binary-distribution-format/#file-contents
    let mut wheel_info = vec![
        ("Wheel-Version", "1.0".to_string()),
        ("Generator", format!("uv {uv_version}")),
        ("Root-Is-Purelib", "true".to_string()),
    ];
    for python_tag in &filename.python_tag {
        for abi_tag in &filename.abi_tag {
            for platform_tag in &filename.platform_tag {
                wheel_info.push(("Tag", format!("{python_tag}-{abi_tag}-{platform_tag}")));
            }
        }
    }
    wheel_info
        .into_iter()
        .map(|(key, value)| format!("{key}: {value}"))
        .join("\n")
}

/// Write the `RECORD` file.
///
/// <https://packaging.python.org/en/latest/specifications/recording-installed-packages/#the-record-file>
fn write_record(
    writer: &mut dyn Write,
    dist_info_dir: &str,
    record: Vec<RecordEntry>,
) -> Result<(), Error> {
    let mut record_writer = csv::Writer::from_writer(writer);
    for entry in record {
        record_writer.write_record(&[
            entry.path,
            format!("sha256={}", entry.hash),
            entry.size.to_string(),
        ])?;
    }

    // We can't compute the hash or size for RECORD without modifying it at the same time.
    record_writer.write_record(&[
        format!("{dist_info_dir}/RECORD"),
        String::new(),
        String::new(),
    ])?;
    record_writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::bufread::GzDecoder;
    use insta::assert_snapshot;
    use std::str::FromStr;
    use tempfile::TempDir;
    use uv_fs::copy_dir_all;
    use uv_normalize::PackageName;
    use uv_pep440::Version;

    #[test]
    fn test_wheel() {
        let filename = WheelFilename {
            name: PackageName::from_str("foo").unwrap(),
            version: Version::from_str("1.2.3").unwrap(),
            build_tag: None,
            python_tag: vec!["py2".to_string(), "py3".to_string()],
            abi_tag: vec!["none".to_string()],
            platform_tag: vec!["any".to_string()],
        };

        assert_snapshot!(wheel_info(&filename, "1.0.0+test"), @r"
        Wheel-Version: 1.0
        Generator: uv 1.0.0+test
        Root-Is-Purelib: true
        Tag: py2-none-any
        Tag: py3-none-any
    ");
    }

    #[test]
    fn test_record() {
        let record = vec![RecordEntry {
            path: "built_by_uv/__init__.py".to_string(),
            hash: "89f869e53a3a0061a52c0233e6442d4d72de80a8a2d3406d9ea0bfd397ed7865".to_string(),
            size: 37,
        }];

        let mut writer = Vec::new();
        write_record(&mut writer, "built_by_uv-0.1.0", record).unwrap();
        assert_snapshot!(String::from_utf8(writer).unwrap(), @r"
            built_by_uv/__init__.py,sha256=89f869e53a3a0061a52c0233e6442d4d72de80a8a2d3406d9ea0bfd397ed7865,37
            built_by_uv-0.1.0/RECORD,,
        ");
    }

    /// Snapshot all files from the prepare metadata hook.
    #[test]
    fn test_prepare_metadata() {
        let metadata_dir = TempDir::new().unwrap();
        let built_by_uv = Path::new("../../scripts/packages/built-by-uv");
        metadata(built_by_uv, metadata_dir.path(), "1.0.0+test").unwrap();

        let mut files: Vec<_> = WalkDir::new(metadata_dir.path())
            .into_iter()
            .map(|entry| {
                entry
                    .unwrap()
                    .path()
                    .strip_prefix(metadata_dir.path())
                    .expect("walkdir starts with root")
                    .portable_display()
                    .to_string()
            })
            .filter(|path| !path.is_empty())
            .collect();
        files.sort();
        assert_snapshot!(files.join("\n"), @r"
        built_by_uv-0.1.0.dist-info
        built_by_uv-0.1.0.dist-info/METADATA
        built_by_uv-0.1.0.dist-info/RECORD
        built_by_uv-0.1.0.dist-info/WHEEL
        ");

        let metadata_file = metadata_dir
            .path()
            .join("built_by_uv-0.1.0.dist-info/METADATA");
        assert_snapshot!(fs_err::read_to_string(metadata_file).unwrap(), @r###"
    Metadata-Version: 2.4
    Name: built-by-uv
    Version: 0.1.0
    Summary: A package to be built with the uv build backend that uses all features exposed by the build backend
    Requires-Dist: anyio>=4,<5
    Requires-Python: >=3.12
    Description-Content-Type: text/markdown

    # built_by_uv

    A package to be built with the uv build backend that uses all features exposed by the build backend.
    "###);

        let record_file = metadata_dir
            .path()
            .join("built_by_uv-0.1.0.dist-info/RECORD");
        assert_snapshot!(fs_err::read_to_string(record_file).unwrap(), @r###"
    built_by_uv-0.1.0.dist-info/WHEEL,sha256=3da1bfa0e8fd1b6cc246aa0b2b44a35815596c600cb485c39a6f8c106c3d5a8d,83
    built_by_uv-0.1.0.dist-info/METADATA,sha256=acb91f5a18cb53fa57b45eb4590ea13195a774c856a9dd8cf27cc5435d6451b6,372
    built_by_uv-0.1.0.dist-info/RECORD,,
    "###);

        let wheel_file = metadata_dir
            .path()
            .join("built_by_uv-0.1.0.dist-info/WHEEL");
        assert_snapshot!(fs_err::read_to_string(wheel_file).unwrap(), @r###"
        Wheel-Version: 1.0
        Generator: uv 1.0.0+test
        Root-Is-Purelib: true
        Tag: py3-none-any
    "###);
    }

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
        build_source_dist(
            src.path(),
            source_dist_dir.path(),
            SourceDistSettings::default(),
            "1.0.0+test",
        )
        .unwrap();

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

        // Check that we write deterministic wheels.
        let wheel_filename = "built_by_uv-0.1.0-py3-none-any.whl";
        assert_eq!(
            fs_err::read(direct_output_dir.path().join(wheel_filename)).unwrap(),
            fs_err::read(indirect_output_dir.path().join(wheel_filename)).unwrap()
        );

        // Check the contained files and directories
        assert_snapshot!(source_dist_contents.iter().map(|path| path.replace('\\', "/")).join("\n"), @r"
            built_by_uv-0.1.0/LICENSE-APACHE
            built_by_uv-0.1.0/LICENSE-MIT
            built_by_uv-0.1.0/PKG-INFO
            built_by_uv-0.1.0/README.md
            built_by_uv-0.1.0/assets/data.csv
            built_by_uv-0.1.0/header/built_by_uv.h
            built_by_uv-0.1.0/pyproject.toml
            built_by_uv-0.1.0/scripts/whoami.sh
            built_by_uv-0.1.0/src/built_by_uv
            built_by_uv-0.1.0/src/built_by_uv/__init__.py
            built_by_uv-0.1.0/src/built_by_uv/arithmetic
            built_by_uv-0.1.0/src/built_by_uv/arithmetic/__init__.py
            built_by_uv-0.1.0/src/built_by_uv/arithmetic/circle.py
            built_by_uv-0.1.0/src/built_by_uv/arithmetic/pi.txt
            built_by_uv-0.1.0/third-party-licenses/PEP-401.txt
        ");

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

        assert_snapshot!(indirect_wheel_contents.iter().map(|path| path.replace('\\', "/")).join("\n"), @r"
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
            built_by_uv-0.1.0.dist-info/licenses/third-party-licenses/PEP-401.txt
            built_by_uv/
            built_by_uv/__init__.py
            built_by_uv/arithmetic/
            built_by_uv/arithmetic/__init__.py
            built_by_uv/arithmetic/circle.py
            built_by_uv/arithmetic/pi.txt
        ");
    }
}

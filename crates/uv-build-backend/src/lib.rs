mod metadata;
mod pep639_glob;

use crate::metadata::{PyProjectToml, ValidationError};
use crate::pep639_glob::Pep639GlobError;
use fs_err::File;
use glob::{GlobError, PatternError};
use itertools::Itertools;
use sha2::{Digest, Sha256};
use std::fs::FileType;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf, StripPrefixError};
use std::{io, mem};
use thiserror::Error;
use tracing::{debug, trace};
use uv_distribution_filename::WheelFilename;
use uv_fs::Simplified;
use walkdir::WalkDir;
use zip::{CompressionMethod, ZipWriter};

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Invalid pyproject.toml")]
    Toml(#[from] toml::de::Error),
    #[error("Invalid pyproject.toml")]
    Validation(#[from] ValidationError),
    #[error("Invalid `project.license-files` glob expression: `{0}`")]
    Pep639Glob(String, #[source] Pep639GlobError),
    #[error("The `project.license-files` entry is not a valid glob pattern: `{0}`")]
    Pattern(String, #[source] PatternError),
    /// [`GlobError`] is a wrapped io error.
    #[error(transparent)]
    Glob(#[from] GlobError),
    #[error("Failed to walk source tree: `{}`", root.user_display())]
    WalkDir {
        root: PathBuf,
        #[source]
        err: walkdir::Error,
    },
    #[error("Non-UTF-8 paths are not supported: `{}`", _0.user_display())]
    NotUtf8Path(PathBuf),
    #[error("Failed to walk source tree")]
    StripPrefix(#[from] StripPrefixError),
    #[error("Unsupported file type: {0:?}")]
    UnsupportedFileType(FileType),
    #[error("Failed to write wheel zip archive")]
    Zip(#[from] zip::result::ZipError),
    #[error("Failed to write RECORD file")]
    Csv(#[from] csv::Error),
    #[error("Expected a Python module with an `__init__.py` at: `{}`", _0.user_display())]
    MissingModule(PathBuf),
    #[error("Inconsistent metadata between prepare and build step: `{0}`")]
    InconsistentSteps(&'static str),
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
pub fn build(
    source_tree: &Path,
    wheel_dir: &Path,
    metadata_directory: Option<&Path>,
) -> Result<WheelFilename, Error> {
    let contents = fs_err::read_to_string(source_tree.join("pyproject.toml"))?;
    let pyproject_toml = PyProjectToml::parse(&contents)?;
    pyproject_toml.check_build_system();

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
    let strip_root = source_tree.join("src");
    let module_root = strip_root.join(pyproject_toml.name().as_dist_info_name().as_ref());
    if !module_root.join("__init__.py").is_file() {
        return Err(Error::MissingModule(module_root));
    }
    for entry in WalkDir::new(module_root) {
        let entry = entry.map_err(|err| Error::WalkDir {
            root: source_tree.to_path_buf(),
            err,
        })?;

        let relative_path = entry.path().strip_prefix(&strip_root)?;
        let relative_path_str = relative_path
            .to_str()
            .ok_or_else(|| Error::NotUtf8Path(relative_path.to_path_buf()))?;
        if entry.file_type().is_dir() {
            wheel_writer.write_directory(relative_path_str)?;
        } else if entry.file_type().is_file() {
            wheel_writer.write_file(relative_path_str, entry.path())?;
        } else {
            // TODO(konsti): We may want to support symlinks, there is support for installing them.
            return Err(Error::UnsupportedFileType(entry.file_type()));
        }

        entry.path();
    }

    debug!("Adding metadata files to {}", wheel_path.user_display());
    let dist_info_dir =
        write_dist_info(&mut wheel_writer, &pyproject_toml, &filename, source_tree)?;
    wheel_writer.close(&dist_info_dir)?;

    Ok(filename)
}

/// Write the dist-info directory to the output directory without building the wheel.
pub fn metadata(source_tree: &Path, metadata_directory: &Path) -> Result<String, Error> {
    let contents = fs_err::read_to_string(source_tree.join("pyproject.toml"))?;
    let pyproject_toml = PyProjectToml::parse(&contents)?;
    pyproject_toml.check_build_system();

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
    let dist_info_dir =
        write_dist_info(&mut wheel_writer, &pyproject_toml, &filename, source_tree)?;
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
) -> Result<String, Error> {
    let dist_info_dir = format!(
        "{}-{}.dist-info",
        pyproject_toml.name().as_dist_info_name(),
        pyproject_toml.version()
    );

    writer.write_directory(&dist_info_dir)?;

    // Add `WHEEL`.
    let wheel_info = wheel_info(filename);
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
fn wheel_info(filename: &WheelFilename) -> String {
    // https://packaging.python.org/en/latest/specifications/binary-distribution-format/#file-contents
    let mut wheel_info = vec![
        ("Wheel-Version", "1.0".to_string()),
        ("Generator", format!("uv {}", uv_version::version())),
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
    use insta::{assert_snapshot, with_settings};
    use std::str::FromStr;
    use tempfile::TempDir;
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

        with_settings!({
            filters => [(uv_version::version(), "[VERSION]")],
        }, {
            assert_snapshot!(wheel_info(&filename), @r"
                Wheel-Version: 1.0
                Generator: uv [VERSION]
                Root-Is-Purelib: true
                Tag: py2-none-any
                Tag: py3-none-any
            ");
        });
    }

    #[test]
    fn test_record() {
        let record = vec![RecordEntry {
            path: "uv_backend/__init__.py".to_string(),
            hash: "89f869e53a3a0061a52c0233e6442d4d72de80a8a2d3406d9ea0bfd397ed7865".to_string(),
            size: 37,
        }];

        let mut writer = Vec::new();
        write_record(&mut writer, "uv_backend-0.1.0", record).unwrap();
        assert_snapshot!(String::from_utf8(writer).unwrap(), @r"
            uv_backend/__init__.py,sha256=89f869e53a3a0061a52c0233e6442d4d72de80a8a2d3406d9ea0bfd397ed7865,37
            uv_backend-0.1.0/RECORD,,
        ");
    }

    /// Check that we write deterministic wheels.
    #[test]
    fn test_determinism() {
        let temp1 = TempDir::new().unwrap();
        let uv_backend = Path::new("../../scripts/packages/uv_backend");
        build(uv_backend, temp1.path(), None).unwrap();

        // Touch the file to check that we don't serialize the last modified date.
        fs_err::write(
            uv_backend.join("src/uv_backend/__init__.py"),
            "def greet():\n    print(\"Hello 👋\")\n",
        )
        .unwrap();

        let temp2 = TempDir::new().unwrap();
        build(uv_backend, temp2.path(), None).unwrap();

        let wheel_filename = "uv_backend-0.1.0-py3-none-any.whl";
        assert_eq!(
            fs_err::read(temp1.path().join(wheel_filename)).unwrap(),
            fs_err::read(temp2.path().join(wheel_filename)).unwrap()
        );
    }

    /// Snapshot all files from the prepare metadata hook.
    #[test]
    fn test_prepare_metadata() {
        let metadata_dir = TempDir::new().unwrap();
        let uv_backend = Path::new("../../scripts/packages/uv_backend");
        metadata(uv_backend, metadata_dir.path()).unwrap();

        let mut files: Vec<_> = WalkDir::new(metadata_dir.path())
            .into_iter()
            .map(|entry| {
                entry
                    .unwrap()
                    .path()
                    .strip_prefix(metadata_dir.path())
                    .unwrap()
                    .portable_display()
                    .to_string()
            })
            .filter(|path| !path.is_empty())
            .collect();
        files.sort();
        assert_snapshot!(files.join("\n"), @r"
        uv_backend-0.1.0.dist-info
        uv_backend-0.1.0.dist-info/METADATA
        uv_backend-0.1.0.dist-info/RECORD
        uv_backend-0.1.0.dist-info/WHEEL
        ");

        let metadata_file = metadata_dir
            .path()
            .join("uv_backend-0.1.0.dist-info/METADATA");
        assert_snapshot!(fs_err::read_to_string(metadata_file).unwrap(), @r###"
        Metadata-Version: 2.3
        Name: uv-backend
        Version: 0.1.0
        Summary: Add your description here
        Requires-Python: >=3.12
        Description-Content-Type: text/markdown

        # uv_backend

        A simple package to be built with the uv build backend.
        "###);

        let record_file = metadata_dir
            .path()
            .join("uv_backend-0.1.0.dist-info/RECORD");
        assert_snapshot!(fs_err::read_to_string(record_file).unwrap(), @r###"
        uv_backend-0.1.0.dist-info/WHEEL,sha256=70ce44709b6a53e0d0c5a6755b0290179697020f1f867e794f26154fe4825738,79
        uv_backend-0.1.0.dist-info/METADATA,sha256=e4a0d390317d7182f65ea978254c71ed283e0a4242150cf1c99a694b113ff68d,224
        uv_backend-0.1.0.dist-info/RECORD,,
        "###);

        let wheel_file = metadata_dir.path().join("uv_backend-0.1.0.dist-info/WHEEL");
        let filters = vec![(uv_version::version(), "[VERSION]")];
        with_settings!({
            filters => filters
        }, {
            assert_snapshot!(fs_err::read_to_string(wheel_file).unwrap(), @r###"
                Wheel-Version: 1.0
                Generator: uv [VERSION]
                Root-Is-Purelib: true
                Tag: py3-none-any
            "###);
        });
    }
}

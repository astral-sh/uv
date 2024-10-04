mod metadata;
mod pep639_glob;

use crate::metadata::{PyProjectToml, ValidationError};
use crate::pep639_glob::Pep639GlobError;
use async_zip::base::write::ZipFileWriter;
use async_zip::error::ZipError;
use async_zip::{Compression, ZipEntryBuilder, ZipString};
use glob::{GlobError, PatternError};
use sha2::{Digest, Sha256};
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio_util::compat::FuturesAsyncWriteCompatExt;
use tokio_util::io::InspectReader;
use uv_distribution_filename::WheelFilename;
use uv_fs::Simplified;
use walkdir::WalkDir;

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

    // Errors in the writers.
    #[error("Failed to write wheel zip archive")]
    Zip(#[from] ZipError),
    #[error("Failed to write `{src}` to zip archive `{}`", zip.user_display())]
    ZipCopy {
        src: String,
        zip: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to write `{}` to `{}`", src.user_display(), dst.user_display())]
    IoCopy {
        src: PathBuf,
        dst: PathBuf,
        #[source]
        err: io::Error,
    },
    #[error("Failed to write to `{}`", dst.user_display())]
    IoWrite {
        dst: PathBuf,
        #[source]
        err: io::Error,
    },
}

/// Allow dispatching between writing to a directory, writing to zip and writing to a `.tar.gz`.
///
/// Contract: You must call close before dropping to obtain a valid output (dropping is fine in the
/// error case).
trait AsyncDirectoryWrite: Sized {
    async fn write_bytes(
        &mut self,
        directory: &Path,
        filename: &str,
        bytes: &[u8],
    ) -> Result<(), Error>;

    async fn write_file(
        &mut self,
        directory: &Path,
        filename: &str,
        file: &Path,
    ) -> Result<(), Error>;

    #[allow(clippy::unused_async)] // https://github.com/rust-lang/rust-clippy/issues/11660
    async fn close(self) -> Result<(), Error> {
        Ok(())
    }
}

/// Zip archive (wheel) writer.
struct AsyncZipWriter {
    writer: ZipFileWriter<tokio_util::compat::Compat<fs_err::tokio::File>>,
    /// For better error messages.
    zip_path: PathBuf,
    /// The entries in the `RECORD` file.
    record: Vec<RecordEntry>,
}

impl AsyncDirectoryWrite for AsyncZipWriter {
    async fn write_bytes(
        &mut self,
        directory: &Path,
        filename: &str,
        bytes: &[u8],
    ) -> Result<(), Error> {
        self.writer
            .write_entry_whole(
                ZipEntryBuilder::new(
                    ZipString::from(format!("{}/{}", directory.display(), filename)),
                    // TODO(konsti): Editables use stored.
                    Compression::Deflate,
                )
                // https://github.com/Majored/rs-async-zip/issues/150
                .unix_permissions(0o644),
                bytes,
            )
            .await?;
        Ok(())
    }

    async fn write_file(
        &mut self,
        directory: &Path,
        filename: &str,
        file: &Path,
    ) -> Result<(), Error> {
        let reader = tokio::io::BufReader::new(fs_err::tokio::File::open(file).await?);
        let mut hasher = Sha256::new();
        let mut reader = InspectReader::new(reader, |bytes| hasher.update(bytes));

        let path = format!("{}/{}", directory.portable_display(), filename);
        let mut stream_writer = self
            .writer
            .write_entry_stream(
                ZipEntryBuilder::new(
                    ZipString::from(path.clone()),
                    // TODO(konsti): Editables use stored.
                    Compression::Deflate,
                )
                // https://github.com/Majored/rs-async-zip/issues/150
                .unix_permissions(0o644),
            )
            .await?
            .compat_write();
        let size = tokio::io::copy(&mut reader, &mut stream_writer)
            .await
            .map_err(|err| Error::ZipCopy {
                src: file.user_display().to_string(),
                zip: self.zip_path.clone(),
                err,
            })?;

        self.record.push(RecordEntry {
            path,
            hash: format!("{:x}", hasher.finalize()),
            size,
        });
        Ok(())
    }

    async fn close(self) -> Result<(), Error> {
        self.writer.close().await?;
        Ok(())
    }
}

struct AsyncFsWriter {
    /// The virtualenv or metadata directory that add file paths are relative to.
    root: PathBuf,
    /// The entries in the `RECORD` file.
    record: Vec<RecordEntry>,
}

/// File system writer.
impl AsyncDirectoryWrite for AsyncFsWriter {
    async fn write_bytes(
        &mut self,
        directory: &Path,
        filename: &str,
        bytes: &[u8],
    ) -> Result<(), Error> {
        fs_err::tokio::create_dir_all(self.root.join(directory)).await?;

        let reader = tokio::io::BufReader::new(bytes);
        let mut hasher = Sha256::new();
        let mut reader = InspectReader::new(reader, |bytes| hasher.update(bytes));

        let dst = self.root.join(directory).join(filename);
        let mut writer = tokio::io::BufWriter::new(fs_err::tokio::File::create(&dst).await?);
        let size = tokio::io::copy(&mut reader, &mut writer)
            .await
            .map_err(|err| Error::IoWrite { dst, err })?;

        self.record.push(RecordEntry {
            path: directory.join(filename).portable_display().to_string(),
            hash: format!("{:x}", hasher.finalize()),
            size,
        });

        Ok(())
    }

    async fn write_file(
        &mut self,
        directory: &Path,
        filename: &str,
        file: &Path,
    ) -> Result<(), Error> {
        let reader = tokio::io::BufReader::new(fs_err::tokio::File::open(file).await?);
        let mut hasher = Sha256::new();
        let mut reader = InspectReader::new(reader, |bytes| hasher.update(bytes));

        let dst = self.root.join(directory).join(filename);
        let mut writer = tokio::io::BufWriter::new(fs_err::tokio::File::create(&dst).await?);
        let size = tokio::io::copy(&mut reader, &mut writer)
            .await
            .map_err(|err| Error::IoCopy {
                src: file.to_path_buf(),
                dst,
                err,
            })?;

        self.record.push(RecordEntry {
            path: directory.join(filename).portable_display().to_string(),
            hash: format!("{:x}", hasher.finalize()),
            size,
        });

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
    size: u64,
}

struct HashedReader {
    size: usize,
    hasher: Sha256,
}

impl HashedReader {
    fn new() -> Self {
        Self {
            size: 0,
            hasher: Sha256::new(),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        self.size += bytes.len();
    }
}

/// Build a wheel from the source tree and place it in the output directory.
pub async fn build(source_tree: &Path, wheel_dir: &Path) -> Result<WheelFilename, Error> {
    let contents = fs_err::tokio::read_to_string(source_tree.join("pyproject.toml")).await?;
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

    // TODO(konsti): async-zip doesn't like a buffered writer
    let zip_path = wheel_dir.join(filename.to_string());
    let wheel_file = fs_err::tokio::File::create(&zip_path).await?;
    let mut wheel_writer = AsyncZipWriter {
        writer: ZipFileWriter::with_tokio(wheel_file),
        zip_path: zip_path.clone(),
        record: vec![],
    };
    write_dist_info(&mut wheel_writer, &pyproject_toml, source_tree).await?;

    for entry in WalkDir::new(source_tree) {
        let entry = entry.map_err(|err| Error::WalkDir {
            root: source_tree.to_path_buf(),
            err,
        })?;

        entry.path();
    }

    wheel_writer.close().await?;
    Ok(filename)
}

/// Write the dist-info directory to the output directory without building the wheel.
pub async fn metadata(source_tree: &Path, metadata_directory: &Path) -> Result<String, Error> {
    let contents = fs_err::tokio::read_to_string(source_tree.join("pyproject.toml")).await?;
    let pyproject_toml = PyProjectToml::parse(&contents)?;
    pyproject_toml.check_build_system();

    let mut wheel_writer = AsyncFsWriter {
        root: metadata_directory.to_path_buf(),
        record: vec![],
    };
    write_dist_info(&mut wheel_writer, &pyproject_toml, source_tree).await?;
    wheel_writer.close().await?;

    Ok(format!(
        "{}-{}.dist-info",
        pyproject_toml.name().as_dist_info_name(),
        pyproject_toml.version()
    ))
}

/// Add `METADATA` and `entry_points.txt` to the dist-info directory.
async fn write_dist_info(
    writer: &mut impl AsyncDirectoryWrite,
    pyproject_toml: &PyProjectToml,
    root: &Path,
) -> Result<(), Error> {
    let dist_info_dir = PathBuf::from(format!(
        "{}-{}.dist-info",
        pyproject_toml.name().as_dist_info_name(),
        pyproject_toml.version()
    ));

    let metadata = pyproject_toml
        .to_metadata(root)
        .await?
        .core_metadata_format();
    writer
        .write_bytes(&dist_info_dir, "METADATA", metadata.as_bytes())
        .await?;

    let entrypoint = pyproject_toml.to_entry_points()?;
    writer
        .write_bytes(&dist_info_dir, "entry_points.txt", entrypoint.as_bytes())
        .await?;

    Ok(())
}

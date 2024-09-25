mod metadata;
mod pep639_glob;

use crate::metadata::{PyProjectToml, ValidationError};
use crate::pep639_glob::Pep639GlobError;
use async_zip::base::write::ZipFileWriter;
use async_zip::error::ZipError;
use async_zip::{Compression, ZipEntryBuilder, ZipString};
use glob::{GlobError, PatternError};
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uv_distribution_filename::WheelFilename;

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
    #[error("Failed to write wheel zip archive")]
    Zip(#[from] ZipError),
}

/// Allow dispatching between writing to a directory, writing to zip and writing to a `.tar.gz`.
trait AsyncDirectoryWrite: Sized {
    async fn write_bytes(
        &mut self,
        directory: &Path,
        filename: &str,
        bytes: &[u8],
    ) -> Result<(), Error>;

    #[allow(clippy::unused_async)] // https://github.com/rust-lang/rust-clippy/issues/11660
    async fn close(self) -> Result<(), Error> {
        Ok(())
    }
}

/// Zip archive (wheel) writer.
struct AsyncZipWriter(ZipFileWriter<tokio_util::compat::Compat<fs_err::tokio::File>>);

impl AsyncDirectoryWrite for AsyncZipWriter {
    async fn write_bytes(
        &mut self,
        directory: &Path,
        filename: &str,
        bytes: &[u8],
    ) -> Result<(), Error> {
        self.0
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

    async fn close(self) -> Result<(), Error> {
        self.0.close().await?;
        Ok(())
    }
}

struct AsyncFsWriter {
    root: PathBuf,
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
        fs_err::tokio::write(self.root.join(directory).join(filename), bytes).await?;
        Ok(())
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
    let wheel_file = fs_err::tokio::File::create(wheel_dir.join(filename.to_string())).await?;
    let mut wheel_writer = AsyncZipWriter(ZipFileWriter::with_tokio(wheel_file));
    write_dist_info(&mut wheel_writer, &pyproject_toml, source_tree).await?;
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

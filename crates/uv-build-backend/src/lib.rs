mod metadata;
mod pep639_glob;

use crate::metadata::{PyProjectToml, ValidationError};
use crate::pep639_glob::Pep639GlobError;
use glob::{GlobError, PatternError};
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uv_distribution_filename::WheelFilename;
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
    #[error("Failed to write wheel zip archive")]
    Zip(#[from] zip::result::ZipError),
}

/// Allow dispatching between writing to a directory, writing to zip and writing to a `.tar.gz`.
///
/// All paths are string types instead of path types since wheel are portable between platforms.
trait DirectoryWriter: Sized {
    /// Add a file with the given content.
    fn write_bytes(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error>;

    /// Create a directory.
    fn write_directory(&mut self, directory: &str) -> Result<(), Error>;

    #[allow(clippy::unused_async)] // https://github.com/rust-lang/rust-clippy/issues/11660
    fn close(self) -> Result<(), Error> {
        Ok(())
    }
}

/// Zip archive (wheel) writer.
struct ZipDirectoryWriter {
    writer: ZipWriter<fs_err::File>,
    compression: CompressionMethod,
}

impl ZipDirectoryWriter {
    /// A wheel writer with deflate compression.
    fn new_wheel(file: fs_err::File) -> Self {
        Self {
            writer: ZipWriter::new(file),
            compression: CompressionMethod::Deflated,
        }
    }

    /// A wheel writer with no (stored) compression.
    ///
    /// Since editables are temporary, we save time be skipping compression and decompression.
    #[expect(dead_code)]
    fn new_editable(file: fs_err::File) -> Self {
        Self {
            writer: ZipWriter::new(file),
            compression: CompressionMethod::Stored,
        }
    }
}

impl DirectoryWriter for ZipDirectoryWriter {
    fn write_bytes(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error> {
        let options = zip::write::FileOptions::default().compression_method(self.compression);
        self.writer.start_file(path, options)?;
        self.writer.write_all(bytes)?;
        Ok(())
    }

    fn write_directory(&mut self, directory: &str) -> Result<(), Error> {
        let options = zip::write::FileOptions::default().compression_method(self.compression);
        Ok(self.writer.add_directory(directory, options)?)
    }

    fn close(mut self) -> Result<(), Error> {
        self.writer.finish()?;
        Ok(())
    }
}

struct AsyncFsWriter {
    root: PathBuf,
}

/// File system writer.
impl DirectoryWriter for AsyncFsWriter {
    fn write_bytes(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error> {
        Ok(fs_err::write(path, bytes)?)
    }

    fn write_directory(&mut self, directory: &str) -> Result<(), Error> {
        Ok(fs_err::create_dir(self.root.join(directory))?)
    }
}

/// Build a wheel from the source tree and place it in the output directory.
pub fn build(source_tree: &Path, wheel_dir: &Path) -> Result<WheelFilename, Error> {
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

    let mut wheel_writer =
        ZipDirectoryWriter::new_wheel(fs_err::File::create(wheel_dir.join(filename.to_string()))?);
    write_dist_info(&mut wheel_writer, &pyproject_toml, source_tree)?;
    wheel_writer.close()?;
    Ok(filename)
}

/// Write the dist-info directory to the output directory without building the wheel.
pub fn metadata(source_tree: &Path, metadata_directory: &Path) -> Result<String, Error> {
    let contents = fs_err::read_to_string(source_tree.join("pyproject.toml"))?;
    let pyproject_toml = PyProjectToml::parse(&contents)?;
    pyproject_toml.check_build_system();

    let mut wheel_writer = AsyncFsWriter {
        root: metadata_directory.to_path_buf(),
    };
    write_dist_info(&mut wheel_writer, &pyproject_toml, source_tree)?;
    wheel_writer.close()?;

    Ok(format!(
        "{}-{}.dist-info",
        pyproject_toml.name().as_dist_info_name(),
        pyproject_toml.version()
    ))
}

/// Add `METADATA` and `entry_points.txt` to the dist-info directory.
fn write_dist_info(
    writer: &mut impl DirectoryWriter,
    pyproject_toml: &PyProjectToml,
    root: &Path,
) -> Result<(), Error> {
    let dist_info_dir = format!(
        "{}-{}.dist-info",
        pyproject_toml.name().as_dist_info_name(),
        pyproject_toml.version()
    );

    writer.write_directory(&dist_info_dir)?;

    let metadata = pyproject_toml.to_metadata(root)?.core_metadata_format();
    writer.write_bytes(&format!("{dist_info_dir}/METADATA"), metadata.as_bytes())?;

    let entrypoint = pyproject_toml.to_entry_points()?;
    writer.write_bytes(
        &format!("{dist_info_dir}/entry_points.txt"),
        entrypoint.as_bytes(),
    )?;

    Ok(())
}

//! Helpers for constructing test archives.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use flate2::write::GzEncoder;
use futures::executor::block_on;
use futures::io::AllowStdIo;
use indoc::{formatdoc, indoc};
use tar_codec::{ArchiveBuilder as _, EntryMetadata, TarEncoder};
use tokio_util::compat::FuturesAsyncWriteCompatExt;

use uv_fs::PythonExt;
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::packse::generate_wheel;

/// Write the given files to a gzip-compressed tar archive.
pub fn write_tar_gz(writer: impl Write, entries: &[(&str, impl AsRef<[u8]>)]) -> Result<()> {
    let mut encoder = GzEncoder::new(writer, flate2::Compression::default());
    let mut tar = TarEncoder::new(AllowStdIo::new(&mut encoder).compat_write()).builder();

    for (path, contents) in entries {
        block_on(tar.add_file(path, contents.as_ref(), EntryMetadata::default()))?;
    }

    block_on(tar.finish())?;
    encoder.finish()?;
    Ok(())
}

/// Create a source archive with a backend that provides metadata and builds wheels.
///
/// Reading the metadata requires running the backend.
/// If `marker_path` is set, importing the backend creates that file.
/// The path is stored in the archive, so changing it changes the archive's hash.
pub fn generate_source_archive(
    name: &PackageName,
    version: &Version,
    subdirectory: &str,
    marker_path: Option<&Path>,
) -> Result<Vec<u8>> {
    let (filename, wheel) = generate_wheel(
        name,
        version,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[],
    );
    let name = name.as_dist_info_name();
    let pyproject = indoc! {r#"
        [build-system]
        requires = []
        build-backend = "backend"
        backend-path = ["."]
    "#};
    let marker = if let Some(marker_path) = marker_path {
        format!("Path({}).touch()", marker_path.escape_for_python())
    } else {
        String::new()
    };
    let backend = formatdoc! {r#"
        import shutil
        from pathlib import Path
        from zipfile import ZipFile

        {marker}

        WHEEL = Path(__file__).with_name("{filename}")
        DIST_INFO = "{name}-{version}.dist-info"

        def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
            shutil.copyfile(WHEEL, Path(wheel_directory) / WHEEL.name)
            return WHEEL.name

        def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
            dist_info = Path(metadata_directory) / DIST_INFO
            dist_info.mkdir()
            with ZipFile(WHEEL) as wheel:
                (dist_info / "METADATA").write_bytes(wheel.read(f"{{DIST_INFO}}/METADATA"))
            return dist_info.name
    "#};
    let mut prefix = format!("{name}-{version}/");
    if !subdirectory.is_empty() {
        prefix.push_str(subdirectory.trim_end_matches('/'));
        prefix.push('/');
    }
    let mut archive = Vec::new();
    write_tar_gz(
        &mut archive,
        &[
            (&format!("{prefix}pyproject.toml"), pyproject.as_bytes()),
            (&format!("{prefix}backend.py"), backend.as_bytes()),
            (&format!("{prefix}{filename}"), wheel.as_slice()),
        ],
    )?;
    Ok(archive)
}

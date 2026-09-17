//! Helpers for constructing test archives.

use std::io::Write;

use anyhow::Result;
use flate2::write::GzEncoder;
use futures::executor::block_on;
use futures::io::AllowStdIo;
use tar_codec::{ArchiveBuilder as _, EntryMetadata, TarEncoder};
use tokio_util::compat::FuturesAsyncWriteCompatExt;

/// Write the given files to a gzip-compressed tar archive.
pub fn write_tar_gz(writer: impl Write, entries: &[(&str, impl AsRef<[u8]>)]) -> Result<()> {
    write_tar_gz_inner(
        writer,
        entries
            .iter()
            .map(|(path, contents)| (*path, contents.as_ref(), false)),
    )
}

fn write_tar_gz_inner<'a>(
    writer: impl Write,
    entries: impl IntoIterator<Item = (&'a str, &'a [u8], bool)>,
) -> Result<()> {
    let mut encoder = GzEncoder::new(writer, flate2::Compression::default());
    let mut tar = TarEncoder::new(AllowStdIo::new(&mut encoder).compat_write()).builder();

    for (path, contents, executable) in entries {
        block_on(tar.add_file(
            path,
            contents,
            EntryMetadata::default().executable(executable),
        ))?;
    }

    block_on(tar.finish())?;
    encoder.finish()?;
    Ok(())
}

/// Write the given files to a gzip-compressed tar archive, preserving executable intent.
pub fn write_tar_gz_with_executables(
    writer: impl Write,
    entries: &[(&str, &[u8], bool)],
) -> Result<()> {
    write_tar_gz_inner(writer, entries.iter().copied())
}

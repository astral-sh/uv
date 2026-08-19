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
    let mut encoder = GzEncoder::new(writer, flate2::Compression::default());
    let mut tar = TarEncoder::new(AllowStdIo::new(&mut encoder).compat_write()).builder();

    for (path, contents) in entries {
        block_on(tar.add_file(path, contents.as_ref(), EntryMetadata::default()))?;
    }

    block_on(tar.finish())?;
    encoder.finish()?;
    Ok(())
}

use std::path::Path;

use async_compression::tokio::bufread::{GzipEncoder, ZstdEncoder};
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::io::AllowStdIo;
use tar_codec::{ArchiveBuilder as _, EntryMetadata, TarEncoder};
use tokio::io::AsyncReadExt;
use tokio_util::compat::FuturesAsyncWriteCompatExt;

use uv_distribution_filename::{LegacySourceDistExtension, SourceDistExtension};
use uv_extract::dirhash::UnhashedFile;
use uv_preview::PreviewFeature;

const FILES: &[(&str, &[u8])] = &[
    ("package/module.py", b"print('hello')\n"),
    ("package/data.bin", b"\x00\x01\x02\xff"),
];

async fn archives() -> anyhow::Result<Vec<(SourceDistExtension, Vec<u8>)>> {
    let mut tar_bytes = Vec::new();
    {
        let mut tar = TarEncoder::new(AllowStdIo::new(&mut tar_bytes).compat_write()).builder();
        for (path, contents) in FILES {
            tar.add_file(path, *contents, EntryMetadata::default())
                .await?;
        }
        tar.finish().await?;
    }

    let mut gzip_bytes = Vec::new();
    GzipEncoder::new(tar_bytes.as_slice())
        .read_to_end(&mut gzip_bytes)
        .await?;
    let mut zstd_bytes = Vec::new();
    ZstdEncoder::new(tar_bytes.as_slice())
        .read_to_end(&mut zstd_bytes)
        .await?;

    let mut zip = ZipFileWriter::new(Vec::new());
    for (path, contents) in FILES {
        zip.write_entry_whole(
            ZipEntryBuilder::new((*path).into(), Compression::Stored),
            contents,
        )
        .await?;
    }

    Ok(vec![
        (
            SourceDistExtension::Legacy(LegacySourceDistExtension::Tar),
            tar_bytes,
        ),
        (SourceDistExtension::TarGz, gzip_bytes.clone()),
        (
            SourceDistExtension::Legacy(LegacySourceDistExtension::Tgz),
            gzip_bytes,
        ),
        (
            SourceDistExtension::Legacy(LegacySourceDistExtension::TarZst),
            zstd_bytes,
        ),
        (
            SourceDistExtension::Legacy(LegacySourceDistExtension::Zip),
            zip.close().await?,
        ),
    ])
}

fn check_files(target: &Path, files: &[UnhashedFile]) -> anyhow::Result<()> {
    assert_eq!(files.len(), FILES.len());
    for (path, contents) in FILES {
        let file = files
            .iter()
            .find(|file| file.path() == Path::new(path))
            .expect("Extracted files should include every archive member");
        assert_eq!(file.size(), contents.len() as u64);
        assert_eq!(fs_err::read(target.join(path))?, *contents);
    }
    Ok(())
}

#[test]
fn archive_formats_with_memory_and_file_readers() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let archives = runtime.block_on(archives())?;

    for features in [&[][..], &[PreviewFeature::TarCodec][..]] {
        let _features = uv_preview::test::with_features(features);
        for (extension, bytes) in &archives {
            let target = tempfile::tempdir()?;
            let mut reader = bytes.as_slice();
            let (target, files) =
                runtime.block_on(uv_extract::stream::archive(&mut reader, *extension, target))?;
            check_files(target.path(), &files)?;

            let archive_file = tempfile::NamedTempFile::new()?;
            fs_err::write(archive_file.path(), bytes)?;
            let mut reader = runtime.block_on(fs_err::tokio::File::open(archive_file.path()))?;
            let target = tempfile::tempdir()?;
            let (target, files) =
                runtime.block_on(uv_extract::stream::archive(&mut reader, *extension, target))?;
            check_files(target.path(), &files)?;
        }
    }
    Ok(())
}

use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use async_compression::tokio::bufread::{GzipDecoder, ZstdDecoder};
use async_compression::tokio::write::GzipEncoder;
use async_zip::base::read::seek::ZipFileReader;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use base64::{Engine, prelude::BASE64_URL_SAFE_NO_PAD};
use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use sha2::{Digest, Sha256};
use tokio::io::{
    AsyncReadExt as TokioAsyncReadExt, AsyncWriteExt as TokioAsyncWriteExt, BufReader,
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use uv_distribution_filename::{
    LegacySourceDistExtension, SourceDistExtension, SourceDistFilename, WheelFilename,
};
use uv_install_wheel::{RecordEntry, read_record};
use uv_metadata::find_archive_dist_info;
use uv_pypi_types::ResolutionMetadata;

// Match uv_build's whole-entry and streaming paths when rewriting its wheels.
const WHOLE_FILE_ZIP_ENTRY_LIMIT: u64 = 16 * 1024 * 1024;
const ZIP_STREAM_BUFFER_SIZE: usize = 128 * 1024;

pub(super) async fn sdist_metadata(
    path: &Path,
    filename: &SourceDistFilename,
) -> Result<ResolutionMetadata> {
    let input = fs_err::tokio::File::open(path).await?;
    let contents = match filename.extension {
        SourceDistExtension::TarGz
        | SourceDistExtension::Legacy(LegacySourceDistExtension::Tgz) => {
            sdist_tar_contents(GzipDecoder::new(BufReader::new(input)), filename).await?
        }
        SourceDistExtension::Legacy(LegacySourceDistExtension::Tar) => {
            sdist_tar_contents(BufReader::new(input), filename).await?
        }
        SourceDistExtension::Legacy(LegacySourceDistExtension::TarZst) => {
            sdist_tar_contents(ZstdDecoder::new(BufReader::new(input)), filename).await?
        }
        SourceDistExtension::Legacy(LegacySourceDistExtension::Zip) => {
            sdist_zip_contents(input, filename).await?
        }
        SourceDistExtension::Legacy(
            LegacySourceDistExtension::TarBz2
            | LegacySourceDistExtension::TarLz
            | LegacySourceDistExtension::TarLzma
            | LegacySourceDistExtension::TarXz
            | LegacySourceDistExtension::Tbz
            | LegacySourceDistExtension::Tlz
            | LegacySourceDistExtension::Txz,
        ) => bail!("Unsupported source distribution compression"),
    };
    contents
        .metadata
        .context("Source distribution has no PKG-INFO file")
}

#[derive(Default)]
struct SdistContents {
    metadata: Option<ResolutionMetadata>,
    lock_seen: bool,
}

impl SdistContents {
    fn lock(&mut self, regular: bool) -> Result<()> {
        if self.lock_seen || !regular {
            bail!("Source distribution has an invalid or duplicate pylock.toml file");
        }
        self.lock_seen = true;
        Ok(())
    }
}

#[derive(PartialEq, Eq)]
enum SdistFile {
    Lock,
    Metadata,
    Other,
}

/// Locate files at the source distribution root and verify the root name.
fn sdist_file(path: &Path, filename: &SourceDistFilename) -> Result<SdistFile> {
    let mut components = path.components();
    let Some(Component::Normal(root)) = components.next() else {
        bail!("Source distribution contains an invalid path");
    };
    let Some(root) = root.to_str() else {
        bail!("Source distribution root is not UTF-8");
    };
    let root = SourceDistFilename::parsed_normalized_filename(&format!("{root}.tar.gz"))?;
    if root.name != filename.name || root.version != filename.version {
        bail!("Source distribution root does not match its filename");
    }
    Ok(match (components.next(), components.next()) {
        (Some(Component::Normal(name)), None) if name == "pylock.toml" => SdistFile::Lock,
        (Some(Component::Normal(name)), None) if name == "PKG-INFO" => SdistFile::Metadata,
        _ => SdistFile::Other,
    })
}

async fn sdist_tar_contents<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    filename: &SourceDistFilename,
) -> Result<SdistContents> {
    let mut archive = tokio_tar::Archive::new(reader);
    let mut entries = archive.entries()?;
    let mut result = SdistContents::default();
    while let Some(entry) = entries.next().await {
        let mut entry = entry?;
        match sdist_file(&entry.path()?, filename)? {
            SdistFile::Lock => result.lock(entry.header().entry_type().is_file())?,
            SdistFile::Metadata => {
                if result.metadata.is_some() || !entry.header().entry_type().is_file() {
                    bail!("Source distribution has an invalid or duplicate PKG-INFO file");
                }
                let mut contents = Vec::new();
                TokioAsyncReadExt::read_to_end(
                    &mut TokioAsyncReadExt::take(&mut entry, WHOLE_FILE_ZIP_ENTRY_LIMIT + 1),
                    &mut contents,
                )
                .await?;
                if contents.len() as u64 > WHOLE_FILE_ZIP_ENTRY_LIMIT {
                    bail!("Source distribution PKG-INFO is too large");
                }
                result.metadata = Some(ResolutionMetadata::parse_pkg_info(&contents)?);
            }
            SdistFile::Other => {}
        }
    }
    Ok(result)
}

async fn sdist_zip_contents(
    input: fs_err::tokio::File,
    filename: &SourceDistFilename,
) -> Result<SdistContents> {
    let mut archive = ZipFileReader::new(BufReader::new(input).compat()).await?;
    let mut result = SdistContents::default();
    for index in 0..archive.file().entries().len() {
        let entry = &archive.file().entries()[index];
        let file = sdist_file(Path::new(entry.filename().as_str()?), filename)?;
        let regular = !entry.dir()?
            && entry.unix_permissions().is_none_or(|mode| {
                let file_type = mode & 0o170_000;
                file_type == 0 || file_type == 0o100_000
            });
        match file {
            SdistFile::Lock => result.lock(regular)?,
            SdistFile::Metadata => {
                if result.metadata.is_some() || !regular {
                    bail!("Source distribution has an invalid or duplicate PKG-INFO file");
                }
                let mut entry = archive.reader_with_entry(index).await?;
                let mut contents = Vec::new();
                AsyncReadExt::read_to_end(
                    &mut AsyncReadExt::take(&mut entry, WHOLE_FILE_ZIP_ENTRY_LIMIT + 1),
                    &mut contents,
                )
                .await?;
                if contents.len() as u64 > WHOLE_FILE_ZIP_ENTRY_LIMIT {
                    bail!("Source distribution PKG-INFO is too large");
                }
                if entry.compute_hash() != entry.entry().crc32() {
                    bail!("Source distribution PKG-INFO has an invalid checksum");
                }
                result.metadata = Some(ResolutionMetadata::parse_pkg_info(&contents)?);
            }
            SdistFile::Other => {}
        }
    }
    Ok(result)
}

pub(super) async fn wheel_metadata(
    path: &Path,
    filename: &WheelFilename,
) -> Result<ResolutionMetadata> {
    let input = fs_err::tokio::File::open(path).await?;
    let mut archive = ZipFileReader::new(BufReader::new(input).compat()).await?;
    let (index, _) = find_archive_dist_info(
        filename,
        archive
            .file()
            .entries()
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| Some((index, entry.filename().as_str().ok()?))),
    )?;
    let mut contents = Vec::new();
    archive
        .reader_with_entry(index)
        .await?
        .read_to_end_checked(&mut contents)
        .await?;
    Ok(ResolutionMetadata::parse_metadata(&contents)?)
}

/// Add the source lock before using the distribution to build a wheel.
pub(super) async fn add_to_sdist(
    path: &Path,
    filename: &SourceDistFilename,
    lock: &[u8],
) -> Result<()> {
    let input = fs_err::tokio::File::open(path).await?;
    let mut archive = tokio_tar::Archive::new(GzipDecoder::new(BufReader::new(input)));
    let temporary = uv_fs::tempfile_in(
        path.parent()
            .context("Source distribution has no parent directory")?,
    )
    .context("Failed to create a temporary source distribution")?;
    let mut entries = archive.entries()?;
    let mut top_level: Option<PathBuf> = None;
    let mut ranges = Vec::new();
    let mut position = 0;
    while let Some(entry) = entries.next().await {
        let mut entry = entry?;
        if entry.header().entry_type().is_gnu_sparse() {
            bail!("Cannot add a lock to a source distribution with sparse entries");
        }
        let global = entry.header().entry_type().is_pax_global_extensions();
        if let Some(extensions) = entry.pax_extensions().await? {
            for extension in extensions {
                let extension = extension?;
                let key = extension.key_bytes();
                if key.starts_with(b"GNU.sparse.")
                    || (global && [b"path".as_slice(), b"linkpath", b"size"].contains(&key))
                {
                    bail!("Cannot add a lock to a source distribution with these PAX extensions");
                }
            }
        }
        let end = entry
            .raw_file_position()
            .checked_add(
                entry
                    .effective_size()
                    .checked_add(511)
                    .context("Archive entry is too large")?
                    & !511,
            )
            .context("Archive entry is too large")?;
        if end < position {
            bail!("Source distribution contains overlapping entries");
        }
        if global {
            ranges.push((position, end));
            position = end;
            continue;
        }
        let path = entry.path()?.into_owned();
        let mut components = path.components();
        let Some(Component::Normal(root)) = components.next() else {
            bail!(
                "Source distribution contains an invalid path: {}",
                path.display()
            );
        };
        if components.any(|component| !matches!(component, Component::Normal(_))) {
            bail!(
                "Source distribution contains an invalid path: {}",
                path.display()
            );
        }
        if let Some(top_level) = &top_level {
            if top_level != Path::new(root) {
                bail!("Source distribution contains multiple top-level directories");
            }
        } else {
            let root_name = root
                .to_str()
                .context("Source distribution root is not UTF-8")?;
            let root_filename = format!("{root_name}.tar.gz");
            let root_filename = SourceDistFilename::parsed_normalized_filename(&root_filename)?;
            if root_filename.name != filename.name || root_filename.version != filename.version {
                bail!("Source distribution root does not match its filename");
            }
            top_level = Some(PathBuf::from(root));
        }
        if path != Path::new(root).join("pylock.toml") {
            ranges.push((position, end));
        }
        position = end;
    }
    let lock_path = top_level
        .context("Source distribution is empty")?
        .join("pylock.toml");
    drop(entries);
    drop(archive);
    let input = fs_err::tokio::File::open(path).await?;
    let mut input = GzipDecoder::new(BufReader::new(input));
    let output = fs_err::tokio::OpenOptions::new()
        .write(true)
        .open(temporary.as_ref())
        .await?;
    let mut output = GzipEncoder::new(output);
    let mut position = 0;
    for (start, end) in ranges {
        let skipped = tokio::io::copy(
            &mut (&mut input).take(start - position),
            &mut tokio::io::sink(),
        )
        .await?;
        if skipped != start - position {
            bail!("Source distribution ended before its last entry");
        }
        let copied = tokio::io::copy(&mut (&mut input).take(end - start), &mut output).await?;
        if copied != end - start {
            bail!("Source distribution ended before its last entry");
        }
        position = end;
    }
    let mut builder = tokio_tar::Builder::new(output);
    let mut header = tokio_tar::Header::new_gnu();
    // Python's tarfile can mistake an old-style regular entry for a directory.
    // https://github.com/python/cpython/issues/141707
    header.set_entry_type(tokio_tar::EntryType::Regular);
    header.set_size(lock.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_cksum();
    builder.append_data(&mut header, &lock_path, lock).await?;
    let mut output = builder.into_inner().await?;
    output.shutdown().await?;
    drop(output);
    drop(input);
    uv_fs::persist_with_retry(temporary, path)
        .await
        .context("Failed to replace the source distribution")?;
    Ok(())
}

/// Add a lock to the wheel and update its RECORD.
pub(super) async fn add_to_wheel(path: &Path, filename: &WheelFilename, lock: &[u8]) -> Result<()> {
    let input = fs_err::tokio::File::open(path).await?;
    let mut archive = ZipFileReader::new(BufReader::new(input).compat()).await?;
    let names = archive
        .file()
        .entries()
        .iter()
        .map(|entry| entry.filename().as_str().map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    let (_, stem) = find_archive_dist_info(
        filename,
        names
            .iter()
            .enumerate()
            .map(|(index, name)| (index, name.as_str())),
    )?;
    let dist_info = format!("{stem}.dist-info");
    let lock_path = format!("{dist_info}/pylock.toml");
    let record_path = format!("{dist_info}/RECORD");
    let temporary = uv_fs::tempfile_in(path.parent().context("Wheel has no parent directory")?)
        .context("Failed to create a temporary wheel")?;
    let output = fs_err::tokio::OpenOptions::new()
        .write(true)
        .open(temporary.as_ref())
        .await?;
    let mut writer = ZipFileWriter::new(output.compat_write());
    let mut record = None;
    for (index, name) in names.iter().enumerate() {
        if name == &format!("{dist_info}/RECORD.jws") || name == &format!("{dist_info}/RECORD.p7s")
        {
            bail!("Cannot add a lock to a signed wheel");
        }
        if name == &lock_path {
            continue;
        }
        let mut input = archive.reader_with_entry(index).await?;
        let metadata = ZipEntryBuilder::from(input.entry().clone()).extra_fields(Vec::new());
        if name == &record_path {
            if record.is_some() {
                bail!("Wheel contains multiple RECORD files");
            }
            let mut contents = Vec::new();
            input.read_to_end(&mut contents).await?;
            if input.compute_hash() != input.entry().crc32() {
                bail!("Wheel entry has an invalid CRC: `{name}`");
            }
            record = Some((metadata, read_record(contents.as_slice())?));
        } else if input.entry().uncompressed_size() <= WHOLE_FILE_ZIP_ENTRY_LIMIT {
            let mut contents = Vec::new();
            (&mut input)
                .take(WHOLE_FILE_ZIP_ENTRY_LIMIT + 1)
                .read_to_end(&mut contents)
                .await?;
            if contents.len() as u64 > WHOLE_FILE_ZIP_ENTRY_LIMIT {
                bail!("Wheel entry exceeds its declared size");
            }
            if input.compute_hash() != input.entry().crc32() {
                bail!("Wheel entry has an invalid CRC: `{name}`");
            }
            writer.write_entry_whole(metadata, &contents).await?;
        } else {
            let mut output = writer.write_entry_seekable(metadata).await?;
            copy_zip_entry(&mut input, &mut output).await?;
            if input.compute_hash() != input.entry().crc32() {
                bail!("Wheel entry has an invalid CRC: `{name}`");
            }
            output.close().await?;
        }
    }
    let (metadata, mut entries) = record.context("Wheel has no RECORD file")?;
    entries.retain(|entry| entry.path != lock_path);
    let position = entries
        .iter()
        .position(|entry| entry.path == record_path)
        .unwrap_or(entries.len());
    entries.insert(
        position,
        RecordEntry {
            path: lock_path.clone(),
            hash: Some(format!(
                "sha256={}",
                BASE64_URL_SAFE_NO_PAD.encode(Sha256::digest(lock))
            )),
            size: Some(lock.len() as u64),
        },
    );
    let mut record = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(Vec::new());
    for entry in entries {
        record.serialize(entry)?;
    }
    let record = record.into_inner()?;
    let entry =
        ZipEntryBuilder::new(lock_path.into(), Compression::Deflate).unix_permissions(0o100_644);
    writer.write_entry_whole(entry, lock).await?;
    writer.write_entry_whole(metadata, &record).await?;
    let mut output = writer.close().await?.into_inner();
    output.shutdown().await?;
    drop(output);
    drop(archive);
    uv_fs::persist_with_retry(temporary, path)
        .await
        .context("Failed to replace the wheel")?;
    Ok(())
}

async fn copy_zip_entry(
    input: &mut (impl futures::AsyncRead + Unpin),
    output: &mut (impl futures::AsyncWrite + Unpin),
) -> Result<()> {
    let mut buffer = vec![0; ZIP_STREAM_BUFFER_SIZE];
    loop {
        let mut filled = 0;
        while filled < buffer.len() {
            let read = input.read(&mut buffer[filled..]).await?;
            if read == 0 {
                break;
            }
            filled += read;
        }
        if filled == 0 {
            return Ok(());
        }
        output.write_all(&buffer[..filled]).await?;
    }
}

use std::path::{Component, Path};

use anyhow::{Context, Result, bail};
use async_compression::tokio::bufread::{GzipDecoder, ZstdDecoder};
use async_zip::base::read::seek::ZipFileReader;
use futures::{AsyncReadExt, StreamExt};
use tokio::io::{AsyncReadExt as TokioAsyncReadExt, BufReader};
use tokio_util::compat::TokioAsyncReadCompatExt;
use uv_distribution_filename::{
    LegacySourceDistExtension, SourceDistExtension, SourceDistFilename, WheelFilename,
};
use uv_metadata::find_archive_dist_info;
use uv_pypi_types::ResolutionMetadata;

const WHOLE_FILE_ZIP_ENTRY_LIMIT: u64 = 16 * 1024 * 1024;

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

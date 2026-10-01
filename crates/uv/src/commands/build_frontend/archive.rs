use std::path::{Component, Path};

use anyhow::{Context, Result, bail};
use async_compression::tokio::bufread::GzipDecoder;
use async_zip::base::read::seek::ZipFileReader;
use futures::StreamExt;
use tokio::io::{AsyncReadExt as TokioAsyncReadExt, BufReader};
use tokio_util::compat::TokioAsyncReadCompatExt;
use uv_distribution_filename::{SourceDistExtension, SourceDistFilename, WheelFilename};
use uv_metadata::find_archive_dist_info;
use uv_pypi_types::ResolutionMetadata;

const WHOLE_FILE_ZIP_ENTRY_LIMIT: u64 = 16 * 1024 * 1024;

pub(super) async fn sdist_metadata(
    path: &Path,
    filename: &SourceDistFilename,
) -> Result<ResolutionMetadata> {
    inspect_sdist(path, filename)
        .await?
        .metadata
        .context("Source distribution has no PKG-INFO file")
}

async fn inspect_sdist(path: &Path, filename: &SourceDistFilename) -> Result<SdistContents> {
    let input = fs_err::tokio::File::open(path).await?;
    match filename.extension {
        SourceDistExtension::TarGz => {
            sdist_tar_contents(GzipDecoder::new(BufReader::new(input)), filename).await
        }
        SourceDistExtension::Legacy(_) => bail!("Unsupported source distribution compression"),
    }
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

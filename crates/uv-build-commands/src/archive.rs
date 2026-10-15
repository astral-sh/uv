use std::path::{Component, Path};

use anyhow::{Context, Result, bail};
use async_compression::tokio::bufread::GzipDecoder;
use futures::StreamExt;
use tokio::io::{AsyncReadExt, BufReader};
use uv_distribution_filename::SourceDistFilename;
use uv_lock::build::ExportedLock;
use uv_pypi_types::ResolutionMetadata;

const MAX_CORE_METADATA_SIZE: u64 = 16 * 1024 * 1024;

pub(super) async fn verify_sdist_metadata(
    path: &Path,
    filename: &SourceDistFilename,
    lock: &ExportedLock,
) -> Result<()> {
    let metadata = sdist_metadata(path, filename).await?;
    if metadata.name != filename.name
        || metadata.version != filename.version
        || !lock.matches_metadata(&metadata)?
    {
        bail!("The source distribution's metadata does not match the project lock");
    }
    Ok(())
}

async fn sdist_metadata(path: &Path, filename: &SourceDistFilename) -> Result<ResolutionMetadata> {
    let input = fs_err::tokio::File::open(path).await?;
    let contents = sdist_tar_contents(GzipDecoder::new(BufReader::new(input)), filename).await?;
    if !contents.pyproject_seen {
        bail!("Source distribution has no pyproject.toml file");
    }
    contents
        .metadata
        .context("Source distribution has no PKG-INFO file")
}

#[derive(Default)]
struct SdistContents {
    metadata: Option<ResolutionMetadata>,
    lock_seen: bool,
    pyproject_seen: bool,
}

fn record_sdist_file(seen: &mut bool, regular: bool, filename: &str) -> Result<()> {
    if *seen || !regular {
        bail!("Source distribution has an invalid or duplicate {filename} file");
    }
    *seen = true;
    Ok(())
}

#[derive(PartialEq, Eq)]
enum SdistFile {
    Lock,
    Metadata,
    Pyproject,
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
        (Some(Component::Normal(name)), None) if name == "pyproject.toml" => SdistFile::Pyproject,
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
        let regular = entry.header().entry_type().is_file();
        match sdist_file(&entry.path()?, filename)? {
            SdistFile::Lock => record_sdist_file(&mut result.lock_seen, regular, "pylock.toml")?,
            SdistFile::Pyproject => {
                record_sdist_file(&mut result.pyproject_seen, regular, "pyproject.toml")?;
            }
            SdistFile::Metadata => {
                if result.metadata.is_some() || !regular {
                    bail!("Source distribution has an invalid or duplicate PKG-INFO file");
                }
                let mut contents = Vec::new();
                AsyncReadExt::read_to_end(
                    &mut AsyncReadExt::take(&mut entry, MAX_CORE_METADATA_SIZE + 1),
                    &mut contents,
                )
                .await?;
                if contents.len() as u64 > MAX_CORE_METADATA_SIZE {
                    bail!("Source distribution PKG-INFO is too large");
                }
                result.metadata = Some(ResolutionMetadata::parse_pkg_info(&contents)?);
            }
            SdistFile::Other => {}
        }
    }
    Ok(result)
}

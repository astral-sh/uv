use std::path::{Path, PathBuf};

use futures::{FutureExt, StreamExt};
use reqwest::Response;
use tracing::{debug, info_span, warn, Instrument};
use url::Url;

use distribution_filename::DistFilename;
use distribution_types::{File, FileLocation, FlatIndexLocation, IndexUrl, UrlString};
use uv_cache::{Cache, CacheBucket};

use crate::cached_client::{CacheControl, CachedClientError};
use crate::html::SimpleHtml;
use crate::{Connectivity, Error, ErrorKind, RegistryClient};

#[derive(Debug, thiserror::Error)]
pub enum FlatIndexError {
    #[error("Expected a file URL, but received: {0}")]
    NonFileUrl(Url),

    #[error("Failed to read `--find-links` directory: {0}")]
    FindLinksDirectory(PathBuf, #[source] FindLinksDirectoryError),

    #[error("Failed to read `--find-links` URL: {0}")]
    FindLinksUrl(Url, #[source] Error),
}

#[derive(Debug, thiserror::Error)]
pub enum FindLinksDirectoryError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    VerbatimUrl(#[from] pep508_rs::VerbatimUrlError),
}

#[derive(Debug, Default, Clone)]
pub struct FlatIndexEntries {
    /// The list of `--find-links` entries.
    pub entries: Vec<(DistFilename, File, IndexUrl)>,
    /// Whether any `--find-links` entries could not be resolved due to a lack of network
    /// connectivity.
    pub offline: bool,
}

impl FlatIndexEntries {
    /// Create a [`FlatIndexEntries`] from a list of `--find-links` entries.
    fn from_entries(entries: Vec<(DistFilename, File, IndexUrl)>) -> Self {
        Self {
            entries,
            offline: false,
        }
    }

    /// Create a [`FlatIndexEntries`] to represent an offline `--find-links` entry.
    fn offline() -> Self {
        Self {
            entries: Vec::new(),
            offline: true,
        }
    }

    /// Extend this list of `--find-links` entries with another list.
    fn extend(&mut self, other: Self) {
        self.entries.extend(other.entries);
        self.offline |= other.offline;
    }

    /// Return the number of `--find-links` entries.
    fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return `true` if there are no `--find-links` entries.
    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A client for reading distributions from `--find-links` entries (either local directories or
/// remote HTML indexes).
#[derive(Debug, Clone)]
pub struct FlatIndexClient<'a> {
    client: &'a RegistryClient,
    cache: &'a Cache,
}

impl<'a> FlatIndexClient<'a> {
    /// Create a new [`FlatIndexClient`].
    pub fn new(client: &'a RegistryClient, cache: &'a Cache) -> Self {
        Self { client, cache }
    }

    /// Read the directories and flat remote indexes from `--find-links`.
    #[allow(clippy::result_large_err)]
    pub async fn fetch(
        &self,
        indexes: impl Iterator<Item = &FlatIndexLocation>,
    ) -> Result<FlatIndexEntries, FlatIndexError> {
        let mut fetches = futures::stream::iter(indexes)
            .map(|index| async move {
                let entries = match index {
                    FlatIndexLocation::Path(url) => {
                        let path = url
                            .to_file_path()
                            .map_err(|()| FlatIndexError::NonFileUrl(url.to_url()))?;
                        Self::read_from_directory(&path, index)
                            .map_err(|err| FlatIndexError::FindLinksDirectory(path.clone(), err))?
                    }
                    FlatIndexLocation::Url(url) => self
                        .read_from_url(url, index)
                        .await
                        .map_err(|err| FlatIndexError::FindLinksUrl(url.to_url(), err))?,
                };
                if entries.is_empty() {
                    warn!("No packages found in `--find-links` entry: {}", index);
                } else {
                    debug!(
                        "Found {} package{} in `--find-links` entry: {}",
                        entries.len(),
                        if entries.len() == 1 { "" } else { "s" },
                        index
                    );
                }
                Ok::<FlatIndexEntries, FlatIndexError>(entries)
            })
            .buffered(16);

        let mut results = FlatIndexEntries::default();
        while let Some(entries) = fetches.next().await.transpose()? {
            results.extend(entries);
        }
        Ok(results)
    }

    /// Read a flat remote index from a `--find-links` URL.
    async fn read_from_url(
        &self,
        url: &Url,
        flat_index: &FlatIndexLocation,
    ) -> Result<FlatIndexEntries, Error> {
        let cache_entry = self.cache.entry(
            CacheBucket::FlatIndex,
            "html",
            format!("{}.msgpack", cache_key::cache_digest(&url.to_string())),
        );
        let cache_control = match self.client.connectivity() {
            Connectivity::Online => CacheControl::from(
                self.cache
                    .freshness(&cache_entry, None)
                    .map_err(ErrorKind::Io)?,
            ),
            Connectivity::Offline => CacheControl::AllowStale,
        };

        let flat_index_request = self
            .client
            .uncached_client(url)
            .get(url.clone())
            .header("Accept-Encoding", "gzip")
            .header("Accept", "text/html")
            .build()
            .map_err(ErrorKind::from)?;
        let parse_simple_response = |response: Response| {
            async {
                // Use the response URL, rather than the request URL, as the base for relative URLs.
                // This ensures that we handle redirects and other URL transformations correctly.
                let url = response.url().clone();

                let text = response.text().await.map_err(ErrorKind::from)?;
                let SimpleHtml { base, files } = SimpleHtml::parse(&text, &url)
                    .map_err(|err| Error::from_html_err(err, url.clone()))?;

                let files: Vec<File> = files
                    .into_iter()
                    .filter_map(|file| {
                        match File::try_from(file, base.as_url()) {
                            Ok(file) => Some(file),
                            Err(err) => {
                                // Ignore files with unparsable version specifiers.
                                warn!("Skipping file in {url}: {err}");
                                None
                            }
                        }
                    })
                    .collect();
                Ok::<Vec<File>, CachedClientError<Error>>(files)
            }
            .boxed_local()
            .instrument(info_span!("parse_flat_index_html", url = % url))
        };
        let response = self
            .client
            .cached_client()
            .get_serde(
                flat_index_request,
                &cache_entry,
                cache_control,
                parse_simple_response,
            )
            .await;
        match response {
            Ok(files) => {
                let files = files
                    .into_iter()
                    .filter_map(|file| {
                        Some((
                            DistFilename::try_from_normalized_filename(&file.filename)?,
                            file,
                            IndexUrl::from(flat_index.clone()),
                        ))
                    })
                    .collect();
                Ok(FlatIndexEntries::from_entries(files))
            }
            Err(CachedClientError::Client(err)) if err.is_offline() => {
                Ok(FlatIndexEntries::offline())
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Read a flat remote index from a `--find-links` directory.
    fn read_from_directory(
        path: &Path,
        flat_index: &FlatIndexLocation,
    ) -> Result<FlatIndexEntries, FindLinksDirectoryError> {
        let mut dists = Vec::new();
        for entry in fs_err::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;

            if metadata.is_dir() {
                continue;
            }

            if metadata.is_symlink() {
                let Ok(target) = entry.path().read_link() else {
                    warn!(
                        "Skipping unreadable symlink in `--find-links` directory: {}",
                        entry.path().display()
                    );
                    continue;
                };
                if target.is_dir() {
                    continue;
                }
            }

            let Ok(filename) = entry.file_name().into_string() else {
                warn!(
                    "Skipping non-UTF-8 filename in `--find-links` directory: {}",
                    entry.file_name().to_string_lossy()
                );
                continue;
            };

            // SAFETY: The index path is itself constructed from a URL.
            let url = Url::from_file_path(entry.path()).unwrap();

            let file = File {
                dist_info_metadata: false,
                filename: filename.to_string(),
                hashes: Vec::new(),
                requires_python: None,
                size: None,
                upload_time_utc_ms: None,
                url: FileLocation::AbsoluteUrl(UrlString::from(url)),
                yanked: None,
            };

            let Some(filename) = DistFilename::try_from_normalized_filename(&filename) else {
                debug!(
                    "Ignoring `--find-links` entry (expected a wheel or source distribution filename): {}",
                    entry.path().display()
                );
                continue;
            };
            dists.push((filename, file, IndexUrl::from(flat_index.clone())));
        }
        Ok(FlatIndexEntries::from_entries(dists))
    }
}

use std::path::{Path, PathBuf};

use futures::{FutureExt, StreamExt};
use reqwest::Response;
use tracing::{Instrument, debug, info_span, warn};
use url::Url;

use uv_cache::{Cache, CacheBucket};
use uv_cache_key::cache_digest;
use uv_distribution_types::{File, FileLocation, IndexEntryFilename, IndexUrl, UrlString};
use uv_pypi_types::HashDigests;
use uv_redacted::DisplaySafeUrl;
use uv_small_str::SmallString;

use crate::cached_client::{CacheControl, CachedClientError};
use crate::html::SimpleHtml;
use crate::{CachedClient, Connectivity, Error, ErrorKind, OwnedArchive};

#[derive(Debug, thiserror::Error)]
pub enum FlatIndexError {
    #[error("Expected a file URL, but received: {0}")]
    NonFileUrl(DisplaySafeUrl),

    #[error("Failed to read `--find-links` directory: {0}")]
    FindLinksDirectory(PathBuf, #[source] FindLinksDirectoryError),

    #[error("Failed to read `--find-links` URL: {0}")]
    FindLinksUrl(DisplaySafeUrl, #[source] Error),
}

#[derive(Debug, thiserror::Error)]
pub enum FindLinksDirectoryError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    VerbatimUrl(#[from] uv_pep508::VerbatimUrlError),
}

/// An entry in a `--find-links` index.
#[derive(Debug, Clone)]
pub struct FlatIndexEntry {
    pub filename: IndexEntryFilename,
    pub file: File,
    pub index: IndexUrl,
}

#[derive(Debug, Default, Clone)]
pub struct FlatIndexEntries {
    /// The list of `--find-links` entries.
    pub entries: Vec<FlatIndexEntry>,
    /// Whether any `--find-links` entries could not be resolved due to a lack of network
    /// connectivity.
    pub offline: bool,
}

impl FlatIndexEntries {
    /// Create a [`FlatIndexEntries`] from a list of `--find-links` entries.
    fn from_entries(entries: Vec<FlatIndexEntry>) -> Self {
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
    client: &'a CachedClient,
    connectivity: Connectivity,
    cache: &'a Cache,
}

impl<'a> FlatIndexClient<'a> {
    /// Create a new [`FlatIndexClient`].
    pub fn new(client: &'a CachedClient, connectivity: Connectivity, cache: &'a Cache) -> Self {
        Self {
            client,
            connectivity,
            cache,
        }
    }

    /// Read the directories and flat remote indexes from `--find-links`.
    pub async fn fetch_all(
        &self,
        indexes: impl Iterator<Item = &IndexUrl>,
    ) -> Result<FlatIndexEntries, FlatIndexError> {
        let mut fetches = futures::stream::iter(indexes)
            .map(async |index| {
                let entries = self.fetch_index(index).await?;
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
        results
            .entries
            .sort_by(|a, b| a.filename.cmp(&b.filename).then(a.index.cmp(&b.index)));
        Ok(results)
    }

    /// Fetch a flat remote index from a `--find-links` URL.
    pub async fn fetch_index(&self, index: &IndexUrl) -> Result<FlatIndexEntries, FlatIndexError> {
        match index {
            IndexUrl::Path(url) => {
                let path = url
                    .to_file_path()
                    .map_err(|()| FlatIndexError::NonFileUrl(url.to_url()))?;
                Self::read_from_directory(&path, index)
                    .map_err(|err| FlatIndexError::FindLinksDirectory(path.clone(), err))
            }
            IndexUrl::Pypi(url) | IndexUrl::Url(url) => self
                .read_from_url(url, index)
                .await
                .map_err(|err| FlatIndexError::FindLinksUrl(url.to_url(), err)),
        }
    }

    /// Read a flat remote index from a `--find-links` URL.
    async fn read_from_url(
        &self,
        url: &DisplaySafeUrl,
        flat_index: &IndexUrl,
    ) -> Result<FlatIndexEntries, Error> {
        let cache_entry = self.cache.entry(
            CacheBucket::FlatIndex,
            "html",
            format!("{}.msgpack", cache_digest(&url.to_string())),
        );
        let cache_control = match self.connectivity {
            Connectivity::Online => CacheControl::from(
                self.cache
                    .freshness(&cache_entry, None, None)
                    .map_err(ErrorKind::Io)?,
            ),
            Connectivity::Offline => CacheControl::AllowStale,
        };

        let flat_index_request = self
            .client
            .uncached()
            .for_host(url)
            .get(Url::from(url.clone()))
            .header("Accept-Encoding", "gzip")
            .header("Accept", "text/html")
            .build()
            .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;
        let parse_simple_response = |response: Response| {
            async {
                // Use the response URL, rather than the request URL, as the base for relative URLs.
                // This ensures that we handle redirects and other URL transformations correctly.
                let url = DisplaySafeUrl::from(response.url().clone());

                let text = response
                    .text()
                    .await
                    .map_err(|err| ErrorKind::from_reqwest(url.clone(), err))?;
                let SimpleHtml { base, files } = SimpleHtml::parse(&text, &url)
                    .map_err(|err| Error::from_html_err(err, url.clone()))?;

                // Convert to a reference-counted string.
                let base = SmallString::from(base.as_str());

                let unarchived: Vec<File> = files
                    .into_iter()
                    .filter_map(|file| {
                        match File::try_from_pypi(file, &base) {
                            Ok(file) => Some(file),
                            Err(err) => {
                                // Ignore files with unparsable version specifiers.
                                warn!("Skipping file in {}: {err}", &url);
                                None
                            }
                        }
                    })
                    .collect();
                OwnedArchive::from_unarchived(&unarchived)
            }
            .boxed_local()
            .instrument(info_span!("parse_flat_index_html", url = % url))
        };
        let response = self
            .client
            .get_cacheable_with_retry(
                flat_index_request,
                &cache_entry,
                cache_control,
                parse_simple_response,
            )
            .await;
        match response {
            Ok(files) => {
                let files = files
                    .iter()
                    .map(|file| {
                        rkyv::deserialize::<File, rkyv::rancor::Error>(file)
                            .expect("archived version always deserializes")
                    })
                    .filter_map(|file| {
                        Some(FlatIndexEntry {
                            filename: IndexEntryFilename::try_from_normalized_filename(
                                &file.filename,
                            )?,
                            file,
                            index: flat_index.clone(),
                        })
                    })
                    .collect();
                Ok(FlatIndexEntries::from_entries(files))
            }
            Err(CachedClientError::Client { err, .. }) if err.is_offline() => {
                Ok(FlatIndexEntries::offline())
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Read a flat remote index from a `--find-links` directory.
    fn read_from_directory(
        path: &Path,
        flat_index: &IndexUrl,
    ) -> Result<FlatIndexEntries, FindLinksDirectoryError> {
        // The path context is provided by the caller.
        #[allow(clippy::disallowed_methods)]
        let entries = std::fs::read_dir(path)?;

        let mut dists = Vec::new();
        for entry in entries {
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

            let filename = entry.file_name();
            let Some(filename) = filename.to_str() else {
                warn!(
                    "Skipping non-UTF-8 filename in `--find-links` directory: {}",
                    filename.to_string_lossy()
                );
                continue;
            };

            // SAFETY: The index path is itself constructed from a URL.
            let url = DisplaySafeUrl::from_file_path(entry.path()).unwrap();

            let file = File {
                dist_info_metadata: false,
                filename: filename.into(),
                hashes: HashDigests::empty(),
                requires_python: None,
                size: None,
                upload_time_utc_ms: None,
                url: FileLocation::AbsoluteUrl(UrlString::from(url)),
                yanked: None,
                zstd: None,
            };

            // Try to parse as a distribution filename first
            let Some(filename) = IndexEntryFilename::try_from_normalized_filename(filename) else {
                debug!(
                    "Ignoring `--find-links` entry (expected a wheel, source distribution, or variants.json filename): {}",
                    entry.path().display()
                );
                continue;
            };
            dists.push(FlatIndexEntry {
                filename,
                file,
                index: flat_index.clone(),
            });
        }
        Ok(FlatIndexEntries::from_entries(dists))
    }
}

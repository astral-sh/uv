use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::path::PathBuf;

use futures::{FutureExt, StreamExt};
use reqwest::Response;
use rustc_hash::FxHashMap;
use tracing::{debug, info_span, instrument, warn, Instrument};
use url::Url;

use distribution_filename::DistFilename;
use distribution_types::{
    BuiltDist, Dist, File, FileLocation, FlatIndexLocation, IndexUrl, PrioritizedDist,
    RegistryBuiltDist, RegistrySourceDist, SourceDist,
};
use pep440_rs::Version;
use platform_tags::Tags;
use pypi_types::{Hashes, Yanked};
use uv_cache::{Cache, CacheBucket};
use uv_normalize::PackageName;

use crate::cached_client::{CacheControl, CachedClientError};
use crate::html::SimpleHtml;
use crate::{Connectivity, Error, ErrorKind, RegistryClient};

#[derive(Debug, thiserror::Error)]
pub enum FlatIndexError {
    #[error("Failed to read `--find-links` directory: {0}")]
    FindLinksDirectory(PathBuf, #[source] std::io::Error),

    #[error("Failed to read `--find-links` URL: {0}")]
    FindLinksUrl(Url, #[source] Error),
}

#[derive(Debug, Default, Clone)]
pub struct FlatIndexEntries {
    /// The list of `--find-links` entries.
    entries: Vec<(DistFilename, File, IndexUrl)>,
    /// Whether any `--find-links` entries could not be resolved due to a lack of network
    /// connectivity.
    offline: bool,
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
                    FlatIndexLocation::Path(path) => Self::read_from_directory(path)
                        .map_err(|err| FlatIndexError::FindLinksDirectory(path.clone(), err))?,
                    FlatIndexLocation::Url(url) => self
                        .read_from_url(url)
                        .await
                        .map_err(|err| FlatIndexError::FindLinksUrl(url.clone(), err))?,
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
    async fn read_from_url(&self, url: &Url) -> Result<FlatIndexEntries, Error> {
        let cache_entry = self.cache.entry(
            CacheBucket::FlatIndex,
            "html",
            format!("{}.msgpack", cache_key::digest(&url.to_string())),
        );
        let cache_control = match self.client.connectivity() {
            Connectivity::Online => CacheControl::from(
                self.cache
                    .freshness(&cache_entry, None)
                    .map_err(ErrorKind::Io)?,
            ),
            Connectivity::Offline => CacheControl::AllowStale,
        };

        let cached_client = self.client.cached_client();

        let flat_index_request = cached_client
            .uncached()
            .get(url.clone())
            .header("Accept-Encoding", "gzip")
            .header("Accept", "text/html")
            .build()
            .map_err(ErrorKind::RequestError)?;
        let parse_simple_response = |response: Response| {
            async {
                let text = response.text().await.map_err(ErrorKind::RequestError)?;
                let SimpleHtml { base, files } = SimpleHtml::parse(&text, url)
                    .map_err(|err| Error::from_html_err(err, url.clone()))?;

                let files: Vec<File> = files
                    .into_iter()
                    .filter_map(|file| {
                        match File::try_from(file, base.as_url().as_str()) {
                            Ok(file) => Some(file),
                            Err(err) => {
                                // Ignore files with unparseable version specifiers.
                                warn!("Skipping file in {url}: {err}");
                                None
                            }
                        }
                    })
                    .collect();
                Ok::<Vec<File>, CachedClientError<Error>>(files)
            }
            .boxed()
            .instrument(info_span!("parse_flat_index_html", url = % url))
        };
        let response = cached_client
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
                            IndexUrl::Url(url.clone()),
                        ))
                    })
                    .collect();
                Ok(FlatIndexEntries::from_entries(files))
            }
            Err(CachedClientError::Client(err)) if matches!(err.kind(), ErrorKind::Offline(_)) => {
                Ok(FlatIndexEntries::offline())
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Read a flat remote index from a `--find-links` directory.
    fn read_from_directory(path: &PathBuf) -> Result<FlatIndexEntries, std::io::Error> {
        // Absolute paths are required for the URL conversion.
        let path = fs_err::canonicalize(path)?;

        let mut dists = Vec::new();
        for entry in fs_err::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if !metadata.is_file() {
                continue;
            }

            let Ok(filename) = entry.file_name().into_string() else {
                warn!(
                    "Skipping non-UTF-8 filename in `--find-links` directory: {}",
                    entry.file_name().to_string_lossy()
                );
                continue;
            };

            let file = File {
                dist_info_metadata: None,
                filename: filename.to_string(),
                hashes: Hashes::default(),
                requires_python: None,
                size: None,
                upload_time_utc_ms: None,
                url: FileLocation::Path(entry.path().to_path_buf()),
                yanked: None,
            };

            let Some(filename) = DistFilename::try_from_normalized_filename(&filename) else {
                debug!(
                    "Ignoring `--find-links` entry (expected a wheel or source distribution filename): {}",
                    entry.path().display()
                );
                continue;
            };
            dists.push((filename, file, IndexUrl::Pypi));
        }
        Ok(FlatIndexEntries::from_entries(dists))
    }
}

/// A set of [`PrioritizedDist`] from a `--find-links` entry, indexed by [`PackageName`]
/// and [`Version`].
#[derive(Debug, Clone, Default)]
pub struct FlatIndex {
    /// The list of [`FlatDistributions`] from the `--find-links` entries, indexed by package name.
    index: FxHashMap<PackageName, FlatDistributions>,
    /// Whether any `--find-links` entries could not be resolved due to a lack of network
    /// connectivity.
    offline: bool,
}

impl FlatIndex {
    /// Collect all files from a `--find-links` target into a [`FlatIndex`].
    #[instrument(skip_all)]
    pub fn from_entries(entries: FlatIndexEntries, tags: &Tags) -> Self {
        // Collect compatible distributions.
        let mut index = FxHashMap::default();
        for (filename, file, url) in entries.entries {
            let distributions = index.entry(filename.name().clone()).or_default();
            Self::add_file(distributions, file, filename, tags, url);
        }

        // Collect offline entries.
        let offline = entries.offline;

        Self { index, offline }
    }

    fn add_file(
        distributions: &mut FlatDistributions,
        file: File,
        filename: DistFilename,
        tags: &Tags,
        index: IndexUrl,
    ) {
        // No `requires-python` here: for source distributions, we don't have that information;
        // for wheels, we read it lazily only when selected.
        match filename {
            DistFilename::WheelFilename(filename) => {
                let compatibility = filename.compatibility(tags);
                let version = filename.version.clone();

                let dist = Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                    filename,
                    file: Box::new(file),
                    index,
                }));
                match distributions.0.entry(version) {
                    Entry::Occupied(mut entry) => {
                        entry.get_mut().insert_built(
                            dist,
                            None,
                            Yanked::default(),
                            None,
                            compatibility.into(),
                        );
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(PrioritizedDist::from_built(
                            dist,
                            None,
                            Yanked::default(),
                            None,
                            compatibility.into(),
                        ));
                    }
                }
            }
            DistFilename::SourceDistFilename(filename) => {
                let dist = Dist::Source(SourceDist::Registry(RegistrySourceDist {
                    filename: filename.clone(),
                    file: Box::new(file),
                    index,
                }));
                match distributions.0.entry(filename.version.clone()) {
                    Entry::Occupied(mut entry) => {
                        entry
                            .get_mut()
                            .insert_source(dist, None, Yanked::default(), None);
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(PrioritizedDist::from_source(
                            dist,
                            None,
                            Yanked::default(),
                            None,
                        ));
                    }
                }
            }
        }
    }

    /// Get the [`FlatDistributions`] for the given package name.
    pub fn get(&self, package_name: &PackageName) -> Option<&FlatDistributions> {
        self.index.get(package_name)
    }

    /// Returns `true` if there are any offline `--find-links` entries.
    pub fn offline(&self) -> bool {
        self.offline
    }
}

/// A set of [`PrioritizedDist`] from a `--find-links` entry for a single package, indexed
/// by [`Version`].
#[derive(Debug, Clone, Default)]
pub struct FlatDistributions(BTreeMap<Version, PrioritizedDist>);

impl FlatDistributions {
    pub fn iter(&self) -> impl Iterator<Item = (&Version, &PrioritizedDist)> {
        self.0.iter()
    }

    pub fn remove(&mut self, version: &Version) -> Option<PrioritizedDist> {
        self.0.remove(version)
    }
}

impl IntoIterator for FlatDistributions {
    type Item = (Version, PrioritizedDist);
    type IntoIter = std::collections::btree_map::IntoIter<Version, PrioritizedDist>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl From<FlatDistributions> for BTreeMap<Version, PrioritizedDist> {
    fn from(distributions: FlatDistributions) -> Self {
        distributions.0
    }
}

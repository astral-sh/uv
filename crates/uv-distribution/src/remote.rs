use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::path::Path;
use std::sync::Arc;

use reqwest::Body;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_util::io::ReaderStream;
use tracing::{debug, instrument, warn};
use url::Url;

use uv_auth::PyxTokenStore;
use uv_cache_key::RepositoryUrl;
use uv_client::{MetadataFormat, VersionFiles};
use uv_configuration::BuildOptions;
use uv_distribution_filename::{DistFilename, SourceDistFilename, WheelFilename};
use uv_distribution_types::{
    BuildableSource, File, HashComparison, HashPolicy, IncompatibleSource, IncompatibleWheel,
    IndexFormat, IndexMetadata, IndexUrl, PrioritizedDist, RegistryBuiltWheel, RegistrySourceDist,
    SourceDist, SourceDistCompatibility, SourceUrl, WheelCompatibility,
};
use uv_git_types::{GitOid, GitUrl};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::VerbatimUrl;
use uv_platform_tags::{TagCompatibility, Tags};
use uv_pypi_types::HashDigest;
use uv_static::EnvVars;
use uv_types::{BuildContext, HashStrategy};

use crate::Error;
use crate::distribution_database::ManagedClient;

/// A resolver for remote Git-based indexes.
pub(crate) struct RemoteCacheResolver<'a, Context: BuildContext> {
    build_context: &'a Context,
    /// Cache for Git index entries.
    index_cache: Arc<Mutex<GitIndexCache>>,
    /// Cache for server-provided cache keys.
    key_cache: Arc<Mutex<CacheKeyCache>>,
    store: Option<PyxTokenStore>,
    workspace: Option<String>,
}

/// A cache for server-provided cache keys.
///
/// Maps (repository, commit, subdirectory) to the server-computed cache key.
type CacheKeyCache = FxHashMap<CacheKeyRequest, String>;

/// Request body for fetching a cache key from the server.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
struct CacheKeyRequest {
    repository: String,
    commit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    subdirectory: Option<String>,
}

/// Response from the cache key endpoint.
#[derive(Debug, Deserialize)]
struct CacheKeyResponse {
    key: String,
}

impl<'a, T: BuildContext> RemoteCacheResolver<'a, T> {
    /// Initialize a [`RemoteCacheResolver`] from a [`BuildContext`].
    pub(crate) fn new(build_context: &'a T) -> Self {
        Self {
            build_context,
            index_cache: Arc::default(),
            key_cache: Arc::default(),
            store: PyxTokenStore::from_settings().ok(),
            workspace: std::env::var(EnvVars::PYX_CACHE_WORKSPACE).ok(),
        }
    }

    /// Fetch the cache key from the server for the given Git source.
    async fn get_cache_key(
        &self,
        repository: &RepositoryUrl,
        commit: GitOid,
        subdirectory: Option<&Path>,
        client: &ManagedClient<'a>,
    ) -> Result<Option<String>, Error> {
        let Some(store) = &self.store else {
            return Ok(None);
        };

        // Build the request.
        let request = CacheKeyRequest {
            repository: repository.to_string(),
            commit: commit.to_string(),
            subdirectory: subdirectory.and_then(|p| p.to_str()).map(String::from),
        };

        // Check the local cache first.
        {
            let cache = self.key_cache.lock().await;
            if let Some(key) = cache.get(&request) {
                return Ok(Some(key.clone()));
            }
        }

        // Build the API URL with query parameters.
        let Some(workspace) = &self.workspace else {
            return Ok(None);
        };
        let url = {
            let mut url = store.api().clone();
            url.set_path(&format!("v1/cache/{workspace}/key"));
            url.query_pairs_mut()
                .append_pair("repository", &request.repository)
                .append_pair("commit", &request.commit);
            if let Some(ref subdir) = request.subdirectory {
                url.query_pairs_mut().append_pair("subdirectory", subdir);
            }
            url
        };
        debug!("Fetching cache key from: {url}");

        // Build and send the request.
        let response = match client
            .unmanaged
            .uncached_client(&url)
            .get(Url::from(url.clone()))
            .send()
            .await
        {
            Ok(response) => response,
            Err(err) => {
                debug!("Failed to fetch cache key: {err}");
                return Ok(None);
            }
        };

        if !response.status().is_success() {
            debug!(
                "Failed to fetch cache key: {} {}",
                response.status(),
                response.text().await.unwrap_or_default()
            );
            return Ok(None);
        }

        let response: CacheKeyResponse = response.json().await?;

        // Cache the key.
        {
            let mut cache = self.key_cache.lock().await;
            cache.insert(request, response.key.clone());
        }

        Ok(Some(response.key))
    }

    /// Create a cache entry on the server and return the cache key.
    ///
    /// Unlike [`get_cache_key`], this creates the necessary server-side resources
    /// (registry, view) that are required before uploading wheels.
    async fn create_cache_entry(
        &self,
        repository: &RepositoryUrl,
        commit: GitOid,
        subdirectory: Option<&Path>,
        client: &ManagedClient<'a>,
    ) -> Result<Option<String>, Error> {
        let Some(store) = &self.store else {
            return Ok(None);
        };

        // Build the request.
        let request = CacheKeyRequest {
            repository: repository.to_string(),
            commit: commit.to_string(),
            subdirectory: subdirectory.and_then(|p| p.to_str()).map(String::from),
        };

        // Build the API URL.
        let Some(workspace) = &self.workspace else {
            return Ok(None);
        };
        let url = {
            let mut url = store.api().clone();
            url.set_path(&format!("v1/cache/{workspace}"));
            url
        };
        debug!("Creating cache entry at: {url}");

        // Build and send the request.
        let body = serde_json::to_vec(&request).expect("failed to serialize cache key request");
        let response = match client
            .unmanaged
            .uncached_client(&url)
            .post(Url::from(url.clone()))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
        {
            Ok(response) => response,
            Err(err) => {
                debug!("Failed to create cache entry: {err}");
                return Ok(None);
            }
        };

        if !response.status().is_success() {
            debug!(
                "Failed to create cache entry: {} {}",
                response.status(),
                response.text().await.unwrap_or_default()
            );
            return Ok(None);
        }

        let response: CacheKeyResponse = response.json().await?;

        // Cache the key.
        {
            let mut cache = self.key_cache.lock().await;
            cache.insert(request, response.key.clone());
        }

        Ok(Some(response.key))
    }

    /// Return the cached Git index for the given distribution, if any.
    pub(crate) async fn get_cached_distribution(
        &self,
        source: &BuildableSource<'_>,
        tags: Option<&Tags>,
        client: &ManagedClient<'a>,
    ) -> Result<Option<GitIndex>, Error> {
        // Fetch the entries for the given distribution.
        let entries = self.get_or_fetch_index(source, client).await?;
        if entries.is_empty() {
            return Ok(None);
        }

        // Create the index.
        let index = GitIndex::from_entries(
            entries,
            tags,
            &HashStrategy::default(),
            self.build_context.build_options(),
        );
        Ok(Some(index))
    }

    /// Fetch the remote Git index for the given distribution.
    async fn get_or_fetch_index(
        &self,
        source: &BuildableSource<'_>,
        client: &ManagedClient<'a>,
    ) -> Result<Vec<GitIndexEntry>, Error> {
        #[derive(Debug)]
        struct BuildableGitSource<'a> {
            git: &'a GitUrl,
            subdirectory: Option<&'a Path>,
            name: Option<&'a PackageName>,
        }

        let Some(workspace) = &self.workspace else {
            return Ok(Vec::default());
        };

        let source = match source {
            BuildableSource::Dist(SourceDist::Git(dist)) => BuildableGitSource {
                git: &dist.git,
                subdirectory: dist.subdirectory.as_deref(),
                name: Some(&dist.name),
            },
            BuildableSource::Url(SourceUrl::Git(url)) => BuildableGitSource {
                git: url.git,
                subdirectory: url.subdirectory,
                name: None,
            },
            _ => {
                return Ok(Vec::default());
            }
        };

        let Some(precise) = self.build_context.git().get_precise(source.git) else {
            return Ok(Vec::default());
        };

        // Fetch the cache key from the server.
        let repository = RepositoryUrl::new(source.git.repository());
        let Some(cache_key) = self
            .get_cache_key(&repository, precise, source.subdirectory, client)
            .await?
        else {
            return Ok(Vec::default());
        };

        // Build the index URL using the server-provided cache key.
        let Some(store) = &self.store else {
            return Ok(Vec::default());
        };
        let index = IndexUrl::from(
            VerbatimUrl::parse_url(format!(
                "{}/v1/cache/{workspace}/{cache_key}",
                store.api().as_str().trim_end_matches('/'),
            ))
            .unwrap(),
        );
        debug!("Using remote Git index URL: {}", index);

        // Determine the package name.
        let name = if let Some(name) = source.name {
            Cow::Borrowed(name)
        } else {
            // Fetch the list of packages from the Simple API.
            let index_metadata = client
                .managed(|client| client.fetch_simple_index(&index))
                .await?;

            // Ensure that the index contains exactly one package.
            let mut packages = index_metadata.iter().cloned();
            let Some(name) = packages.next() else {
                debug!("Remote Git index at `{index}` contains no packages");
                return Ok(Vec::default());
            };
            if packages.next().is_some() {
                debug!("Remote Git index at `{index}` contains multiple packages");
                return Ok(Vec::default());
            }
            Cow::Owned(name)
        };

        // Store the index entries in a cache, to avoid redundant fetches.
        {
            let cache = self.index_cache.lock().await;
            if let Some(entries) = cache.get(&index) {
                return Ok(entries.to_vec());
            }
        }

        // Perform a remote fetch via the Simple API.
        let metadata = IndexMetadata {
            url: index.clone(),
            format: IndexFormat::Simple,
        };
        let archives = client
            .manual(|client, semaphore| {
                client.simple_detail(
                    name.as_ref(),
                    Some(metadata.as_ref()),
                    self.build_context.capabilities(),
                    semaphore,
                )
            })
            .await?;

        // Collect the files from the remote index.
        let mut entries = Vec::new();
        for (_, archive) in archives {
            let MetadataFormat::Simple(archive) = archive else {
                continue;
            };
            for datum in archive.iter().rev() {
                let files = rkyv::deserialize::<VersionFiles, rkyv::rancor::Error>(&datum.files)
                    .expect("archived version files always deserializes");
                for (filename, file) in files.all() {
                    if *filename.name() != *name {
                        warn!(
                            "Skipping file `{filename}` from remote Git index at `{index}` due to name mismatch (expected: `{name}`)"
                        );
                        continue;
                    }

                    entries.push(GitIndexEntry {
                        filename,
                        file,
                        index: index.clone(),
                    });
                }
            }
        }

        // Write to the cache.
        {
            let mut cache = self.index_cache.lock().await;
            cache.insert(index.clone(), entries.clone());
        }

        Ok(entries)
    }

    /// Upload a built wheel to the remote cache.
    pub(crate) async fn upload_to_cache(
        &self,
        source: &BuildableSource<'_>,
        wheel_path: &Path,
        filename: &WheelFilename,
        client: &ManagedClient<'a>,
    ) -> Result<(), Error> {
        #[derive(Debug)]
        struct BuildableGitSource<'a> {
            git: &'a GitUrl,
            subdirectory: Option<&'a Path>,
        }

        let Some(workspace) = &self.workspace else {
            return Ok(());
        };

        let Some(store) = &self.store else {
            return Ok(());
        };

        let source = match source {
            BuildableSource::Dist(SourceDist::Git(dist)) => BuildableGitSource {
                git: &dist.git,
                subdirectory: dist.subdirectory.as_deref(),
            },
            BuildableSource::Url(SourceUrl::Git(url)) => BuildableGitSource {
                git: url.git,
                subdirectory: url.subdirectory,
            },
            _ => {
                return Ok(());
            }
        };

        let Some(precise) = self.build_context.git().get_precise(source.git) else {
            return Ok(());
        };

        // Create the cache entry on the server (or get existing key).
        let repository = RepositoryUrl::new(source.git.repository());
        let Some(cache_key) = self
            .create_cache_entry(&repository, precise, source.subdirectory, client)
            .await?
        else {
            return Ok(());
        };

        // Build the upload URL using the server-provided cache key.
        let url = {
            let mut url = store.api().clone();
            url.set_path(&format!("v1/cache/{workspace}/{cache_key}"));
            url
        };
        debug!("Uploading wheel to remote cache: {url}");

        // Get the file size for the Content-Length header.
        let file_size = fs_err::tokio::metadata(wheel_path)
            .await
            .map_err(Error::CacheRead)?
            .len();

        // Open the wheel file.
        let file = fs_err::tokio::File::open(wheel_path)
            .await
            .map_err(Error::CacheRead)?;
        let stream = ReaderStream::new(file);
        let body = Body::wrap_stream(stream);

        // Build the multipart form with streaming body.
        let part = reqwest::multipart::Part::stream_with_length(body, file_size)
            .file_name(filename.to_string());
        let form = reqwest::multipart::Form::new().part("content", part);

        // Build and send the request.
        let response = match client
            .unmanaged
            .uncached_client(&url)
            .post(Url::from(url.clone()))
            .multipart(form)
            .send()
            .await
        {
            Ok(response) => response,
            Err(err) => {
                warn!("Failed to upload wheel to cache: {err}");
                return Ok(());
            }
        };

        if !response.status().is_success() {
            warn!(
                "Failed to upload wheel to cache: {} {}",
                response.status(),
                response.text().await.unwrap_or_default()
            );
        }

        Ok(())
    }
}

/// An entry in a remote Git index.
#[derive(Debug, Clone)]
struct GitIndexEntry {
    filename: DistFilename,
    file: File,
    index: IndexUrl,
}

/// A set of [`PrioritizedDist`] from a Git index.
///
/// In practice, it's assumed that the [`GitIndex`] will only contain distributions for a single
/// package.
#[derive(Debug, Clone, Default)]
pub(crate) struct GitIndex(FxHashMap<PackageName, GitIndexDistributions>);

impl GitIndex {
    /// Collect all files from a Git index.
    #[instrument(skip_all)]
    fn from_entries(
        entries: Vec<GitIndexEntry>,
        tags: Option<&Tags>,
        hasher: &HashStrategy,
        build_options: &BuildOptions,
    ) -> Self {
        let mut index = FxHashMap::<PackageName, GitIndexDistributions>::default();
        for entry in entries {
            let distributions = index.entry(entry.filename.name().clone()).or_default();
            distributions.add_file(
                entry.file,
                entry.filename,
                tags,
                hasher,
                build_options,
                entry.index,
            );
        }
        Self(index)
    }

    /// Returns an [`Iterator`] over the distributions.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &PrioritizedDist> {
        self.0
            .iter()
            .flat_map(|(.., distributions)| distributions.0.iter().map(|(.., dist)| dist))
    }
}

/// A set of [`PrioritizedDist`] from a Git index, indexed by [`Version`].
#[derive(Debug, Clone, Default)]
pub(crate) struct GitIndexDistributions(BTreeMap<Version, PrioritizedDist>);

impl GitIndexDistributions {
    /// Add the given [`File`] to the [`GitIndexDistributions`] for the given package.
    fn add_file(
        &mut self,
        file: File,
        filename: DistFilename,
        tags: Option<&Tags>,
        hasher: &HashStrategy,
        build_options: &BuildOptions,
        index: IndexUrl,
    ) {
        // TODO(charlie): Incorporate `Requires-Python`, yanked status, etc.
        match filename {
            DistFilename::WheelFilename(filename) => {
                let version = filename.version.clone();

                let compatibility = Self::wheel_compatibility(
                    &filename,
                    file.hashes.as_slice(),
                    tags,
                    hasher,
                    build_options,
                );
                let dist = RegistryBuiltWheel {
                    filename,
                    file: Box::new(file),
                    index,
                };
                match self.0.entry(version) {
                    Entry::Occupied(mut entry) => {
                        entry.get_mut().insert_built(dist, vec![], compatibility);
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(PrioritizedDist::from_built(dist, vec![], compatibility));
                    }
                }
            }
            DistFilename::SourceDistFilename(filename) => {
                let compatibility = Self::source_dist_compatibility(
                    &filename,
                    file.hashes.as_slice(),
                    hasher,
                    build_options,
                );
                let dist = RegistrySourceDist {
                    name: filename.name.clone(),
                    version: filename.version.clone(),
                    ext: filename.extension,
                    file: Box::new(file),
                    index,
                    wheels: vec![],
                };
                match self.0.entry(filename.version) {
                    Entry::Occupied(mut entry) => {
                        entry.get_mut().insert_source(dist, vec![], compatibility);
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(PrioritizedDist::from_source(dist, vec![], compatibility));
                    }
                }
            }
        }
    }

    fn source_dist_compatibility(
        filename: &SourceDistFilename,
        hashes: &[HashDigest],
        hasher: &HashStrategy,
        build_options: &BuildOptions,
    ) -> SourceDistCompatibility {
        // Check if source distributions are allowed for this package.
        if build_options.no_build_package(&filename.name) {
            return SourceDistCompatibility::Incompatible(IncompatibleSource::NoBuild);
        }

        // Check if hashes line up.
        let hash = if let HashPolicy::Validate(required) =
            hasher.get_package(&filename.name, &filename.version)
        {
            if hashes.is_empty() {
                HashComparison::Missing
            } else if required.iter().any(|hash| hashes.contains(hash)) {
                HashComparison::Matched
            } else {
                HashComparison::Mismatched
            }
        } else {
            HashComparison::Matched
        };

        SourceDistCompatibility::Compatible(hash)
    }

    fn wheel_compatibility(
        filename: &WheelFilename,
        hashes: &[HashDigest],
        tags: Option<&Tags>,
        hasher: &HashStrategy,
        build_options: &BuildOptions,
    ) -> WheelCompatibility {
        // Check if binaries are allowed for this package.
        if build_options.no_binary_package(&filename.name) {
            return WheelCompatibility::Incompatible(IncompatibleWheel::NoBinary);
        }

        // Determine a compatibility for the wheel based on tags.
        let priority = match tags {
            Some(tags) => match filename.compatibility(tags) {
                TagCompatibility::Incompatible(tag) => {
                    return WheelCompatibility::Incompatible(IncompatibleWheel::Tag(tag));
                }
                TagCompatibility::Compatible(priority) => Some(priority),
            },
            None => None,
        };

        // Check if hashes line up.
        let hash = if let HashPolicy::Validate(required) =
            hasher.get_package(&filename.name, &filename.version)
        {
            if hashes.is_empty() {
                HashComparison::Missing
            } else if required.iter().any(|hash| hashes.contains(hash)) {
                HashComparison::Matched
            } else {
                HashComparison::Mismatched
            }
        } else {
            HashComparison::Matched
        };

        // Break ties with the build tag.
        let build_tag = filename.build_tag().cloned();

        WheelCompatibility::Compatible(hash, priority, build_tag)
    }
}

/// A map from [`IndexUrl`] to [`GitIndex`] entries found at the given URL.
#[derive(Default, Debug, Clone)]
struct GitIndexCache(FxHashMap<IndexUrl, Vec<GitIndexEntry>>);

impl GitIndexCache {
    /// Get the entries for a given index URL.
    fn get(&self, index: &IndexUrl) -> Option<&[GitIndexEntry]> {
        self.0.get(index).map(Vec::as_slice)
    }

    /// Insert the entries for a given index URL.
    fn insert(
        &mut self,
        index: IndexUrl,
        entries: Vec<GitIndexEntry>,
    ) -> Option<Vec<GitIndexEntry>> {
        self.0.insert(index, entries)
    }
}

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::path::Path;
use std::sync::Arc;

use blake2::Digest;
use rustc_hash::FxHashMap;
use tokio::sync::Mutex;
use tracing::{debug, instrument, warn};
use url::Url;

use uv_auth::{Credentials, PyxTokenStore};
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
    cache: Arc<Mutex<GitIndexCache>>,
    store: Option<PyxTokenStore>,
    workspace: Option<String>,
}

impl<'a, T: BuildContext> RemoteCacheResolver<'a, T> {
    /// Initialize a [`RemoteCacheResolver`] from a [`BuildContext`].
    pub(crate) fn new(build_context: &'a T) -> Self {
        Self {
            build_context,
            cache: Arc::default(),
            store: PyxTokenStore::from_settings().ok(),
            workspace: std::env::var(EnvVars::PYX_GIT_CACHE).ok(),
        }
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

        // Determine the cache key for the Git source.
        let cache_key = GitCacheKey {
            repository: RepositoryUrl::new(source.git.repository()),
            precise,
            subdirectory: source.subdirectory,
        };
        let digest = cache_key.digest();
        let index = IndexUrl::from(
            VerbatimUrl::parse_url(format!(
                "http://localhost:8000/v1/git/{workspace}/{}/{}/{}",
                &digest[..2],
                &digest[2..4],
                &digest[4..],
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
            let cache = self.cache.lock().await;
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
            let mut cache = self.cache.lock().await;
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

        // Determine the cache key for the Git source.
        let cache_key = GitCacheKey {
            repository: RepositoryUrl::new(source.git.repository()),
            precise,
            subdirectory: source.subdirectory,
        };
        let digest = cache_key.digest();

        // Build the upload URL.
        let url = {
            let mut url = store.api().clone();
            url.set_path(&format!(
                "v1/cache/{workspace}/{}/{}/{}",
                &digest[..2],
                &digest[2..4],
                &digest[4..],
            ));
            url
        };
        debug!("Uploading wheel to remote cache: {url}");

        // Get an access token.
        let access_token = store
            .access_token(client.unmanaged.uncached_client(&url).raw_client(), 60 * 5)
            .await
            .map_err(Error::TokenStore)?;

        let Some(access_token) = access_token else {
            debug!("No access token available; skipping cache upload");
            return Ok(());
        };

        // Read the wheel file.
        let wheel_bytes = fs_err::tokio::read(wheel_path)
            .await
            .map_err(Error::CacheRead)?;

        // Build and send the request.
        let credentials = Credentials::from(access_token);
        let request = client
            .unmanaged
            .uncached_client(&url)
            .raw_client()
            .put(Url::from(url.clone()))
            .query(&[("filename", filename.to_string())])
            .body(wheel_bytes)
            .build()?;
        let request = credentials.authenticate(request);

        let response = client
            .unmanaged
            .uncached_client(&url)
            .raw_client()
            .execute(request)
            .await?;

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

/// A cache key for a Git repository at a precise commit.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GitCacheKey<'a> {
    repository: RepositoryUrl,
    precise: GitOid,
    subdirectory: Option<&'a Path>,
}

impl GitCacheKey<'_> {
    /// Compute the digest for the Git cache key.
    fn digest(&self) -> String {
        let mut hasher = blake2::Blake2b::<blake2::digest::consts::U32>::new();
        hasher.update(self.repository.as_str().as_bytes());
        hasher.update(b"/");
        hasher.update(self.precise.as_str().as_bytes());
        if let Some(subdirectory) = self
            .subdirectory
            .and_then(|subdirectory| subdirectory.to_str())
        {
            hasher.update(b"?subdirectory=");
            hasher.update(subdirectory.as_bytes());
        }
        hex::encode(hasher.finalize())
    }
}

impl std::fmt::Display for GitCacheKey<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.repository, self.precise.as_str())?;
        if let Some(subdirectory) = &self.subdirectory {
            write!(f, "?subdirectory={}", subdirectory.display())?;
        }
        Ok(())
    }
}

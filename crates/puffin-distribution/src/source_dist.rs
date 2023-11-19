//! Fetch and build source distributions from remote sources.

use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::bail;
use fs_err::tokio as fs;
use futures::TryStreamExt;
use fxhash::FxHashMap;
use reqwest::Response;
use tempfile::TempDir;
use thiserror::Error;
use tokio::task::JoinError;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::debug;
use url::Url;
use zip::ZipArchive;

use distribution_filename::WheelFilename;
use distribution_types::direct_url::{DirectArchiveUrl, DirectGitUrl};
use distribution_types::{GitSourceDist, Identifier, SourceDist};
use install_wheel_rs::find_dist_info;
use platform_tags::Tags;
use puffin_cache::{RepositoryUrl, WheelMetadataCache};
use puffin_client::{CachedClient, CachedClientError};
use puffin_git::{Fetch, GitSource};
use puffin_normalize::PackageName;
use puffin_traits::BuildContext;
use pypi_types::Metadata21;

use crate::Reporter;

type Metadata21s = FxHashMap<WheelFilename, Metadata21>;

const BUILT_WHEELS_CACHE: &str = "built-wheels-v0";
const GIT_CACHE: &str = "git-v0";

#[derive(Debug, Error)]
pub enum SourceDistError {
    // Network error
    #[error("Failed to parse url '{0}'")]
    UrlParseError(String, #[source] url::ParseError),
    #[error("Git operation failed for {0}")]
    GitErr(#[source] anyhow::Error),
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error(transparent)]
    Client(#[from] puffin_client::Error),

    // Cache writing error
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Cache (de)serialization failed")]
    Serde(#[from] serde_json::Error),

    // Build error
    #[error("Failed to build {0}")]
    BuildError(#[source] anyhow::Error),
    #[error("Package metadata name `{metadata}` does not match given name `{given}`")]
    NameMismatch {
        given: PackageName,
        metadata: PackageName,
    },

    // Should not occur, i've only seen it when another task panicked
    #[error("The task executor is broken, did some other task panic?")]
    JoinError(#[from] JoinError),
}

/// Fetch and build a source distribution from a remote source, or from a local cache.
pub struct SourceDistCachedBuilder<'a, T: BuildContext> {
    build_context: &'a T,
    cached_client: &'a CachedClient,
    reporter: Option<Arc<dyn Reporter>>,
    tags: &'a Tags,
}

const METADATA_JSON: &str = "metadata.json";

impl<'a, T: BuildContext> SourceDistCachedBuilder<'a, T> {
    /// Initialize a [`SourceDistCachedBuilder`] from a [`BuildContext`].
    pub fn new(build_context: &'a T, cached_client: &'a CachedClient, tags: &'a Tags) -> Self {
        Self {
            build_context,
            reporter: None,
            cached_client,
            tags,
        }
    }

    /// Set the [`Reporter`] to use for this source distribution fetcher.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            reporter: Some(Arc::new(reporter)),
            ..self
        }
    }

    pub async fn download_and_build(
        &self,
        source_dist: &SourceDist,
    ) -> Result<(Metadata21, Option<Url>), SourceDistError> {
        let precise = self.precise(source_dist).await?;

        let (metadata, precise) = match &source_dist {
            SourceDist::DirectUrl(direct_url_source_dist) => {
                let filename = direct_url_source_dist
                    .url
                    .path()
                    .rsplit_once('/')
                    .map_or(direct_url_source_dist.url.path(), |(_, filename)| filename)
                    .to_string();
                let DirectArchiveUrl {
                    url: _,
                    subdirectory,
                } = DirectArchiveUrl::from(&direct_url_source_dist.url);

                let metadata = self
                    .url(
                        source_dist,
                        &filename,
                        &direct_url_source_dist.url,
                        WheelMetadataCache::Url,
                        subdirectory.as_deref(),
                    )
                    .await?;
                (
                    metadata,
                    Some(
                        precise
                            .clone()
                            .unwrap_or_else(|| direct_url_source_dist.url.clone()),
                    ),
                )
            }
            SourceDist::Registry(registry_source_dist) => {
                let url = Url::parse(&registry_source_dist.file.url).map_err(|err| {
                    SourceDistError::UrlParseError(registry_source_dist.file.url.to_string(), err)
                })?;
                let metadata = self
                    .url(
                        source_dist,
                        &registry_source_dist.file.filename,
                        &url,
                        WheelMetadataCache::Index(registry_source_dist.index.clone()),
                        None,
                    )
                    .await?;
                (
                    metadata,
                    Some(precise.clone().unwrap_or_else(|| url.clone())),
                )
            }
            SourceDist::Git(git_source_dist) => {
                let metadata = self.git(source_dist, git_source_dist).await?;
                (metadata, precise)
            }
            SourceDist::Path(_path_source_dist) => {
                todo!()
            }
        };

        Ok((metadata, precise))
    }

    #[allow(clippy::too_many_arguments)]
    async fn url(
        &self,
        source_dist: &SourceDist,
        filename: &str,
        url: &Url,
        cache_shard: WheelMetadataCache,
        subdirectory: Option<&Path>,
    ) -> Result<Metadata21, SourceDistError> {
        let cache_dir =
            cache_shard.built_wheel_cache_dir(self.build_context.cache(), filename, url);
        let cache_file = METADATA_JSON;

        let response_callback = |response| async {
            debug!("Downloading and building source distribution: {source_dist}");
            let task = self
                .reporter
                .as_ref()
                .map(|reporter| reporter.on_build_start(source_dist));
            let (temp_dir, sdist_file) = self.download_source_dist_url(response, filename).await?;
            let (wheel_filename, metadata) = self
                .build_source_dist(source_dist, temp_dir, &sdist_file, subdirectory)
                .await
                .map_err(SourceDistError::BuildError)?;
            if let Some(task) = task {
                if let Some(reporter) = self.reporter.as_ref() {
                    reporter.on_build_complete(source_dist, task);
                }
            }

            let mut metadatas = Metadata21s::default();
            metadatas.insert(wheel_filename, metadata);
            Ok(metadatas)
        };
        let req = self.cached_client.uncached().get(url.clone()).build()?;
        let metadatas = self
            .cached_client
            .get_cached_with_callback(req, &cache_dir, cache_file, response_callback)
            .await
            .map_err(|err| match err {
                CachedClientError::Callback(err) => err,
                CachedClientError::Client(err) => SourceDistError::Client(err),
            })?;

        if let Some((_key, metadata)) = metadatas
            .iter()
            .find(|(key, _metadata)| key.is_compatible(self.tags))
        {
            return Ok(metadata.clone());
        }

        // At this point, we're seeing cached metadata (fresh source dist) but the
        // wheel(s) we built previously are incompatible
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source_dist));
        let response = self
            .cached_client
            .uncached()
            .get(url.clone())
            .send()
            .await
            .map_err(puffin_client::Error::RequestMiddlewareError)?;
        let (temp_dir, sdist_file) = self.download_source_dist_url(response, filename).await?;
        let (wheel_filename, metadata) = self
            .build_source_dist(source_dist, temp_dir, &sdist_file, subdirectory)
            .await
            .map_err(SourceDistError::BuildError)?;
        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source_dist, task);
            }
        }

        // Not elegant that we have to read again here, but also not too relevant given that we
        // have to build a source dist next
        // TODO(konstin): Make this non-fatal
        let cached = fs::read(cache_dir.join(cache_file)).await?;
        let mut cached = serde_json::from_slice::<Metadata21s>(&cached)?;
        cached.insert(wheel_filename, metadata.clone());
        fs::write(cache_file, serde_json::to_vec(&cached)?).await?;
        Ok(metadata.clone())
    }

    async fn git(
        &self,
        source_dist: &SourceDist,
        git_source_dist: &GitSourceDist,
    ) -> Result<Metadata21, SourceDistError> {
        let task = self
            .reporter
            .as_ref()
            .map(|reporter| reporter.on_build_start(source_dist));

        // TODO(konstin): Can we special case when we have a git sha so we know there is no change?
        let (fetch, subdirectory) = self
            .download_source_dist_git(git_source_dist.url.clone())
            .await?;

        let git_sha = fetch
            .git()
            .precise()
            .expect("Exact commit after checkout")
            .to_string();
        let cache_dir = WheelMetadataCache::Git(RepositoryUrl::new(&git_source_dist.url))
            .built_wheel_cache_dir(self.build_context.cache(), &git_sha, &git_source_dist.url);
        let cache_file = cache_dir.join(METADATA_JSON);
        // TODO(konstin): Should we lock this file for parallel processes?
        let mut metadatas = if cache_file.is_file() {
            let cached = fs::read(&cache_file).await?;
            let metadatas = serde_json::from_slice::<Metadata21s>(&cached)?;
            // Do we have previous compatible build of this source dist?
            if let Some((_key, metadata)) = metadatas
                .iter()
                .find(|(key, _metadata)| key.is_compatible(self.tags))
            {
                return Ok(metadata.clone());
            }
            metadatas
        } else {
            Metadata21s::default()
        };

        let (filename, metadata) = self
            .build_source_dist(source_dist, None, fetch.path(), subdirectory.as_deref())
            .await
            .map_err(SourceDistError::BuildError)?;

        if metadata.name != git_source_dist.name {
            return Err(SourceDistError::NameMismatch {
                metadata: metadata.name,
                given: git_source_dist.name.clone(),
            });
        }

        // Store the metadata for this build along with all the other builds
        metadatas.insert(filename, metadata.clone());
        let cached = serde_json::to_vec(&metadatas)?;
        fs::create_dir_all(cache_dir).await?;
        fs::write(cache_file, cached).await?;

        if let Some(task) = task {
            if let Some(reporter) = self.reporter.as_ref() {
                reporter.on_build_complete(source_dist, task);
            }
        }

        Ok(metadata)
    }

    async fn download_source_dist_url(
        &self,
        response: Response,
        source_dist_filename: &str,
    ) -> Result<(Option<TempDir>, PathBuf), puffin_client::Error> {
        let reader = response
            .bytes_stream()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
            .into_async_read();
        let mut reader = tokio::io::BufReader::new(reader.compat());

        // Download the source distribution.
        let temp_dir = tempfile::tempdir_in(self.build_context.cache())?;
        let sdist_file = temp_dir.path().join(source_dist_filename);
        let mut writer = tokio::fs::File::create(&sdist_file).await?;
        tokio::io::copy(&mut reader, &mut writer).await?;
        Ok((Some(temp_dir), sdist_file))
    }

    async fn download_source_dist_git(
        &self,
        url: Url,
    ) -> Result<(Fetch, Option<PathBuf>), SourceDistError> {
        debug!("Fetching source distribution from Git: {}", url);

        let DirectGitUrl { url, subdirectory } =
            DirectGitUrl::try_from(&url).map_err(SourceDistError::GitErr)?;

        let git_dir = self.build_context.cache().join(GIT_CACHE);
        let source = if let Some(reporter) = &self.reporter {
            GitSource::new(url, git_dir).with_reporter(Facade::from(reporter.clone()))
        } else {
            GitSource::new(url, git_dir)
        };
        let fetch = tokio::task::spawn_blocking(move || source.fetch())
            .await?
            .map_err(SourceDistError::GitErr)?;
        Ok((fetch, subdirectory))
    }

    /// Build a source distribution, storing the built wheel in the cache.
    async fn build_source_dist(
        &self,
        dist: &SourceDist,
        temp_dir: Option<TempDir>,
        source_dist: &Path,
        subdirectory: Option<&Path>,
    ) -> anyhow::Result<(WheelFilename, Metadata21)> {
        debug!("Building: {dist}");

        if self.build_context.no_build() {
            bail!("Building source distributions is disabled");
        }

        // Create a directory for the wheel.
        // TODO(konstin): Unzip the wheels and store them with the other unzipped wheels, after
        // adding index and compatibility tags to the cache location.
        let wheel_dir = self
            .build_context
            .cache()
            .join(BUILT_WHEELS_CACHE)
            .join(dist.distribution_id());
        fs::create_dir_all(&wheel_dir).await?;

        // Build the wheel.
        let disk_filename = self
            .build_context
            .build_source(source_dist, subdirectory, &wheel_dir, &dist.to_string())
            .await?;

        if let Some(temp_dir) = temp_dir {
            temp_dir.close()?;
        }

        // Read the metadata from the wheel.
        let filename = WheelFilename::from_str(&disk_filename)?;

        let mut archive = ZipArchive::new(fs_err::File::open(wheel_dir.join(disk_filename))?)?;
        let dist_info_dir =
            find_dist_info(&filename, archive.file_names().map(|name| (name, name)))?.1;
        let dist_info =
            std::io::read_to_string(archive.by_name(&format!("{dist_info_dir}/METADATA"))?)?;
        let metadata = Metadata21::parse(dist_info.as_bytes())?;

        debug!("Finished building: {dist}");
        Ok((filename, metadata))
    }

    /// Given a remote source distribution, return a precise variant, if possible.
    ///
    /// For example, given a Git dependency with a reference to a branch or tag, return a URL
    /// with a precise reference to the current commit of that branch or tag.
    ///
    /// This method takes into account various normalizations that are independent from the Git
    /// layer. For example: removing `#subdirectory=pkg_dir`-like fragments, and removing `git+`
    /// prefix kinds.
    async fn precise(&self, dist: &SourceDist) -> Result<Option<Url>, SourceDistError> {
        let SourceDist::Git(source_dist) = dist else {
            return Ok(None);
        };

        let DirectGitUrl { url, subdirectory } =
            DirectGitUrl::try_from(&source_dist.url).map_err(SourceDistError::GitErr)?;

        // If the commit already contains a complete SHA, short-circuit.
        if url.precise().is_some() {
            return Ok(None);
        }

        // Fetch the precise SHA of the Git reference (which could be a branch, a tag, a partial
        // commit, etc.).
        let git_dir = self.build_context.cache().join(GIT_CACHE);
        let source = if let Some(reporter) = &self.reporter {
            GitSource::new(url, git_dir).with_reporter(Facade::from(reporter.clone()))
        } else {
            GitSource::new(url, git_dir)
        };
        let precise = tokio::task::spawn_blocking(move || source.fetch())
            .await?
            .map_err(SourceDistError::GitErr)?;
        let url = precise.into_git();

        // Re-encode as a URL.
        Ok(Some(DirectGitUrl { url, subdirectory }.into()))
    }
}

trait SourceDistReporter: Send + Sync {
    /// Callback to invoke when a repository checkout begins.
    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize;

    /// Callback to invoke when a repository checkout completes.
    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize);
}

/// A facade for converting from [`Reporter`] to [`puffin_git::Reporter`].
struct Facade {
    reporter: Arc<dyn Reporter>,
}

impl From<Arc<dyn Reporter>> for Facade {
    fn from(reporter: Arc<dyn Reporter>) -> Self {
        Self { reporter }
    }
}

impl puffin_git::Reporter for Facade {
    fn on_checkout_start(&self, url: &Url, rev: &str) -> usize {
        self.reporter.on_checkout_start(url, rev)
    }

    fn on_checkout_complete(&self, url: &Url, rev: &str, index: usize) {
        self.reporter.on_checkout_complete(url, rev, index);
    }
}

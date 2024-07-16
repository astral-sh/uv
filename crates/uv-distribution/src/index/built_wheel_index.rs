use distribution_types::{
    DirectUrlSourceDist, DirectorySourceDist, GitSourceDist, Hashed, PathSourceDist,
};
use platform_tags::Tags;
use uv_cache::{ArchiveTimestamp, Cache, CacheBucket, CacheShard, WheelCache};
use uv_fs::symlinks;
use uv_types::HashStrategy;

use crate::index::cached_wheel::CachedWheel;
use crate::source::{HttpRevisionPointer, LocalRevisionPointer, HTTP_REVISION, LOCAL_REVISION};
use crate::Error;

/// A local index of built distributions for a specific source distribution.
#[derive(Debug)]
pub struct BuiltWheelIndex<'a> {
    cache: &'a Cache,
    tags: &'a Tags,
    hasher: &'a HashStrategy,
}

impl<'a> BuiltWheelIndex<'a> {
    /// Initialize an index of built distributions.
    pub fn new(cache: &'a Cache, tags: &'a Tags, hasher: &'a HashStrategy) -> Self {
        Self {
            cache,
            tags,
            hasher,
        }
    }

    /// Return the most compatible [`CachedWheel`] for a given source distribution at a direct URL.
    ///
    /// This method does not perform any freshness checks and assumes that the source distribution
    /// is already up-to-date.
    pub fn url(&self, source_dist: &DirectUrlSourceDist) -> Result<Option<CachedWheel>, Error> {
        // For direct URLs, cache directly under the hash of the URL itself.
        let cache_shard = self.cache.shard(
            CacheBucket::SourceDistributions,
            WheelCache::Url(source_dist.url.raw()).root(),
        );

        // Read the revision from the cache.
        let Some(pointer) = HttpRevisionPointer::read_from(cache_shard.entry(HTTP_REVISION))?
        else {
            return Ok(None);
        };

        // Enforce hash-checking by omitting any wheels that don't satisfy the required hashes.
        let revision = pointer.into_revision();
        if !revision.satisfies(self.hasher.get(source_dist)) {
            return Ok(None);
        }

        Ok(self.find(&cache_shard.shard(revision.id())))
    }

    /// Return the most compatible [`CachedWheel`] for a given source distribution at a local path.
    pub fn path(&self, source_dist: &PathSourceDist) -> Result<Option<CachedWheel>, Error> {
        let cache_shard = self.cache.shard(
            CacheBucket::SourceDistributions,
            WheelCache::Path(&source_dist.url).root(),
        );

        // Read the revision from the cache.
        let Some(pointer) = LocalRevisionPointer::read_from(cache_shard.entry(LOCAL_REVISION))?
        else {
            return Ok(None);
        };

        // Determine the last-modified time of the source distribution.
        let modified =
            ArchiveTimestamp::from_file(&source_dist.install_path).map_err(Error::CacheRead)?;

        // If the distribution is stale, omit it from the index.
        if !pointer.is_up_to_date(modified) {
            return Ok(None);
        }

        // Enforce hash-checking by omitting any wheels that don't satisfy the required hashes.
        let revision = pointer.into_revision();
        if !revision.satisfies(self.hasher.get(source_dist)) {
            return Ok(None);
        }

        Ok(self.find(&cache_shard.shard(revision.id())))
    }

    /// Return the most compatible [`CachedWheel`] for a given source distribution built from a
    /// local directory (source tree).
    pub fn directory(
        &self,
        source_dist: &DirectorySourceDist,
    ) -> Result<Option<CachedWheel>, Error> {
        let cache_shard = self.cache.shard(
            CacheBucket::SourceDistributions,
            if source_dist.editable {
                WheelCache::Editable(&source_dist.url).root()
            } else {
                WheelCache::Path(&source_dist.url).root()
            },
        );

        // Read the revision from the cache.
        let Some(pointer) = LocalRevisionPointer::read_from(cache_shard.entry(LOCAL_REVISION))?
        else {
            return Ok(None);
        };

        // Determine the last-modified time of the source distribution.
        let Some(modified) = ArchiveTimestamp::from_source_tree(&source_dist.install_path)
            .map_err(Error::CacheRead)?
        else {
            return Err(Error::DirWithoutEntrypoint(
                source_dist.install_path.clone(),
            ));
        };

        // If the distribution is stale, omit it from the index.
        if !pointer.is_up_to_date(modified) {
            return Ok(None);
        }

        // Enforce hash-checking by omitting any wheels that don't satisfy the required hashes.
        let revision = pointer.into_revision();
        if !revision.satisfies(self.hasher.get(source_dist)) {
            return Ok(None);
        }

        Ok(self.find(&cache_shard.shard(revision.id())))
    }

    /// Return the most compatible [`CachedWheel`] for a given source distribution at a git URL.
    pub fn git(&self, source_dist: &GitSourceDist) -> Option<CachedWheel> {
        // Enforce hash-checking, which isn't supported for Git distributions.
        if self.hasher.get(source_dist).is_validate() {
            return None;
        }

        let git_sha = source_dist.git.precise()?;

        let cache_shard = self.cache.shard(
            CacheBucket::SourceDistributions,
            WheelCache::Git(&source_dist.url, &git_sha.to_short_string()).root(),
        );

        self.find(&cache_shard)
    }

    /// Find the "best" distribution in the index for a given source distribution.
    ///
    /// This lookup prefers newer versions over older versions, and aims to maximize compatibility
    /// with the target platform.
    ///
    /// The `shard` should point to a directory containing the built distributions for a specific
    /// source distribution. For example, given the built wheel cache structure:
    /// ```text
    /// built-wheels-v0/
    /// └── pypi
    ///     └── django-allauth-0.51.0.tar.gz
    ///         ├── django_allauth-0.51.0-py3-none-any.whl
    ///         └── metadata.json
    /// ```
    ///
    /// The `shard` should be `built-wheels-v0/pypi/django-allauth-0.51.0.tar.gz`.
    fn find(&self, shard: &CacheShard) -> Option<CachedWheel> {
        let mut candidate: Option<CachedWheel> = None;

        // Unzipped wheels are stored as symlinks into the archive directory.
        for subdir in symlinks(shard) {
            match CachedWheel::from_built_source(&subdir) {
                None => {}
                Some(dist_info) => {
                    // Pick the wheel with the highest priority
                    let compatibility = dist_info.filename.compatibility(self.tags);

                    // Only consider wheels that are compatible with our tags.
                    if !compatibility.is_compatible() {
                        continue;
                    }

                    if let Some(existing) = candidate.as_ref() {
                        // Override if the wheel is newer, or "more" compatible.
                        if dist_info.filename.version > existing.filename.version
                            || compatibility > existing.filename.compatibility(self.tags)
                        {
                            candidate = Some(dist_info);
                        }
                    } else {
                        candidate = Some(dist_info);
                    }
                }
            }
        }

        candidate
    }
}

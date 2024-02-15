use distribution_types::{git_reference, DirectUrlSourceDist, GitSourceDist, Name, PathSourceDist};
use platform_tags::Tags;
use uv_cache::{ArchiveTimestamp, Cache, CacheBucket, CacheShard, WheelCache};
use uv_fs::symlinks;

use crate::index::cached_wheel::CachedWheel;
use crate::source::{read_http_manifest, read_timestamp_manifest, MANIFEST};
use crate::Error;

/// A local index of built distributions for a specific source distribution.
pub struct BuiltWheelIndex;

impl BuiltWheelIndex {
    /// Return the most compatible [`CachedWheel`] for a given source distribution at a direct URL.
    ///
    /// This method does not perform any freshness checks and assumes that the source distribution
    /// is already up-to-date.
    pub fn url(
        source_dist: &DirectUrlSourceDist,
        cache: &Cache,
        tags: &Tags,
    ) -> Result<Option<CachedWheel>, Error> {
        // For direct URLs, cache directly under the hash of the URL itself.
        let cache_shard = cache.shard(
            CacheBucket::BuiltWheels,
            WheelCache::Url(source_dist.url.raw()).remote_wheel_dir(source_dist.name().as_ref()),
        );

        // Read the manifest from the cache. There's no need to enforce freshness, since we
        // enforce freshness on the entries.
        let manifest_entry = cache_shard.entry(MANIFEST);
        let Some(manifest) = read_http_manifest(&manifest_entry)? else {
            return Ok(None);
        };

        Ok(Self::find(&cache_shard.shard(manifest.id()), tags))
    }

    /// Return the most compatible [`CachedWheel`] for a given source distribution at a local path.
    pub fn path(
        source_dist: &PathSourceDist,
        cache: &Cache,
        tags: &Tags,
    ) -> Result<Option<CachedWheel>, Error> {
        let cache_shard = cache.shard(
            CacheBucket::BuiltWheels,
            WheelCache::Path(&source_dist.url).remote_wheel_dir(source_dist.name().as_ref()),
        );

        // Determine the last-modified time of the source distribution.
        let Some(modified) = ArchiveTimestamp::from_path(&source_dist.path).expect("archived")
        else {
            return Err(Error::DirWithoutEntrypoint);
        };

        // Read the manifest from the cache. There's no need to enforce freshness, since we
        // enforce freshness on the entries.
        let manifest_entry = cache_shard.entry(MANIFEST);
        let Some(manifest) = read_timestamp_manifest(&manifest_entry, modified)? else {
            return Ok(None);
        };

        Ok(Self::find(&cache_shard.shard(manifest.id()), tags))
    }

    /// Return the most compatible [`CachedWheel`] for a given source distribution at a git URL.
    pub fn git(source_dist: &GitSourceDist, cache: &Cache, tags: &Tags) -> Option<CachedWheel> {
        let Ok(Some(git_sha)) = git_reference(&source_dist.url) else {
            return None;
        };

        let cache_shard = cache.shard(
            CacheBucket::BuiltWheels,
            WheelCache::Git(&source_dist.url, &git_sha.to_short_string())
                .remote_wheel_dir(source_dist.name().as_ref()),
        );

        Self::find(&cache_shard, tags)
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
    fn find(shard: &CacheShard, tags: &Tags) -> Option<CachedWheel> {
        let mut candidate: Option<CachedWheel> = None;

        // Unzipped wheels are stored as symlinks into the archive directory.
        for subdir in symlinks(shard) {
            match CachedWheel::from_path(&subdir) {
                None => {}
                Some(dist_info) => {
                    // Pick the wheel with the highest priority
                    let compatibility = dist_info.filename.compatibility(tags);

                    // Only consider wheels that are compatible with our tags.
                    if !compatibility.is_compatible() {
                        continue;
                    }

                    if let Some(existing) = candidate.as_ref() {
                        // Override if the wheel is newer, or "more" compatible.
                        if dist_info.filename.version > existing.filename.version
                            || compatibility > existing.filename.compatibility(tags)
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

//! Materialize the same cached source revision used by the builder, without executing it.

use std::borrow::Cow;
use std::path::PathBuf;

use uv_cache::{CacheBucket, CacheShard, WheelCache};
use uv_distribution_filename::SourceDistExtension;
use uv_distribution_types::{
    ArchiveHashPolicy, BuildableSource, GitPathSourceUrl, Hashed, IndexUrl, PathSourceUrl,
    SourceDist,
};
use uv_redacted::SafeUrl;
use uv_types::BuildContext;

use super::{SOURCE, SourceDistributionBuilder, fetch_git_source_tree};
use crate::distribution_database::{ManagedClient, RegistryArtifactRequest};
use crate::error::Error;
use crate::hash::ArtifactHashPolicy;

impl<T: BuildContext> SourceDistributionBuilder<'_, T> {
    pub(crate) async fn materialize(
        &self,
        dist: &SourceDist,
        hashes: ArchiveHashPolicy<'_>,
        client: &ManagedClient<'_>,
    ) -> Result<PathBuf, Error> {
        let source = BuildableSource::Dist(dist);
        let cache = self.build_context.cache();
        let bucket = cache.shard(CacheBucket::SourceDistributions, "");
        match dist {
            SourceDist::Directory(dist) => Ok(dist.install_path.to_path_buf()),
            SourceDist::Path(dist) => {
                self.materialize_archive(
                    &source,
                    &PathSourceUrl::from(dist),
                    &bucket.shard(WheelCache::Path(&dist.url).root()),
                    hashes,
                )
                .await
            }
            SourceDist::Registry(dist) => {
                let request = RegistryArtifactRequest::new(
                    &dist.index,
                    &dist.file.location,
                    client.index_locations(),
                )?;
                let shard = bucket.shard(
                    WheelCache::Index(request.index)
                        .wheel_dir(dist.name.as_ref())
                        .join(dist.version.to_string()),
                );
                if request.url.scheme() == "file" {
                    let path = request
                        .url
                        .to_file_path()
                        .map_err(|()| Error::NonFileUrl(request.url.clone()))?;
                    self.materialize_archive(
                        &source,
                        &PathSourceUrl {
                            url: &request.url,
                            path: Cow::Owned(path),
                            ext: dist.ext,
                        },
                        &shard,
                        hashes,
                    )
                    .await
                } else {
                    self.materialize_url(
                        &source,
                        &request.url,
                        Some(request.index),
                        dist.ext,
                        &shard,
                        ArtifactHashPolicy::for_registry(
                            hashes,
                            request.is_proxy,
                            dist.file.hashes.as_slice(),
                        ),
                        client,
                    )
                    .await
                }
            }
            SourceDist::DirectUrl(dist) => {
                let path = self
                    .materialize_url(
                        &source,
                        &dist.url,
                        None,
                        dist.ext,
                        &bucket.shard(WheelCache::Url(&dist.url).root()),
                        ArtifactHashPolicy::required(hashes),
                        client,
                    )
                    .await?;
                Ok(dist
                    .subdirectory
                    .as_ref()
                    .map_or(path.clone(), |subdir| path.join(subdir)))
            }
            SourceDist::GitDirectory(dist) => {
                let fetch = fetch_git_source_tree(
                    self.build_context.git(),
                    &dist.git,
                    dist.url.to_url(),
                    dist.subdirectory.as_deref(),
                    client.unmanaged.git_http_settings(dist.git.url()),
                    cache,
                    None,
                )
                .await?;
                Ok(dist.subdirectory.as_ref().map_or_else(
                    || fetch.path().to_path_buf(),
                    |subdir| fetch.path().join(subdir),
                ))
            }
            SourceDist::GitPath(dist) => {
                let resource = GitPathSourceUrl::from(dist);
                let fetch = self
                    .build_context
                    .git()
                    .fetch(
                        resource.git,
                        client.unmanaged.git_http_settings(resource.git.url()),
                        cache.bucket(CacheBucket::Git),
                        None,
                    )
                    .await?;
                let sha = fetch.git().precise().expect("Exact commit after checkout");
                let shard = bucket.shard(WheelCache::Git(resource.url, sha.as_short_str()).root());
                let _lock = shard.lock().await.map_err(Error::CacheLock)?;
                let revision = self
                    .git_archive_revision(&source, &resource, &fetch, &shard, hashes)
                    .await?;
                if !revision.satisfies(hashes) {
                    return Err(Error::hash_mismatch(
                        source.to_string(),
                        hashes.digests(),
                        revision.hashes(),
                    ));
                }
                Ok(shard.entry(SOURCE).into_path_buf())
            }
        }
    }

    async fn materialize_archive(
        &self,
        source: &BuildableSource<'_>,
        resource: &PathSourceUrl<'_>,
        shard: &CacheShard,
        hashes: ArchiveHashPolicy<'_>,
    ) -> Result<PathBuf, Error> {
        let _lock = shard.lock().await.map_err(Error::CacheLock)?;
        let revision = self
            .archive_revision(source, resource, shard, hashes)
            .await?
            .revision;
        let entry = shard.shard(revision.id()).entry(SOURCE);
        let revision = if entry.path().is_dir() {
            revision
        } else {
            self.heal_archive_revision(source, resource, &entry, revision, hashes)
                .await?
        };
        if !revision.satisfies(hashes) {
            return Err(Error::hash_mismatch(
                source.to_string(),
                hashes.digests(),
                revision.hashes(),
            ));
        }
        Ok(entry.into_path_buf())
    }

    async fn materialize_url(
        &self,
        source: &BuildableSource<'_>,
        url: &SafeUrl,
        index: Option<&IndexUrl>,
        ext: SourceDistExtension,
        shard: &CacheShard,
        hashes: ArtifactHashPolicy<'_>,
        client: &ManagedClient<'_>,
    ) -> Result<PathBuf, Error> {
        let _lock = shard.lock().await.map_err(Error::CacheLock)?;
        let revision = self
            .url_revision(source, ext, url, index, shard, hashes, client)
            .await?;
        let entry = shard.shard(revision.id()).entry(SOURCE);
        if !entry.path().is_dir() {
            self.heal_url_revision(source, ext, url, index, &entry, revision, hashes, client)
                .await?;
        }
        Ok(entry.into_path_buf())
    }
}

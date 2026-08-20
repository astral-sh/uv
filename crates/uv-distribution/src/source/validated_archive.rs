use std::io::ErrorKind;
use std::path::Path;

use futures::TryStreamExt;
use reqwest::Response;
use tempfile::TempDir;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{debug, info_span, warn};

use uv_cache::{Cache, CacheBucket};
use uv_distribution_filename::SourceDistExtension;
use uv_distribution_types::{BuildableSource, RemoteSource, SourceDist};
use uv_extract::hash::Hasher;
use uv_fs::rename_with_retry;
use uv_pypi_types::{HashAlgorithm, HashDigest};

use crate::error::Error;

/// An extracted source archive that satisfies the checks requested during extraction.
///
/// The staging directory and fields are private to this module. Callers can only obtain this type
/// through extraction, so only a validated archive can be persisted. Persisting consumes the value
/// and returns the metadata computed during extraction.
pub(super) struct ValidatedSourceArchive {
    staging_dir: TempDir,
    metadata: SourceArchiveMetadata,
    origin: ArchiveOrigin,
}

/// Metadata computed while consuming a source archive.
pub(super) struct SourceArchiveMetadata {
    pub(super) hashes: Vec<HashDigest>,
    pub(super) size: u64,
}

/// Preserve the existing persistence behavior for HTTP and local archives.
enum ArchiveOrigin {
    Http,
    Local,
}

impl ValidatedSourceArchive {
    /// Download and extract a source distribution, computing its hashes and checking its size.
    pub(super) async fn extract_http(
        response: Response,
        source: &BuildableSource<'_>,
        ext: SourceDistExtension,
        cache: &Cache,
        algorithms: &[HashAlgorithm],
    ) -> Result<Self, Error> {
        let temp_dir = tempfile::tempdir_in(cache.bucket(CacheBucket::SourceDistributions))
            .map_err(Error::CacheWrite)?;

        let reader = response
            .bytes_stream()
            .map_err(std::io::Error::other)
            .into_async_read();

        // Create a hasher for each hash algorithm.
        let mut hashers = algorithms
            .iter()
            .copied()
            .map(Hasher::from)
            .collect::<Vec<_>>();
        let mut hasher = uv_extract::hash::HashReader::new(reader.compat(), &mut hashers);

        // Download and unzip the source distribution into a temporary directory.
        let span = info_span!("download_source_dist", source_dist = %source);
        uv_extract::stream::archive(&mut hasher, ext, temp_dir.path())
            .await
            .map_err(|err| Error::Extract(source.to_string(), err))?;
        drop(span);

        let expected_size = match source {
            BuildableSource::Dist(SourceDist::Registry(dist)) if dist.size_is_authoritative => {
                dist.size()
            }
            BuildableSource::Dist(SourceDist::DirectUrl(dist)) => dist.size(),
            _ => None,
        };

        // If necessary, exhaust the reader to compute the hash or validate the archive size.
        if !algorithms.is_empty() || expected_size.is_some() {
            hasher.finish().await.map_err(Error::HashExhaustion)?;
        }
        if let Some(expected) = expected_size
            && hasher.bytes_read() != expected
        {
            return Err(Error::MismatchedSize {
                distribution: source.to_string(),
                expected,
                actual: hasher.bytes_read(),
            });
        }

        let size = hasher.bytes_read();
        let hashes = hashers.into_iter().map(HashDigest::from).collect();

        Ok(Self {
            staging_dir: temp_dir,
            metadata: SourceArchiveMetadata { hashes, size },
            origin: ArchiveOrigin::Http,
        })
    }

    /// Extract a local source archive and compute its requested hashes.
    pub(super) async fn extract_local(
        path: &Path,
        ext: SourceDistExtension,
        cache: &Cache,
        algorithms: &[HashAlgorithm],
    ) -> Result<Self, Error> {
        debug!("Unpacking for build: {}", path.display());

        let temp_dir = tempfile::tempdir_in(cache.bucket(CacheBucket::SourceDistributions))
            .map_err(Error::CacheWrite)?;
        let reader = fs_err::tokio::File::open(&path)
            .await
            .map_err(Error::CacheRead)?;

        // Create a hasher for each hash algorithm.
        let mut hashers = algorithms
            .iter()
            .copied()
            .map(Hasher::from)
            .collect::<Vec<_>>();
        let mut hasher = uv_extract::hash::HashReader::new(reader, &mut hashers);

        // Unzip the archive into a temporary directory.
        uv_extract::stream::archive(&mut hasher, ext, &temp_dir.path())
            .await
            .map_err(|err| Error::Extract(temp_dir.path().to_string_lossy().into_owned(), err))?;

        // If necessary, exhaust the reader to compute the hash.
        if !algorithms.is_empty() {
            hasher.finish().await.map_err(Error::HashExhaustion)?;
        }

        let size = hasher.bytes_read();
        let hashes = hashers.into_iter().map(HashDigest::from).collect();

        Ok(Self {
            staging_dir: temp_dir,
            metadata: SourceArchiveMetadata { hashes, size },
            origin: ArchiveOrigin::Local,
        })
    }

    /// Persist the validated source tree and return metadata computed during extraction.
    pub(super) async fn persist(self, target: &Path) -> Result<SourceArchiveMetadata, Error> {
        let Self {
            staging_dir: temp_dir,
            metadata,
            origin,
        } = self;
        let existing_directory = match origin {
            ArchiveOrigin::Http => ErrorKind::AlreadyExists,
            ArchiveOrigin::Local => ErrorKind::DirectoryNotEmpty,
        };

        // Extract the top-level directory.
        let extracted = match uv_extract::strip_component(temp_dir.path()) {
            Ok(top_level) => top_level,
            Err(uv_extract::Error::NonSingularArchive(_)) => match origin {
                ArchiveOrigin::Http => temp_dir.keep(),
                ArchiveOrigin::Local => temp_dir.path().to_path_buf(),
            },
            Err(err) => {
                return Err(Error::Extract(
                    temp_dir.path().to_string_lossy().into_owned(),
                    err,
                ));
            }
        };

        // Persist it to the cache.
        fs_err::tokio::create_dir_all(target.parent().expect("Cache entry to have parent"))
            .await
            .map_err(Error::CacheWrite)?;
        if let Err(err) = rename_with_retry(extracted, target).await {
            // If the directory already exists, accept it.
            if err.kind() == existing_directory {
                warn!("Directory already exists: {}", target.display());
            } else {
                return Err(Error::CacheWrite(err));
            }
        }

        Ok(metadata)
    }
}

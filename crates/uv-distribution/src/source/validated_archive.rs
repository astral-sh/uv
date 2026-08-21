use std::io::ErrorKind;
use std::path::Path;

use futures::TryStreamExt;
use reqwest::Response;
use tempfile::TempDir;
use tokio::io::AsyncRead;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{Span, debug, info_span, warn};

use uv_cache::{Cache, CacheBucket};
use uv_distribution_filename::SourceDistExtension;
use uv_distribution_types::{BuildableSource, RemoteSource, SourceDist};
use uv_extract::hash::{HashReader, Hasher};
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

/// Controls cache-rename collisions and cleanup of non-singular archives.
///
/// HTTP archives accept [`ErrorKind::AlreadyExists`] and use [`TempDir::keep`] when the archive
/// contains multiple top-level entries. Local archives accept [`ErrorKind::DirectoryNotEmpty`] and
/// retain automatic temporary-directory cleanup.
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

        let expected_size = match source {
            BuildableSource::Dist(SourceDist::Registry(dist)) if dist.size_is_authoritative => {
                dist.size()
            }
            BuildableSource::Dist(SourceDist::DirectUrl(dist)) => dist.size(),
            _ => None,
        };
        Self::extract(
            reader.compat(),
            ext,
            temp_dir,
            algorithms,
            ArchiveOrigin::Http,
            expected_size,
            source.to_string(),
            || Some(info_span!("download_source_dist", source_dist = %source)),
        )
        .await
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
        let error_context = temp_dir.path().to_string_lossy().into_owned();
        Self::extract(
            reader,
            ext,
            temp_dir,
            algorithms,
            ArchiveOrigin::Local,
            None,
            error_context,
            || None,
        )
        .await
    }

    /// Extract into private staging and complete the requested checks before constructing a value.
    async fn extract(
        reader: impl AsyncRead + Unpin,
        ext: SourceDistExtension,
        staging_dir: TempDir,
        algorithms: &[HashAlgorithm],
        origin: ArchiveOrigin,
        expected_size: Option<u64>,
        error_context: String,
        make_span: impl FnOnce() -> Option<Span>,
    ) -> Result<Self, Error> {
        // Create a hasher for each hash algorithm.
        let mut hashers = algorithms
            .iter()
            .copied()
            .map(Hasher::from)
            .collect::<Vec<_>>();
        let mut hasher = HashReader::new(reader, &mut hashers);

        let span = make_span();
        if let Err(err) = uv_extract::stream::archive(&mut hasher, ext, staging_dir.path()).await {
            return Err(Error::Extract(error_context, err));
        }
        drop(span);

        // If necessary, exhaust the reader to compute hashes or validate the archive size.
        if !algorithms.is_empty() || expected_size.is_some() {
            hasher.finish().await.map_err(Error::HashExhaustion)?;
        }
        if let Some(expected) = expected_size
            && hasher.bytes_read() != expected
        {
            return Err(Error::MismatchedSize {
                distribution: error_context,
                expected,
                actual: hasher.bytes_read(),
            });
        }

        let size = hasher.bytes_read();
        let hashes = hashers.into_iter().map(HashDigest::from).collect();

        Ok(Self {
            staging_dir,
            metadata: SourceArchiveMetadata { hashes, size },
            origin,
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use anyhow::Result;

    use super::*;

    async fn extract(
        bytes: &[u8],
        trailing_read: &Cell<bool>,
        algorithms: &[HashAlgorithm],
        expected_size: Option<u64>,
    ) -> Result<ValidatedSourceArchive, Error> {
        let reader = futures::stream::iter([
            Ok(bytes),
            Err(std::io::Error::other("unexpected trailing read")),
        ])
        .inspect_err(|_| trailing_read.set(true))
        .into_async_read()
        .compat();
        let staging_dir = tempfile::tempdir().map_err(Error::CacheWrite)?;
        ValidatedSourceArchive::extract(
            reader,
            SourceDistExtension::TarGz,
            staging_dir,
            algorithms,
            ArchiveOrigin::Http,
            expected_size,
            "source.tar.gz".to_owned(),
            || None,
        )
        .await
    }

    #[tokio::test]
    async fn reader_is_exhausted_only_for_hashes_or_size() -> Result<()> {
        let _preview = uv_preview::test::with_features(&[]);
        let bytes = fs_err::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../test/links/basic_package-0.1.0.tar.gz"),
        )?;
        let trailing_read = Cell::new(false);
        extract(&bytes, &trailing_read, &[], None).await?;
        assert!(!trailing_read.get());

        for (algorithms, size) in [
            (&[HashAlgorithm::Sha256][..], None),
            (&[][..], Some(bytes.len() as u64)),
        ] {
            assert!(matches!(
                extract(&bytes, &trailing_read, algorithms, size).await,
                Err(Error::HashExhaustion(_))
            ));
            assert!(trailing_read.replace(false));
        }
        Ok(())
    }

    #[tokio::test]
    async fn parser_error_precedes_trailing_read() {
        let _preview = uv_preview::test::with_features(&[]);
        let trailing_read = Cell::new(false);
        assert!(matches!(
            extract(
                &[0xff; 512],
                &trailing_read,
                &[HashAlgorithm::Sha256],
                Some(1)
            )
            .await,
            Err(Error::Extract(_, _))
        ));
        assert!(!trailing_read.get());
    }
}

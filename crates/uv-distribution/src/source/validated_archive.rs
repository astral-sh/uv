use std::io::ErrorKind;
use std::path::Path;

use tempfile::TempDir;
use tokio::io::AsyncRead;
use tracing::warn;

use uv_cache::{Cache, CacheBucket};
use uv_distribution_filename::SourceDistExtension;
use uv_distribution_types::{BuildableSource, HashPolicy};
use uv_extract::hash::{HashReader, Hasher};
use uv_fs::rename_with_retry;
use uv_pypi_types::{HashAlgorithm, HashDigest};

use crate::error::Error;

/// The checks required before an extracted source archive can be persisted to the cache.
pub(super) struct ArchiveValidation<'a> {
    /// Additional hashes to generate beyond those required for validation.
    pub(super) extra_algorithms: &'a [HashAlgorithm],
    /// The caller's trusted hash policy.
    pub(super) hash_policy: HashPolicy<'a>,
    /// Every digest from a cache revision being repaired must remain unchanged.
    pub(super) existing_hashes: &'a [HashDigest],
    pub(super) expected_size: Option<u64>,
}

/// An extracted source archive that satisfies its requested size and hash checks.
///
/// The staging directory and fields are private to this module. Callers can only obtain this type
/// through [`Self::extract`], so only a validated archive can be persisted. Persisting consumes the
/// value, while dropping it removes the staged files.
pub(super) struct ValidatedSourceArchive {
    staging_dir: TempDir,
    metadata: SourceArchiveMetadata,
}

/// Metadata computed while consuming a source archive.
pub(super) struct SourceArchiveMetadata {
    pub(super) hashes: Vec<HashDigest>,
    pub(super) size: u64,
}

impl ValidatedSourceArchive {
    /// Extract into a fresh temporary directory and validate the complete archive before returning.
    pub(super) async fn extract(
        reader: impl AsyncRead + Unpin,
        source: &BuildableSource<'_>,
        ext: SourceDistExtension,
        cache: &Cache,
        validation: ArchiveValidation<'_>,
    ) -> Result<Self, Error> {
        let staging_dir = tempfile::tempdir_in(cache.bucket(CacheBucket::SourceDistributions))
            .map_err(Error::CacheWrite)?;

        // Include every algorithm needed to validate the caller's policy or repair an old revision.
        let mut algorithms = validation.hash_policy.algorithms();
        algorithms.extend_from_slice(validation.extra_algorithms);
        algorithms.extend(validation.existing_hashes.iter().map(HashDigest::algorithm));
        algorithms.sort();
        algorithms.dedup();
        let mut hashers = algorithms
            .iter()
            .copied()
            .map(Hasher::from)
            .collect::<Vec<_>>();
        let mut hasher = HashReader::new(reader, &mut hashers);

        let (staging_dir, _) = uv_extract::stream::archive(&mut hasher, ext, staging_dir)
            .await
            .map_err(|err| Error::Extract(source.to_string(), err))?;

        if !algorithms.is_empty() || validation.expected_size.is_some() {
            hasher.finish().await.map_err(Error::HashExhaustion)?;
        }
        let size = hasher.bytes_read();
        if let Some(expected) = validation.expected_size
            && size != expected
        {
            return Err(Error::MismatchedSize {
                distribution: source.to_string(),
                expected,
                actual: size,
            });
        }

        let hashes = hashers
            .into_iter()
            .map(HashDigest::from)
            .collect::<Vec<_>>();
        if validation.hash_policy.requires_validation() && !validation.hash_policy.matches(&hashes)
        {
            return Err(Error::hash_mismatch(
                source.to_string(),
                validation.hash_policy.digests(),
                &hashes,
            ));
        }
        for existing in validation.existing_hashes {
            if !hashes.contains(existing) {
                return Err(Error::CacheHeal(source.to_string(), existing.algorithm()));
            }
        }
        Ok(Self {
            staging_dir,
            metadata: SourceArchiveMetadata { hashes, size },
        })
    }

    /// Persist the validated source tree and return metadata computed during extraction.
    pub(super) async fn persist(self, target: &Path) -> Result<SourceArchiveMetadata, Error> {
        let extracted = match uv_extract::strip_component(self.staging_dir.path()) {
            Ok(top_level) => top_level,
            Err(uv_extract::Error::NonSingularArchive(_)) => self.staging_dir.path().to_path_buf(),
            Err(err) => {
                return Err(Error::Extract(
                    self.staging_dir.path().to_string_lossy().into_owned(),
                    err,
                ));
            }
        };

        fs_err::tokio::create_dir_all(target.parent().expect("Cache entry to have parent"))
            .await
            .map_err(Error::CacheWrite)?;
        if let Err(err) = rename_with_retry(extracted, target).await {
            // Another task may have persisted the same revision. Accept an existing directory,
            // accounting for the different errors returned by Unix and Windows.
            if matches!(
                err.kind(),
                ErrorKind::AlreadyExists | ErrorKind::DirectoryNotEmpty
            ) && fs_err::tokio::symlink_metadata(target)
                .await
                .is_ok_and(|metadata| metadata.is_dir())
            {
                warn!("Directory already exists: {}", target.display());
            } else {
                return Err(Error::CacheWrite(err));
            }
        }

        Ok(self.metadata)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::slice;

    use anyhow::Result;
    use futures::TryStreamExt;
    use tokio_util::compat::FuturesAsyncReadCompatExt;

    use uv_distribution_types::{DirectSourceUrl, HashGeneration, SourceUrl};
    use uv_redacted::DisplaySafeUrl;

    use super::*;

    const NO_VALIDATION: ArchiveValidation<'static> = ArchiveValidation {
        extra_algorithms: &[],
        hash_policy: HashPolicy::None,
        existing_hashes: &[],
        expected_size: None,
    };

    fn cache() -> Result<Cache> {
        let cache = Cache::temp()?;
        fs_err::create_dir_all(cache.bucket(CacheBucket::SourceDistributions))?;
        Ok(cache)
    }

    fn reader<'a>(bytes: &'a [u8], trailing_read: &'a Cell<bool>) -> impl AsyncRead + Unpin + 'a {
        futures::stream::iter([
            Ok(bytes),
            Err(std::io::Error::other("unexpected trailing read")),
        ])
        .inspect_err(|_| trailing_read.set(true))
        .into_async_read()
        .compat()
    }

    async fn extract(
        cache: &Cache,
        reader: impl AsyncRead + Unpin,
        validation: ArchiveValidation<'_>,
    ) -> Result<ValidatedSourceArchive, Error> {
        let url =
            DisplaySafeUrl::parse("https://example.com/source.tar.gz").expect("valid test URL");
        let source = BuildableSource::Url(SourceUrl::Direct(DirectSourceUrl {
            url: &url,
            subdirectory: None,
            ext: SourceDistExtension::TarGz,
        }));
        ValidatedSourceArchive::extract(
            reader,
            &source,
            SourceDistExtension::TarGz,
            cache,
            validation,
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
        let cache = cache()?;
        let trailing_read = Cell::new(false);
        extract(&cache, reader(&bytes, &trailing_read), NO_VALIDATION).await?;
        assert!(!trailing_read.get());

        let digest = HashDigest::from(Hasher::from(HashAlgorithm::Sha256));
        for validation in [
            ArchiveValidation {
                extra_algorithms: &[HashAlgorithm::Sha256],
                ..NO_VALIDATION
            },
            ArchiveValidation {
                hash_policy: HashPolicy::Generate(HashGeneration::All),
                ..NO_VALIDATION
            },
            ArchiveValidation {
                existing_hashes: slice::from_ref(&digest),
                ..NO_VALIDATION
            },
            ArchiveValidation {
                expected_size: Some(bytes.len() as u64),
                ..NO_VALIDATION
            },
        ] {
            assert!(matches!(
                extract(&cache, reader(&bytes, &trailing_read), validation).await,
                Err(Error::HashExhaustion(_))
            ));
            assert!(trailing_read.replace(false));
        }
        Ok(())
    }

    #[tokio::test]
    async fn parser_error_precedes_trailing_read() -> Result<()> {
        let _preview = uv_preview::test::with_features(&[]);
        let cache = cache()?;
        let trailing_read = Cell::new(false);
        assert!(matches!(
            extract(
                &cache,
                reader(&[0xff; 512], &trailing_read),
                ArchiveValidation {
                    extra_algorithms: &[HashAlgorithm::Sha256],
                    expected_size: Some(1),
                    ..NO_VALIDATION
                },
            )
            .await,
            Err(Error::Extract(_, _))
        ));
        assert!(!trailing_read.get());
        Ok(())
    }

    #[tokio::test]
    async fn staging_directory_is_removed_on_drop_or_failure() -> Result<()> {
        let _preview = uv_preview::test::with_features(&[]);
        let bytes = fs_err::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../test/links/basic_package-0.1.0.tar.gz"),
        )?;
        let cache = cache()?;
        let wrong_hash = HashDigest::from(Hasher::from(HashAlgorithm::Sha256));
        let result = extract(
            &cache,
            &bytes[..],
            ArchiveValidation {
                hash_policy: HashPolicy::All(&[wrong_hash]),
                ..NO_VALIDATION
            },
        )
        .await;
        assert!(matches!(result, Err(Error::MismatchedHashes { .. })));
        assert!(
            fs_err::read_dir(cache.bucket(CacheBucket::SourceDistributions))?
                .next()
                .is_none()
        );

        let existing_hash = HashDigest {
            algorithm: HashAlgorithm::Sha512,
            digest: "f754f5955ce76c8fbdccdacd6e0e34977354b04d062d7f993fa84f3301309257fd225c85ebc99571b8b8ad711b37c407af65c5eae73599802ea3b4d3082d2f32".into(),
        };
        let archive = extract(
            &cache,
            &bytes[..],
            ArchiveValidation {
                existing_hashes: slice::from_ref(&existing_hash),
                ..NO_VALIDATION
            },
        )
        .await?;
        assert_eq!(archive.metadata.hashes, [existing_hash]);
        assert_eq!(archive.metadata.size, bytes.len() as u64);
        let staging_dir = archive.staging_dir.path().to_path_buf();
        assert!(staging_dir.is_dir());
        drop(archive);
        assert!(!staging_dir.exists());

        let archive = extract(&cache, &bytes[..], NO_VALIDATION).await?;
        let staging_dir = archive.staging_dir.path().to_path_buf();
        // A file in place of the parent directory makes cache creation fail on every platform.
        let parent = cache.root().join("not-a-directory");
        fs_err::write(&parent, b"existing")?;
        let target = parent.join("persisted");
        assert!(matches!(
            archive.persist(&target).await,
            Err(Error::CacheWrite(_))
        ));
        assert!(!staging_dir.exists());
        assert_eq!(fs_err::read(parent)?, b"existing");
        Ok(())
    }
}

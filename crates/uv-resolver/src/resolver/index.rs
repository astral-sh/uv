use std::hash::BuildHasherDefault;
use std::sync::Arc;

use rustc_hash::FxHasher;
use uv_distribution_types::{BuiltDist, Dist, DistributionId, Identifier, IndexUrl, SourceDist};
use uv_normalize::PackageName;
use uv_once_map::{RegisteredEntry, RegisteredOnceMap};
use uv_pypi_types::{ParsedUrl, VerbatimParsedUrl};
use uv_resolver_types::{DistributionMetadataIndex, MetadataResponse};

use crate::ResolveError;
use crate::resolver::provider::VersionsResponse;

/// In-memory index of package metadata.
#[derive(Default, Clone)]
pub struct InMemoryIndex(Arc<SharedInMemoryIndex>);

#[derive(Default)]
struct SharedInMemoryIndex {
    /// A map from package name to the metadata for that package and the index where the metadata
    /// came from.
    implicit: FxOnceMap<PackageName, Arc<VersionsResponse>>,

    explicit: FxOnceMap<(PackageName, IndexUrl), Arc<VersionsResponse>>,

    /// A map from a concrete distribution to its metadata.
    distributions: DistributionMetadataIndex,

    /// Concrete artifacts for URL requirements whose filenames required discovery.
    resolved_urls: FxOnceMap<DistributionId, Result<Arc<Dist>, Arc<ResolveError>>>,
}

pub(crate) type FxOnceMap<K, V> = RegisteredOnceMap<K, V, BuildHasherDefault<FxHasher>>;
pub(crate) type FxRegisteredEntry<'a, K, V> =
    RegisteredEntry<'a, K, V, BuildHasherDefault<FxHasher>>;

impl InMemoryIndex {
    /// Returns a reference to the package metadata map.
    pub(crate) fn implicit(&self) -> &FxOnceMap<PackageName, Arc<VersionsResponse>> {
        &self.0.implicit
    }

    /// Returns a reference to the package metadata map.
    pub(crate) fn explicit(&self) -> &FxOnceMap<(PackageName, IndexUrl), Arc<VersionsResponse>> {
        &self.0.explicit
    }

    /// Returns a reference to the distribution metadata map.
    pub fn distributions(&self) -> &DistributionMetadataIndex {
        &self.0.distributions
    }

    /// Publish metadata before waking solvers that need the corresponding URL artifact.
    pub fn insert_url(&self, dist: &Dist, metadata: Arc<MetadataResponse>) {
        self.0.distributions.done(dist.distribution_id(), metadata);
        if header_filename(dist).is_some() {
            self.0
                .resolved_urls
                .done(dist.distribution_id(), Ok(Arc::new(dist.clone())));
        }
    }

    /// Complete failed discovery so a waiting solver can return the original error.
    pub(crate) fn fail_url(&self, url: &VerbatimParsedUrl, error: Arc<ResolveError>) {
        self.0
            .resolved_urls
            .done(url.verbatim.distribution_id(), Err(error));
    }

    /// Whether URL-keyed metadata belongs to this discovered artifact.
    pub fn is_url_metadata_valid(&self, dist: &Dist) -> bool {
        header_filename(dist).is_none()
            || self
                .0
                .resolved_urls
                .get(&dist.distribution_id())
                .is_some_and(|resolved| resolved.is_ok_and(|resolved| resolved.as_ref() == dist))
    }

    /// Register filename discovery even when metadata was supplied by a lockfile.
    pub(crate) fn register_url(&self, url: &VerbatimParsedUrl) -> bool {
        if let ParsedUrl::Archive(archive) = &url.parsed_url
            && archive.archive.extension().is_none()
        {
            return self
                .0
                .resolved_urls
                .register(url.verbatim.distribution_id());
        }
        false
    }

    /// Construct a distribution using a filename discovered while retrieving its metadata.
    pub(crate) fn resolved_url(
        &self,
        name: PackageName,
        mut url: VerbatimParsedUrl,
    ) -> Result<Dist, ResolveError> {
        if let ParsedUrl::Archive(archive) = &mut url.parsed_url
            && archive.archive.extension().is_none()
            && let Some(registered) = self
                .0
                .resolved_urls
                .get_registered(url.verbatim.distribution_id())
        {
            let dist = registered
                .wait_blocking()
                .map_err(ResolveError::UrlResolution)?;
            let parsed = match dist.as_ref() {
                Dist::Built(BuiltDist::DirectUrl(dist)) => Some(dist.to_parsed_url()),
                Dist::Source(SourceDist::DirectUrl(dist)) => Some(dist.to_parsed_url()),
                Dist::Built(
                    BuiltDist::Registry(_) | BuiltDist::Path(_) | BuiltDist::GitPath(_),
                )
                | Dist::Source(
                    SourceDist::Registry(_)
                    | SourceDist::Path(_)
                    | SourceDist::Directory(_)
                    | SourceDist::GitDirectory(_)
                    | SourceDist::GitPath(_),
                ) => None,
            };
            if let Some(ParsedUrl::Archive(resolved)) = parsed {
                archive.archive = resolved.archive;
            }
        }
        Ok(Dist::from_url(name, url)?)
    }

    /// Return exclusive access to distribution metadata when no cloned index can use it.
    pub fn distributions_mut(&mut self) -> Option<&mut DistributionMetadataIndex> {
        Arc::get_mut(&mut self.0).map(|index| &mut index.distributions)
    }
}

/// Return the filename used to discover a remote archive's distribution type.
fn header_filename(dist: &Dist) -> Option<&str> {
    match dist {
        Dist::Built(BuiltDist::DirectUrl(dist)) => dist.header_filename.as_deref(),
        Dist::Source(SourceDist::DirectUrl(dist)) => dist.header_filename.as_deref(),
        Dist::Built(BuiltDist::Registry(_) | BuiltDist::Path(_) | BuiltDist::GitPath(_))
        | Dist::Source(
            SourceDist::Registry(_)
            | SourceDist::Path(_)
            | SourceDist::Directory(_)
            | SourceDist::GitDirectory(_)
            | SourceDist::GitPath(_),
        ) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::Duration;

    use uv_distribution::ArchiveMetadata;
    use uv_distribution_types::{Dist, Identifier};
    use uv_pep508::VerbatimUrl;
    use uv_pypi_types::{
        ArchiveUrlFilename, ParsedArchiveUrl, ParsedUrl, ResolutionMetadata, VerbatimParsedUrl,
    };
    use uv_resolver_types::{MetadataResponse, MetadataUnavailable};

    use crate::{InMemoryIndex, ResolveError};

    /// Cached metadata must not leave the solver waiting when filename discovery fails.
    #[test]
    fn filename_discovery_failure_with_preloaded_metadata() -> Result<(), Box<dyn Error>> {
        let index = InMemoryIndex::default();
        let url = VerbatimUrl::from_url("https://example.com/download".parse()?);
        let url = VerbatimParsedUrl {
            parsed_url: ParsedUrl::Archive(ParsedArchiveUrl {
                url: url.to_url(),
                subdirectory: None,
                archive: ArchiveUrlFilename::Unknown,
            }),
            verbatim: url,
        };
        index.distributions().done(
            url.verbatim.distribution_id(),
            Arc::new(MetadataResponse::Unavailable(MetadataUnavailable::Offline)),
        );
        assert!(index.register_url(&url));

        let (sender, receiver) = mpsc::channel();
        let shared = index.clone();
        let requested = url.clone();
        let name = "example".parse()?;
        let worker = thread::spawn(move || {
            let result = shared.resolved_url(name, requested);
            let _ = sender.send(result);
        });

        let error = Arc::new(ResolveError::Distribution(uv_distribution::Error::NoBuild));
        index.fail_url(&url, error.clone());
        let result = receiver.recv_timeout(Duration::from_secs(5))?;
        let Err(ResolveError::UrlResolution(actual)) = result else {
            return Err("expected the filename discovery error".into());
        };
        assert!(Arc::ptr_eq(&error, &actual));
        worker.join().map_err(|_| "discovery worker failed")?;
        Ok(())
    }

    /// A newly discovered wheel must not be paired with metadata for an older response.
    #[test]
    fn filename_discovery_replaces_preloaded_metadata() -> Result<(), Box<dyn Error>> {
        let index = InMemoryIndex::default();
        let url = VerbatimUrl::from_url("https://example.com/download".parse()?);
        let url = VerbatimParsedUrl {
            parsed_url: ParsedUrl::Archive(ParsedArchiveUrl {
                url: url.to_url(),
                subdirectory: None,
                archive: ArchiveUrlFilename::Unknown,
            }),
            verbatim: url,
        };
        let original = ResolutionMetadata::parse_metadata(b"Name: example\nVersion: 1.0\n")?;
        index.distributions().done(
            url.verbatim.distribution_id(),
            Arc::new(MetadataResponse::Found(ArchiveMetadata::from_metadata23(
                original,
            ))),
        );
        assert!(index.register_url(&url));

        let mut resolved = url.clone();
        let ParsedUrl::Archive(archive) = &mut resolved.parsed_url else {
            return Err("expected an archive URL".into());
        };
        archive.archive = ArchiveUrlFilename::from_header("example-2.0-py3-none-any.whl")?;
        let dist = Dist::from_url("example".parse()?, resolved)?;
        assert!(!index.is_url_metadata_valid(&dist));

        let (sender, receiver) = mpsc::channel();
        let shared = index.clone();
        let requested = url.clone();
        let name = "example".parse()?;
        let worker = thread::spawn(move || {
            let result = shared.resolved_url(name, requested.clone());
            let metadata = shared
                .distributions()
                .get(&requested.verbatim.distribution_id());
            let _ = sender.send((result, metadata));
        });

        let refreshed = ResolutionMetadata::parse_metadata(b"Name: example\nVersion: 2.0\n")?;
        index.insert_url(
            &dist,
            Arc::new(MetadataResponse::Found(ArchiveMetadata::from_metadata23(
                refreshed,
            ))),
        );
        let (actual, metadata) = receiver.recv_timeout(Duration::from_secs(5))?;
        assert_eq!(actual?, dist);
        let Some(metadata) = metadata else {
            return Err("expected metadata for the discovered wheel".into());
        };
        let MetadataResponse::Found(archive) = metadata.as_ref() else {
            return Err("expected available metadata".into());
        };
        assert_eq!(archive.metadata.version, "2.0".parse()?);
        assert!(index.is_url_metadata_valid(&dist));
        worker.join().map_err(|_| "discovery worker failed")?;
        Ok(())
    }
}

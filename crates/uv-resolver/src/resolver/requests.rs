use std::sync::Arc;

use tokio::sync::mpsc::Sender;

use uv_distribution::ArchiveMetadata;
use uv_distribution_types::{
    BuiltDist, DerivationChain, Dist, DistErrorKind, DistributionId, Identifier, IndexMetadata,
    IndexUrl, Name, RegistryVariantsJson, RequestedDist, ResolutionRecorder, SelectedVariant,
    SupportedVariantProperties,
};
use uv_normalize::PackageName;
use uv_once_map::Registration;
use uv_pep440::Version;

use crate::pubgrub::Range;
use crate::resolved::ResolvedDistRef;
use crate::resolver::index::{FxRegisteredEntry, ParsedVariantsJson, ParsedVariantsJsonResponse};
use crate::resolver::{InMemoryIndex, MetadataResponse, Request, VersionsResponse};
use crate::{PythonRequirement, ResolveError};

/// A blocking solver-side handle for requesting metadata from the asynchronous fetcher.
#[derive(Clone)]
pub(crate) struct MetadataRequests {
    index: InMemoryIndex,
    sender: Sender<Request>,
    recorder: Option<ResolutionRecorder>,
}

/// A distribution request whose cache identity is derived from the requested distribution.
pub(crate) enum MetadataRequest<'a> {
    Dist(Dist),
    Resolved(ResolvedDistRef<'a>),
}

impl MetadataRequest<'_> {
    fn id(&self) -> DistributionId {
        match self {
            Self::Dist(dist) => dist.distribution_id(),
            Self::Resolved(dist) => dist.distribution_id(),
        }
    }

    fn into_request(self) -> Request {
        match self {
            Self::Dist(dist) => Request::Dist(dist),
            Self::Resolved(dist) => Request::from(dist),
        }
    }
}

impl Name for MetadataRequest<'_> {
    fn name(&self) -> &PackageName {
        match self {
            Self::Dist(dist) => dist.name(),
            Self::Resolved(dist) => dist.name(),
        }
    }
}

/// A registered version-list request, bound to its package and index scope.
pub(crate) enum PendingVersions<'index> {
    Implicit(FxRegisteredEntry<'index, PackageName, Arc<VersionsResponse>>),
    Explicit(FxRegisteredEntry<'index, (PackageName, IndexUrl), Arc<VersionsResponse>>),
}

impl PendingVersions<'_> {
    pub(crate) fn wait(self) -> Arc<VersionsResponse> {
        match self {
            Self::Implicit(entry) => entry.wait_blocking(),
            Self::Explicit(entry) => entry.wait_blocking(),
        }
    }
}

/// A distribution whose metadata was registered or supplied before resolution.
///
/// Selected pins retain a registered entry that prevents cache removal while borrowed.
#[derive(Clone, Debug)]
pub(crate) struct RegisteredMetadata<'index>(
    FxRegisteredEntry<'index, DistributionId, Arc<MetadataResponse>>,
);

impl RegisteredMetadata<'_> {
    pub(crate) fn id(&self) -> &DistributionId {
        self.0.key()
    }

    pub(crate) fn wait(&self) -> Arc<MetadataResponse> {
        self.0.wait_blocking()
    }
}

/// Index-level variant metadata registered for a selected package version.
#[derive(Clone, Debug)]
pub(crate) struct RegisteredVariantsJson<'index>(
    FxRegisteredEntry<'index, RegistryVariantsJson, Arc<ParsedVariantsJsonResponse>>,
);

impl RegisteredVariantsJson<'_> {
    pub(crate) fn wait(&self) -> Arc<ParsedVariantsJsonResponse> {
        self.0.wait_blocking()
    }
}

impl MetadataRequests {
    pub(crate) fn new(
        index: InMemoryIndex,
        sender: Sender<Request>,
        recorder: Option<ResolutionRecorder>,
    ) -> Self {
        Self {
            index,
            sender,
            recorder,
        }
    }

    /// Schedule a package version request without retaining a handle.
    pub(crate) fn enqueue_package(
        &self,
        name: &PackageName,
        index: Option<&IndexMetadata>,
    ) -> Result<(), ResolveError> {
        if let Some(recorder) = &self.recorder {
            recorder.exclude_newer(name);
        }
        let registered = if let Some(index) = index {
            self.index
                .explicit()
                .register((name.clone(), index.url().clone()))
        } else {
            self.index.implicit().register(name.clone())
        };
        if registered {
            self.sender
                .blocking_send(Request::Package(name.clone(), index.cloned()))?;
        }
        Ok(())
    }

    /// Request the versions of a package once for the selected index scope.
    pub(crate) fn request_package(
        &self,
        name: &PackageName,
        index: Option<&IndexMetadata>,
    ) -> Result<PendingVersions<'_>, ResolveError> {
        if let Some(recorder) = &self.recorder {
            recorder.exclude_newer(name);
        }
        if let Some(index) = index {
            let entry = match self
                .index
                .explicit()
                .register_entry((name.clone(), index.url().clone()))
            {
                Registration::New(entry) => {
                    self.sender
                        .blocking_send(Request::Package(name.clone(), Some(index.clone())))?;
                    entry
                }
                Registration::Existing(entry) => entry,
            };
            Ok(PendingVersions::Explicit(entry))
        } else {
            let entry = match self.index.implicit().register_entry(name.clone()) {
                Registration::New(entry) => {
                    self.sender
                        .blocking_send(Request::Package(name.clone(), None))?;
                    entry
                }
                Registration::Existing(entry) => entry,
            };
            Ok(PendingVersions::Implicit(entry))
        }
    }

    /// Schedule metadata retrieval without retaining or cloning its cache identity.
    pub(crate) fn enqueue_metadata(
        &self,
        request: MetadataRequest<'_>,
    ) -> Result<(), ResolveError> {
        if let Some(recorder) = &self.recorder {
            recorder.dependency_metadata(request.name());
        }
        if self.index.distributions().register(request.id()) {
            self.sender.blocking_send(request.into_request())?;
        }
        Ok(())
    }

    /// Request distribution metadata once, validating and constructing only new requests.
    ///
    /// Metadata already fetched or scheduled by another path does not need a new request.
    pub(crate) fn request_metadata(
        &self,
        request: MetadataRequest<'_>,
        validate: impl FnOnce(&MetadataRequest<'_>) -> Result<(), ResolveError>,
    ) -> Result<RegisteredMetadata<'_>, ResolveError> {
        if let Some(recorder) = &self.recorder {
            recorder.dependency_metadata(request.name());
        }
        let entry = match self.index.distributions().register_entry(request.id()) {
            Registration::New(entry) => {
                validate(&request)?;
                self.sender.blocking_send(request.into_request())?;
                entry
            }
            Registration::Existing(entry) => entry,
        };
        Ok(RegisteredMetadata(entry))
    }

    /// Schedule variant metadata retrieval once for the selected package version.
    pub(crate) fn request_variants_json(
        &self,
        variants_json: &RegistryVariantsJson,
    ) -> Result<RegisteredVariantsJson<'_>, ResolveError> {
        let entry = match self
            .index
            .variants_json()
            .register_entry(variants_json.clone())
        {
            Registration::New(entry) => {
                self.sender
                    .blocking_send(Request::VariantsJson(variants_json.clone()))?;
                entry
            }
            Registration::Existing(entry) => entry,
        };
        Ok(RegisteredVariantsJson(entry))
    }

    /// Request target-specific properties for verified metadata at most once.
    pub(crate) fn request_variant_properties(
        &self,
        parsed: Arc<ParsedVariantsJson>,
    ) -> Result<
        FxRegisteredEntry<'_, RegistryVariantsJson, Arc<SupportedVariantProperties>>,
        ResolveError,
    > {
        let entry = match self
            .index
            .variant_properties()
            .register_entry(parsed.artifact.clone())
        {
            Registration::New(entry) => {
                self.sender
                    .blocking_send(Request::VariantProperties(parsed))?;
                entry
            }
            Registration::Existing(entry) => entry,
        };
        Ok(entry)
    }

    /// Select a direct wheel's variant once, reusing lookahead results when available.
    pub(crate) fn select_direct_variant(
        &self,
        dist: &BuiltDist,
    ) -> Result<Option<SelectedVariant>, ResolveError> {
        let entry = match self
            .index
            .direct_variants()
            .register_entry(dist.distribution_id())
        {
            Registration::New(entry) => {
                self.sender
                    .blocking_send(Request::DirectVariant(dist.clone()))?;
                entry
            }
            Registration::Existing(entry) => entry,
        };
        entry.wait_blocking().map_err(|error| {
            let requested = RequestedDist::Installable(Dist::Built(dist.clone()));
            ResolveError::Dist(
                DistErrorKind::from_requested_dist(&requested, error.as_ref()),
                Box::new(requested),
                DerivationChain::default(),
                error,
            )
        })
    }

    /// Request dependency metadata for the wheels declared in verified variant metadata.
    pub(crate) fn request_variant_dependencies(
        &self,
        dist: Arc<Dist>,
        variants: Arc<ParsedVariantsJson>,
        baseline: &ArchiveMetadata,
    ) -> Result<FxRegisteredEntry<'_, Arc<Dist>, Arc<MetadataResponse>>, ResolveError> {
        let entry = match self
            .index
            .variant_dependencies()
            .register_entry(Arc::clone(&dist))
        {
            Registration::New(entry) => {
                self.sender.blocking_send(Request::VariantDependencies {
                    dist,
                    variants,
                    baseline: Box::new(baseline.clone()),
                })?;
                entry
            }
            Registration::Existing(entry) => entry,
        };
        Ok(entry)
    }

    /// Schedule speculative candidate selection using an already-requested package version map.
    pub(crate) fn prefetch(
        &self,
        name: &PackageName,
        range: &Range<Version>,
        python_requirement: &PythonRequirement,
    ) -> Result<(), ResolveError> {
        self.sender.blocking_send(Request::Prefetch(
            name.clone(),
            range.clone(),
            python_requirement.clone(),
        ))?;
        Ok(())
    }

    /// Acquire metadata registered during input preparation or package visitation.
    pub(crate) fn metadata(&self, dist: &Dist) -> Result<RegisteredMetadata<'_>, ResolveError> {
        if let Some(recorder) = &self.recorder {
            recorder.dependency_metadata(dist.name());
        }
        self.index
            .distributions()
            .get_registered(dist.distribution_id())
            .map(RegisteredMetadata)
            .ok_or_else(|| ResolveError::UnregisteredTask(dist.to_string()))
    }
}

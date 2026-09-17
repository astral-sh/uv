use std::fmt;
use std::sync::Arc;

use tokio::sync::mpsc::Sender;

use uv_distribution_types::{
    Dist, DistributionId, Identifier, IndexMetadata, Name, ResolvedDistRef,
};
use uv_normalize::PackageName;
use uv_once_map::{RegisteredEntry, Registration};
use uv_pep440::Version;

use crate::pubgrub::Range;
use crate::resolver::{InMemoryIndex, MetadataResponse, Request, VersionsResponse};
use crate::{PythonRequirement, ResolveError};

/// A blocking solver-side handle for requesting metadata from the asynchronous fetcher.
#[derive(Clone)]
pub(crate) struct MetadataRequests {
    index: InMemoryIndex,
    sender: Sender<Request>,
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
pub(crate) struct PendingVersions(RegisteredEntry<Arc<VersionsResponse>>);

impl PendingVersions {
    pub(crate) fn wait(self) -> Arc<VersionsResponse> {
        self.0.wait_blocking()
    }
}

/// A registered distribution request, including metadata supplied before resolution.
///
/// Selected pins retain the result slot for repeated dependency queries without map lookups.
#[derive(Clone)]
pub(crate) struct RegisteredMetadata {
    entry: RegisteredEntry<Arc<MetadataResponse>>,
    id: DistributionId,
}

impl RegisteredMetadata {
    pub(crate) fn id(&self) -> &DistributionId {
        &self.id
    }

    pub(crate) fn wait(&self) -> Arc<MetadataResponse> {
        self.entry.wait_blocking()
    }
}

impl fmt::Debug for RegisteredMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("RegisteredMetadata")
            .field(&self.id)
            .finish()
    }
}

impl MetadataRequests {
    pub(crate) fn new(index: InMemoryIndex, sender: Sender<Request>) -> Self {
        Self { index, sender }
    }

    /// Request the versions of a package once for the selected index scope.
    pub(crate) fn request_package(
        &self,
        name: &PackageName,
        index: Option<&IndexMetadata>,
    ) -> Result<PendingVersions, ResolveError> {
        let registration = if let Some(index) = index {
            self.index
                .explicit()
                .register_entry((name.clone(), index.url().clone()))
        } else {
            self.index.implicit().register_entry(name.clone())
        };
        let entry = match registration {
            Registration::New(entry) => {
                self.sender
                    .blocking_send(Request::Package(name.clone(), index.cloned()))?;
                entry
            }
            Registration::Existing(entry) => entry,
        };
        Ok(PendingVersions(entry))
    }

    /// Request distribution metadata once, validating and constructing only new requests.
    ///
    /// Metadata already fetched or scheduled by another path does not need a new request.
    pub(crate) fn request_metadata(
        &self,
        request: MetadataRequest<'_>,
        validate: impl FnOnce(&MetadataRequest<'_>) -> Result<(), ResolveError>,
    ) -> Result<RegisteredMetadata, ResolveError> {
        let id = request.id();
        let entry = match self.index.distributions().register_entry(id.clone()) {
            Registration::New(entry) => {
                validate(&request)?;
                self.sender.blocking_send(request.into_request())?;
                entry
            }
            Registration::Existing(entry) => entry,
        };
        Ok(RegisteredMetadata { entry, id })
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

    /// Recover a handle for metadata registered by this solver or by input preparation.
    pub(crate) fn metadata(
        &self,
        id: DistributionId,
        description: impl FnOnce() -> String,
    ) -> Result<RegisteredMetadata, ResolveError> {
        let entry = self
            .index
            .distributions()
            .entry(&id)
            .ok_or_else(|| ResolveError::UnregisteredTask(description()))?;
        Ok(RegisteredMetadata { entry, id })
    }
}

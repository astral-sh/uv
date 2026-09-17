use std::fmt;
use std::sync::Arc;

use tokio::sync::mpsc::Sender;

use uv_distribution_types::{
    Dist, DistributionId, Identifier, IndexMetadata, IndexUrl, Name, ResolvedDistRef,
};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_resolver_types::DistributionMetadataIndex;

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
pub(crate) struct PendingVersions<'a> {
    index: &'a InMemoryIndex,
    name: &'a PackageName,
    scope: Option<&'a IndexUrl>,
}

impl PendingVersions<'_> {
    pub(crate) fn wait(self) -> Result<Arc<VersionsResponse>, ResolveError> {
        if let Some(scope) = self.scope {
            self.index
                .explicit()
                .wait_blocking(&(self.name.clone(), scope.clone()))
                .map_err(|_| ResolveError::UnregisteredTask(self.name.to_string()))
        } else {
            self.index
                .implicit()
                .wait_blocking(self.name)
                .map_err(|_| ResolveError::UnregisteredTask(self.name.to_string()))
        }
    }
}

/// A distribution whose metadata was registered or supplied before resolution.
///
/// Selected pins retain the cache and identity for repeated dependency queries. Borrowing the
/// cache avoids reference-count updates when pins are cloned for a fork.
#[derive(Clone)]
pub(crate) struct RegisteredMetadata<'index> {
    index: &'index DistributionMetadataIndex,
    id: DistributionId,
}

impl RegisteredMetadata<'_> {
    pub(crate) fn id(&self) -> &DistributionId {
        &self.id
    }

    pub(crate) fn wait(&self) -> Result<Arc<MetadataResponse>, ResolveError> {
        self.index
            .wait_blocking(&self.id)
            .map_err(|_| ResolveError::UnregisteredTask(format!("{:?}", self.id)))
    }
}

impl fmt::Debug for RegisteredMetadata<'_> {
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

    /// Schedule a package version request without retaining a handle.
    pub(crate) fn enqueue_package(
        &self,
        name: &PackageName,
        index: Option<&IndexMetadata>,
    ) -> Result<(), ResolveError> {
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
    pub(crate) fn request_package<'a>(
        &'a self,
        name: &'a PackageName,
        index: Option<&'a IndexMetadata>,
    ) -> Result<PendingVersions<'a>, ResolveError> {
        self.enqueue_package(name, index)?;
        Ok(PendingVersions {
            index: &self.index,
            name,
            scope: index.map(IndexMetadata::url),
        })
    }

    /// Schedule metadata retrieval without retaining or cloning its cache identity.
    pub(crate) fn enqueue_metadata(
        &self,
        request: MetadataRequest<'_>,
    ) -> Result<(), ResolveError> {
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
        let id = request.id();
        if self.index.distributions().register(id.clone()) {
            validate(&request)?;
            self.sender.blocking_send(request.into_request())?;
        }
        Ok(RegisteredMetadata {
            index: self.index.distributions(),
            id,
        })
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

    /// Read metadata registered during input preparation or package visitation, retaining its
    /// registration for subsequent dependency queries.
    pub(crate) fn wait_for_metadata(
        &self,
        dist: &Dist,
    ) -> Result<(RegisteredMetadata<'_>, Arc<MetadataResponse>), ResolveError> {
        let id = dist.distribution_id();
        let response = self
            .index
            .distributions()
            .wait_blocking(&id)
            .map_err(|_| ResolveError::UnregisteredTask(dist.to_string()))?;
        Ok((
            RegisteredMetadata {
                index: self.index.distributions(),
                id,
            },
            response,
        ))
    }
}

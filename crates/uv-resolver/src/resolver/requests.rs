use std::sync::Arc;

use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;

use uv_distribution_types::{Dist, DistributionId, Identifier, IndexMetadata};
use uv_git_types::GitUrl;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_types::HashStrategy;

use crate::pubgrub::Range;
use crate::resolver::index::DirectHashKey;
use crate::resolver::{InMemoryIndex, MetadataResponse, Request, VersionsResponse};
use crate::{PythonRequirement, ResolveError};

/// A blocking solver-side handle for requesting metadata from the asynchronous fetcher.
#[derive(Clone)]
pub(crate) struct MetadataRequests {
    index: InMemoryIndex,
    sender: Sender<Request>,
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
    ) -> Result<(), ResolveError> {
        let registered = if let Some(index) = index {
            self.index
                .explicit()
                .register((name.clone(), index.clone()))
        } else {
            self.index.implicit().register(name.clone())
        };
        if registered {
            self.sender
                .blocking_send(Request::Package(name.clone(), index.cloned()))?;
        }
        Ok(())
    }

    /// Request distribution metadata once, validating and constructing only new requests.
    ///
    /// Metadata already fetched or scheduled by another path does not need a new request.
    pub(crate) fn request_metadata(
        &self,
        id: DistributionId,
        request: impl FnOnce() -> Result<Request, ResolveError>,
    ) -> Result<(), ResolveError> {
        if self.index.distributions().register(id) {
            self.sender.blocking_send(request()?)?;
        }
        Ok(())
    }

    /// Request a direct resource once for the current hash policy. Source trees without required
    /// validation share the metadata that project commands prepare and invalidate.
    pub(crate) fn request_direct(
        &self,
        dist: Dist,
        hasher: &HashStrategy,
    ) -> Result<(), ResolveError> {
        let id = dist.distribution_id();
        let hashes = DirectHashKey::new(&dist, hasher);
        let registered = if hashes.uses_project_cache(&dist) {
            self.index.distributions().register(id)
        } else {
            self.index.direct().register((id, hashes))
        };
        if registered {
            self.sender
                .blocking_send(Request::Dist(dist, Some(hasher.clone())))?;
        }
        Ok(())
    }

    /// Request a Git reference comparison independently of direct package metadata and builds.
    pub(crate) fn request_git_reference(
        &self,
        git: GitUrl,
    ) -> Result<oneshot::Receiver<()>, ResolveError> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .blocking_send(Request::GitReference(Box::new(git), sender))?;
        Ok(receiver)
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

    /// Wait for the versions of a package previously requested in the selected index scope.
    pub(crate) fn wait_for_versions(
        &self,
        name: &PackageName,
        index: Option<&IndexMetadata>,
    ) -> Result<Arc<VersionsResponse>, ResolveError> {
        if let Some(index) = index {
            self.index
                .explicit()
                .wait_blocking(&(name.clone(), index.clone()))
                .map_err(|_| ResolveError::UnregisteredTask(name.to_string()))
        } else {
            self.index
                .implicit()
                .wait_blocking(name)
                .map_err(|_| ResolveError::UnregisteredTask(name.to_string()))
        }
    }

    /// Wait for previously requested distribution metadata, naming an unregistered task on error.
    pub(crate) fn wait_for_metadata(
        &self,
        id: &DistributionId,
        description: impl FnOnce() -> String,
    ) -> Result<Arc<MetadataResponse>, ResolveError> {
        self.index
            .distributions()
            .wait_blocking(id)
            .map_err(|_| ResolveError::UnregisteredTask(description()))
    }

    pub(crate) fn wait_for_direct(
        &self,
        dist: &Dist,
        hasher: &HashStrategy,
    ) -> Result<Arc<MetadataResponse>, ResolveError> {
        let id = dist.distribution_id();
        let hashes = DirectHashKey::new(dist, hasher);
        if hashes.uses_project_cache(dist) {
            self.wait_for_metadata(&id, || dist.to_string())
        } else {
            self.index
                .direct()
                .wait_blocking(&(id, hashes))
                .map_err(|_| ResolveError::UnregisteredTask(dist.to_string()))
        }
    }
}

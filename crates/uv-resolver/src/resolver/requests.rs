use std::sync::Arc;

use tokio::sync::mpsc::Sender;

use uv_distribution_types::{
    DistributionId, GlobalVersionId, IndexMetadata, IndexUrl, RegistryVariantsJson,
};
use uv_normalize::PackageName;
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

    /// Request variant properties once for a package version from the selected index.
    pub(crate) fn request_variants(
        &self,
        id: &GlobalVersionId,
        variants_json: &RegistryVariantsJson,
    ) -> Result<(), ResolveError> {
        if self.index.variant_priorities().register(id.clone()) {
            self.sender
                .blocking_send(Request::Variants(id.clone(), variants_json.clone()))?;
        }
        Ok(())
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
        index: Option<&IndexUrl>,
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
}

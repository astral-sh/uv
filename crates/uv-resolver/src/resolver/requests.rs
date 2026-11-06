use std::sync::Arc;

use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;

use uv_distribution_types::{
    BuiltDist, Dist, DistributionId, Identifier, IndexMetadata, IndexUrl, Name, ResolutionRecorder,
};
use uv_normalize::PackageName;
use uv_once_map::{Completion, RegisteredEntry};
use uv_pep440::Version;

use crate::prioritized_distribution::RegistryVariantsJson;
use crate::pubgrub::Range;
use crate::resolved::ResolvedDistRef;
use crate::resolver::index::ParsedVariantsJsonResponse;
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

    fn into_request(self, completion: Completion<Arc<MetadataResponse>>) -> Request {
        match self {
            Self::Dist(dist) => Request::Dist(dist, completion),
            Self::Resolved(dist) => Request::from_resolved(&dist, completion),
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
    Implicit(RegisteredEntry<'index, PackageName, Arc<VersionsResponse>>),
    Explicit(RegisteredEntry<'index, (PackageName, IndexUrl), Arc<VersionsResponse>>),
}

impl PendingVersions<'_> {
    pub(crate) fn wait(self) -> Result<Arc<VersionsResponse>, ResolveError> {
        match self {
            Self::Implicit(entry) => entry.wait_blocking(),
            Self::Explicit(entry) => entry.wait_blocking(),
        }
        .ok_or(ResolveError::ChannelClosed)
    }
}

/// A distribution whose metadata was registered or supplied before resolution.
///
/// Selected pins retain a registered entry that prevents cache removal while borrowed.
#[derive(Clone, Debug)]
pub(crate) struct RegisteredMetadata<'index>(
    RegisteredEntry<'index, DistributionId, Arc<MetadataResponse>>,
);

impl RegisteredMetadata<'_> {
    pub(crate) fn id(&self) -> &DistributionId {
        self.0.key()
    }

    pub(crate) fn wait(&self) -> Result<Arc<MetadataResponse>, ResolveError> {
        self.0.wait_blocking().ok_or(ResolveError::ChannelClosed)
    }
}

/// Index-level variant metadata registered for a selected package version.
#[derive(Clone, Debug)]
pub(crate) struct RegisteredVariantsJson<'index>(
    RegisteredEntry<'index, RegistryVariantsJson, Arc<ParsedVariantsJsonResponse>>,
);

impl RegisteredVariantsJson<'_> {
    pub(crate) fn wait(&self) -> Result<Arc<ParsedVariantsJsonResponse>, ResolveError> {
        self.0.wait_blocking().ok_or(ResolveError::ChannelClosed)
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
        let completion = if let Some(index) = index {
            self.index
                .explicit()
                .register((name.clone(), index.url().clone()))
        } else {
            self.index.implicit().register(name.clone())
        };
        if let Some(completion) = completion {
            self.sender.blocking_send(Request::Package(
                name.clone(),
                index.cloned(),
                completion,
            ))?;
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
        let (entry, completion) = if let Some(index) = index {
            let (entry, completion) = self
                .index
                .explicit()
                .register_entry((name.clone(), index.url().clone()));
            (PendingVersions::Explicit(entry), completion)
        } else {
            let (entry, completion) = self.index.implicit().register_entry(name.clone());
            (PendingVersions::Implicit(entry), completion)
        };
        if let Some(completion) = completion {
            self.sender.blocking_send(Request::Package(
                name.clone(),
                index.cloned(),
                completion,
            ))?;
        }
        Ok(entry)
    }

    /// Schedule metadata retrieval without retaining or cloning its cache identity.
    pub(crate) fn enqueue_metadata(
        &self,
        request: MetadataRequest<'_>,
    ) -> Result<(), ResolveError> {
        if let Some(recorder) = &self.recorder {
            recorder.dependency_metadata(request.name());
        }
        if let Some(completion) = self.index.distributions().register(request.id()) {
            self.sender
                .blocking_send(request.into_request(completion))?;
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
        let (entry, completion) = self.index.distributions().register_entry(request.id());
        if let Some(completion) = completion {
            validate(&request)?;
            self.sender
                .blocking_send(request.into_request(completion))?;
        }
        Ok(RegisteredMetadata(entry))
    }

    /// Schedule variant metadata retrieval once for the selected package version.
    pub(crate) fn request_variants_json(
        &self,
        variants_json: &RegistryVariantsJson,
    ) -> Result<RegisteredVariantsJson<'_>, ResolveError> {
        let (entry, completion) = self
            .index
            .variants_json()
            .register_entry(variants_json.clone());
        if let Some(completion) = completion {
            self.sender
                .blocking_send(Request::VariantsJson(variants_json.clone(), completion))?;
        }
        Ok(RegisteredVariantsJson(entry))
    }

    /// Read embedded variant metadata for a wheel being considered by the solver.
    pub(crate) fn read_wheel_variant(
        &self,
        wheel: BuiltDist,
    ) -> Result<Option<ParsedVariantsJsonResponse>, ResolveError> {
        let (send, recv) = oneshot::channel();
        self.sender.blocking_send(Request::WheelVariant {
            wheel,
            response: send,
        })?;
        recv.blocking_recv()
            .map_err(|_| ResolveError::ChannelClosed)
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

#[cfg(test)]
mod tests {
    use std::error::Error;

    use tokio::sync::mpsc;
    use uv_pep508::Pep508Url;
    use uv_pypi_types::VerbatimParsedUrl;
    use uv_resolver_types::MetadataUnavailable;

    use super::*;

    /// A validation failure must release the claim so a later request for the same distribution
    /// runs validation and reaches the fetcher.
    #[test]
    fn retry_after_validation_error() -> Result<(), Box<dyn Error>> {
        let (sender, mut receiver) = mpsc::channel(1);
        let requests = MetadataRequests::new(InMemoryIndex::default(), sender, None);
        let name: PackageName = "demo".parse()?;
        let dist = Dist::from_url(
            name.clone(),
            VerbatimParsedUrl::parse_url("https://example.org/demo-1.0-py3-none-any.whl", None)?,
        )?;
        let error = requests
            .request_metadata(MetadataRequest::Dist(dist.clone()), |_| {
                Err(ResolveError::UnhashedPackage(name.clone()))
            })
            .expect_err("validation failed");
        assert!(matches!(error, ResolveError::UnhashedPackage(package) if package == name));
        let entry = requests.request_metadata(MetadataRequest::Dist(dist), |_| Ok(()))?;
        let Request::Dist(_, completion) = receiver.try_recv()? else {
            return Err("expected a distribution metadata request".into());
        };
        completion.finish(Arc::new(MetadataResponse::Unavailable(
            MetadataUnavailable::Offline,
        )));
        assert!(matches!(
            &*entry.wait()?,
            MetadataResponse::Unavailable(MetadataUnavailable::Offline)
        ));
        Ok(())
    }

    /// Dropping the fetch queue abandons both a queued package request and a subsequent failed send.
    #[test]
    fn closed_fetch_queue() -> Result<(), Box<dyn Error>> {
        let (sender, receiver) = mpsc::channel(1);
        let requests = MetadataRequests::new(InMemoryIndex::default(), sender, None);
        let name: PackageName = "demo".parse()?;
        let pending = requests.request_package(&name, None)?;
        drop(receiver);
        assert!(matches!(
            requests.request_package(&name, None),
            Err(ResolveError::ChannelClosed)
        ));
        assert!(matches!(pending.wait(), Err(ResolveError::ChannelClosed)));
        Ok(())
    }
}

use std::fmt::Write;
use std::sync::Arc;

use tracing::{Level, trace};

use uv_distribution_types::{Dist, DistributionId, Identifier, IndexUrl, ResolvedDist};
use uv_git::GitResolver;
use uv_normalize::PackageName;
use uv_pep440::{MIN_VERSION, Version};
use uv_pep508::MarkerTree;
use uv_pypi_types::VerbatimParsedUrl;
use uv_resolver_types::PackageNodeKind;

use crate::pins::FilePins;
use crate::redirect::url_to_precise;
use crate::universal_marker::ConflictMarker;
use crate::{InMemoryIndex, MetadataResponse, ResolveError, ResolverEnvironment, UniversalMarker};

/// The resolution from a single fork including the virtual packages and the edges between them.
#[derive(Debug)]
pub(crate) struct Resolution<'index> {
    pub(crate) nodes: Vec<(ResolutionPackage, Version)>,
    /// The directed connections between the nodes, where the marker is the edge weight. We don't
    /// store the requirement itself, but it can be retrieved from the package metadata.
    pub(crate) edges: Vec<ResolutionDependencyEdge>,
    /// Pinned artifacts and metadata for the selected package versions.
    pub(crate) pins: FilePins<'index>,
    /// The environment setting this resolution was found under.
    pub(crate) env: ResolverEnvironment,
}

/// A selected package version, retaining both its installation artifact and metadata provenance.
#[derive(Debug)]
pub(crate) struct SelectedDistribution {
    version: Version,
    source: SelectedSource,
}

#[derive(Debug)]
enum SelectedSource {
    Url {
        dist: ResolvedDist,
        metadata_id: DistributionId,
        metadata: Arc<MetadataResponse>,
    },
    Registry {
        dist: ResolvedDist,
        metadata_id: DistributionId,
        /// Direct-only resolution need not fetch registry metadata.
        metadata: Option<Arc<MetadataResponse>>,
    },
}

impl SelectedDistribution {
    /// Recover the selected artifact and its metadata before releasing a completed fork's pins.
    pub(crate) fn new(
        package: &ResolutionPackage,
        version: Version,
        pins: &FilePins<'_>,
        index: &InMemoryIndex,
        git: &GitResolver,
    ) -> Result<Self, ResolveError> {
        let source = if let Some(url) = &package.url {
            let metadata_id = Dist::from_url(package.name.clone(), url.clone())?.distribution_id();
            let response = index.distributions().get(&metadata_id).ok_or_else(|| {
                ResolveError::UnregisteredTask(format!("{} @ {}", package.name, url.verbatim))
            })?;
            let MetadataResponse::Found(_) = response.as_ref() else {
                return Err(ResolveError::PackageUnavailable(package.name.clone()));
            };
            SelectedSource::Url {
                dist: ResolvedDist::Installable {
                    dist: Arc::new(Dist::from_url(
                        package.name.clone(),
                        url_to_precise(url.clone(), git),
                    )?),
                    version: Some(version.clone()),
                },
                metadata_id,
                metadata: response,
            }
        } else {
            let (dist, metadata_id) =
                pins.dist_and_id(&package.name, &version).ok_or_else(|| {
                    ResolveError::UnregisteredTask(format!("{}=={version}", package.name))
                })?;
            let metadata =
                index.distributions().get(metadata_id).and_then(|response| {
                    match response.as_ref() {
                        MetadataResponse::Found(_) => Some(response),
                        MetadataResponse::Unavailable(_) | MetadataResponse::Error(..) => None,
                    }
                });
            SelectedSource::Registry {
                dist: dist.clone(),
                metadata_id: metadata_id.clone(),
                metadata,
            }
        };
        Ok(Self { version, source })
    }

    pub(crate) fn version(&self) -> &Version {
        &self.version
    }

    /// Move the selected artifact and metadata into the output graph.
    pub(crate) fn into_parts(self) -> (Version, ResolvedDist, Option<Arc<MetadataResponse>>) {
        match self.source {
            SelectedSource::Url { dist, metadata, .. } => (self.version, dist, Some(metadata)),
            SelectedSource::Registry { dist, metadata, .. } => (self.version, dist, metadata),
        }
    }

    /// The metadata cache uses the requested URL, even when the installation URL is made precise.
    fn metadata_id(&self) -> &DistributionId {
        match &self.source {
            SelectedSource::Url { metadata_id, .. }
            | SelectedSource::Registry { metadata_id, .. } => metadata_id,
        }
    }

    /// Registry hashes belong to the installation artifact; URL hashes use the requested URL.
    pub(crate) fn hashes_id(&self) -> DistributionId {
        match &self.source {
            SelectedSource::Url { .. } => self.metadata_id().clone(),
            SelectedSource::Registry { dist, .. } => dist.distribution_id(),
        }
    }
}

impl Resolution<'_> {
    /// When trace level logging is enabled, we dump the final
    /// set of resolutions, including markers, to help with
    /// debugging. Namely, this tells use precisely the state
    /// emitted by the resolver before going off to construct a
    /// resolution graph.
    pub(crate) fn trace_resolution(&self) {
        if !tracing::enabled!(Level::TRACE) {
            return;
        }
        trace!("Resolution: {:?}", self.env);
        for edge in &self.edges {
            let from = edge.from.map(|index| &self.nodes[index.0]);
            let (to_package, to_version) = &self.nodes[edge.to.0];
            trace!(
                "Resolution edge: {} -> {}",
                from.map(|(package, _)| package.name.as_str())
                    .unwrap_or("ROOT"),
                to_package.name,
            );
            // The unwraps below are OK because `write`ing to
            // a String can never fail (except for OOM).
            let mut msg = String::new();
            write!(
                msg,
                "{}",
                from.map_or(&*MIN_VERSION, |(_, version)| version)
            )
            .unwrap();
            if let Some(extra) = from.and_then(|(package, _)| package.kind.extra()) {
                write!(msg, " (extra: {extra})").unwrap();
            }
            if let Some(dev) = from.and_then(|(package, _)| package.kind.group()) {
                write!(msg, " (group: {dev})").unwrap();
            }
            write!(msg, " -> ").unwrap();
            write!(msg, "{to_version}").unwrap();
            if let Some(extra) = to_package.kind.extra() {
                write!(msg, " (extra: {extra})").unwrap();
            }
            if let Some(dev) = to_package.kind.group() {
                write!(msg, " (group: {dev})").unwrap();
            }
            if let Some(marker) = edge.marker.contents() {
                write!(msg, " ; {marker}").unwrap();
            }
            trace!("Resolution edge:     {msg}");
        }
    }
}

/// Package representation we used during resolution where each extra and also the dev-dependencies
/// group are their own package.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResolutionPackage {
    pub(crate) name: PackageName,
    pub(crate) kind: PackageNodeKind,
    /// For registry packages, this is `None`; otherwise, the direct URL of the distribution.
    pub(crate) url: Option<VerbatimParsedUrl>,
    /// For URL packages, this is `None`; otherwise, the index URL of the distribution.
    pub(crate) index: Option<IndexUrl>,
}

/// A package identity and version, including source and node kind, used when merging nodes.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResolutionNode {
    pub(crate) package: ResolutionPackage,
    pub(crate) version: Version,
}

/// An index into a single fork's node vector.
///
/// These indices are translated into merged graph positions when the fork completes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResolutionNodeId(pub(crate) usize);

/// A dependency between pinned packages, weighted by its marker.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResolutionDependencyEdge {
    /// This value is `None` if the dependency comes from the root package.
    pub(crate) from: Option<ResolutionNodeId>,
    pub(crate) to: ResolutionNodeId,
    pub(crate) marker: MarkerTree,
}

impl ResolutionDependencyEdge {
    pub(crate) fn universal_marker(&self) -> UniversalMarker {
        // We specifically do not account for conflict
        // markers here. Instead, those are computed via
        // a traversal on the resolution graph.
        UniversalMarker::new(self.marker, ConflictMarker::TRUE)
    }
}

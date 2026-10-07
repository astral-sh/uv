use std::fmt::Write;
use std::sync::Arc;

use rustc_hash::FxHashMap;
use tracing::{Level, trace};

use uv_distribution::Metadata;
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
    pub(crate) nodes: FxHashMap<ResolutionPackage, Version>,
    /// The directed connections between the nodes, where the marker is the node weight. We don't
    /// store the requirement itself, but it can be retrieved from the package metadata.
    pub(crate) edges: Vec<ResolutionDependencyEdge>,
    /// Map each package name, version tuple from `packages` to a distribution.
    pub(crate) pins: FilePins<'index>,
    /// The environment setting this resolution was found under.
    pub(crate) env: ResolverEnvironment,
}

/// A completed fork whose selected artifacts and metadata have been recovered.
#[derive(Debug)]
pub(crate) struct ResolvedFork {
    pub(crate) nodes: Vec<(ResolutionPackage, SelectedDistribution)>,
    pub(crate) edges: Vec<ResolutionDependencyEdge>,
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
        metadata: Metadata,
    },
    Registry {
        dist: ResolvedDist,
        metadata_id: DistributionId,
        /// Direct-only resolution need not fetch registry metadata.
        metadata: Option<Metadata>,
    },
}

impl SelectedDistribution {
    fn new(
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
            let MetadataResponse::Found(archive) = &*response else {
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
                metadata: archive.metadata.clone(),
            }
        } else {
            let (dist, metadata_id) =
                pins.dist_and_id(&package.name, &version).ok_or_else(|| {
                    ResolveError::UnregisteredTask(format!("{}=={version}", package.name))
                })?;
            let metadata = index.distributions().get(metadata_id).and_then(|response| {
                if let MetadataResponse::Found(archive) = &*response {
                    Some(archive.metadata.clone())
                } else {
                    None
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
    pub(crate) fn into_parts(self) -> (Version, ResolvedDist, Option<Metadata>) {
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
    /// Recover selected artifacts and metadata once before constructing the merged output graph.
    pub(crate) fn finalize(
        self,
        index: &InMemoryIndex,
        git: &GitResolver,
    ) -> Result<ResolvedFork, ResolveError> {
        let nodes = self
            .nodes
            .into_iter()
            .map(|(package, version)| {
                let selected =
                    SelectedDistribution::new(&package, version, &self.pins, index, git)?;
                Ok((package, selected))
            })
            .collect::<Result<_, ResolveError>>()?;
        Ok(ResolvedFork {
            nodes,
            edges: self.edges,
            env: self.env,
        })
    }

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
            trace!(
                "Resolution edge: {} -> {}",
                edge.from
                    .as_ref()
                    .map(|node| node.package.name.as_str())
                    .unwrap_or("ROOT"),
                edge.to.package.name,
            );
            // The unwraps below are OK because `write`ing to
            // a String can never fail (except for OOM).
            let mut msg = String::new();
            write!(
                msg,
                "{}",
                edge.from
                    .as_ref()
                    .map_or(&*MIN_VERSION, |node| &node.version)
            )
            .unwrap();
            if let Some(extra) = edge
                .from
                .as_ref()
                .and_then(|node| node.package.kind.extra())
            {
                write!(msg, " (extra: {extra})").unwrap();
            }
            if let Some(dev) = edge
                .from
                .as_ref()
                .and_then(|node| node.package.kind.group())
            {
                write!(msg, " (group: {dev})").unwrap();
            }

            write!(msg, " -> ").unwrap();

            write!(msg, "{}", edge.to.version).unwrap();
            if let Some(extra) = edge.to.package.kind.extra() {
                write!(msg, " (extra: {extra})").unwrap();
            }
            if let Some(dev) = edge.to.package.kind.group() {
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

/// A pinned package used as an endpoint in a resolution dependency edge.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResolutionNode {
    pub(crate) package: ResolutionPackage,
    pub(crate) version: Version,
}

/// A dependency between pinned packages, weighted by its marker.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResolutionDependencyEdge {
    /// This value is `None` if the dependency comes from the root package.
    pub(crate) from: Option<ResolutionNode>,
    pub(crate) to: ResolutionNode,
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

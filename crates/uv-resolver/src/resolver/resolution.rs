use std::fmt::Write;

use rustc_hash::FxHashMap;
use tracing::{Level, trace};

use uv_distribution_types::IndexUrl;
use uv_normalize::PackageName;
use uv_pep440::{MIN_VERSION, Version};
use uv_pep508::MarkerTree;
use uv_pypi_types::VerbatimParsedUrl;
use uv_resolver_types::PackageNodeKind;

use crate::pins::FilePins;
use crate::universal_marker::ConflictMarker;
use crate::{ResolverEnvironment, UniversalMarker};

/// The resolution from a single fork including the virtual packages and the edges between them.
#[derive(Debug)]
pub(crate) struct Resolution {
    pub(crate) nodes: FxHashMap<ResolutionPackage, Version>,
    /// The directed connections between the nodes, where the marker is the node weight. We don't
    /// store the requirement itself, but it can be retrieved from the package metadata.
    pub(crate) edges: Vec<ResolutionDependencyEdge>,
    /// Map each package name, version tuple from `packages` to a distribution.
    pub(crate) pins: FilePins,
    /// The environment setting this resolution was found under.
    pub(crate) env: ResolverEnvironment,
}

impl Resolution {
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

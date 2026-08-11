use std::fmt::Write;

use rustc_hash::FxHashMap;
use tracing::{Level, trace};

use uv_distribution_types::IndexUrl;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::Version;
use uv_pep508::MarkerTree;
use uv_pypi_types::VerbatimParsedUrl;

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
                    .map(PackageName::as_str)
                    .unwrap_or("ROOT"),
                edge.to,
            );
            // The unwraps below are OK because `write`ing to
            // a String can never fail (except for OOM).
            let mut msg = String::new();
            write!(msg, "{}", edge.from_version).unwrap();
            if let Some(ref extra) = edge.from_extra {
                write!(msg, " (extra: {extra})").unwrap();
            }
            if let Some(ref dev) = edge.from_group {
                write!(msg, " (group: {dev})").unwrap();
            }

            write!(msg, " -> ").unwrap();

            write!(msg, "{}", edge.to_version).unwrap();
            if let Some(ref extra) = edge.to_extra {
                write!(msg, " (extra: {extra})").unwrap();
            }
            if let Some(ref dev) = edge.to_group {
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
    pub(crate) extra: Option<ExtraName>,
    pub(crate) dev: Option<GroupName>,
    /// For registry packages, this is `None`; otherwise, the direct URL of the distribution.
    pub(crate) url: Option<VerbatimParsedUrl>,
    /// For URL packages, this is `None`; otherwise, the index URL of the distribution.
    pub(crate) index: Option<IndexUrl>,
}

/// The `from_` fields and the `to_` fields allow mapping to the originating and target
///  [`ResolutionPackage`] respectively. The `marker` is the edge weight.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResolutionDependencyEdge {
    /// This value is `None` if the dependency comes from the root package.
    pub(crate) from: Option<PackageName>,
    pub(crate) from_version: Version,
    pub(crate) from_url: Option<VerbatimParsedUrl>,
    pub(crate) from_index: Option<IndexUrl>,
    pub(crate) from_extra: Option<ExtraName>,
    pub(crate) from_group: Option<GroupName>,
    pub(crate) to: PackageName,
    pub(crate) to_version: Version,
    pub(crate) to_url: Option<VerbatimParsedUrl>,
    pub(crate) to_index: Option<IndexUrl>,
    pub(crate) to_extra: Option<ExtraName>,
    pub(crate) to_group: Option<GroupName>,
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

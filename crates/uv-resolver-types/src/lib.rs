//! Data shared by dependency resolution and lockfile handling.

mod distribution;
pub mod graph_ops;
mod metadata;
mod options;
mod output;
pub mod universal_marker;

pub use distribution::{AnnotatedDist, PackageNodeKind};
pub use metadata::{DistributionMetadataIndex, MetadataResponse, MetadataUnavailable};
pub use options::{Flexibility, Options, OptionsBuilder};
pub use output::{ConflictingDistributionError, ResolutionGraphNode, ResolverOutput};
pub use universal_marker::{ConflictMarker, ConflictMarkerError, UniversalMarker};

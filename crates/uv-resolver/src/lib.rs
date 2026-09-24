pub use error::{ErrorTree, NoSolutionError, NoSolutionHeader, ResolveError};
pub use exclusions::Exclusions;
pub use flat_index::{FlatDistributions, FlatIndex};
pub use manifest::Manifest;
pub use preferences::{Preference, PreferenceError, Preferences};
pub use pubgrub::PubGrubHint;
pub use python_requirement::PythonRequirement;
pub use resolution::{ConflictingDistributionError, DisplayResolutionGraph, ResolverOutput};
pub use resolver::{
    DefaultResolverProvider, InMemoryIndex, MetadataResponse, PackageVersionsResult,
    Reporter as ResolverReporter, Resolver, ResolverEnvironment, ResolverProvider,
    VersionsResponse, WheelMetadataResult,
};
pub use universal_marker::UniversalMarker;
pub use upgrade::UpgradePackages;
pub use uv_configuration::{
    AnnotationStyle, DependencyMode, ExcludeNewer, ExcludeNewerChange, ExcludeNewerOverrideChange,
    ExcludeNewerPackage, ExcludeNewerPackageChange, ExcludeNewerPackageEntry,
    ExcludeNewerValueChange, ExcludeNewerValueWithSpanRef, ForkStrategy, Prerelease,
    PrereleaseMode, PrereleasePackage, PrereleasePackageEntry, ResolutionMode,
    serialize_exclude_newer_package_with_spans,
};
pub use uv_distribution_types::{ExcludeNewerOverride, ExcludeNewerSpan, ExcludeNewerValue};
pub use uv_resolver_types::{Flexibility, Options, OptionsBuilder};
pub use version_map::VersionMap;
pub use yanks::AllowedYanks;

use uv_resolver_types::{graph_ops, universal_marker};

/// A custom `HashSet` using `hashbrown`.
///
/// We use `hashbrown` instead of `std` to get access to its `Equivalent`
/// trait. This lets use store things like `ConflictItem`, but refer to it via
/// `ConflictItemRef`. i.e., We can avoid allocs on lookups.
type FxHashbrownSet<T> = hashbrown::HashSet<T, rustc_hash::FxBuildHasher>;

type FxHashbrownMap<K, V> = hashbrown::HashMap<K, V, rustc_hash::FxBuildHasher>;

mod candidate_selector;
mod dependency_provider;
mod error;
mod exclusions;
mod flat_index;
mod fork_indexes;
mod fork_urls;
mod manifest;
mod marker;
mod pins;
mod preferences;
mod prerelease;
pub mod pubgrub;
mod python_requirement;
mod redirect;
mod resolution;
mod resolution_mode;
mod resolver;
mod upgrade;
mod version_map;
mod yanks;

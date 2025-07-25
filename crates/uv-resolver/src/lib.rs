pub use dependency_mode::DependencyMode;
pub use error::{ErrorTree, NoSolutionError, NoSolutionHeader, ResolveError, SentinelRange};
pub use exclude_newer::{
    ExcludeNewer, ExcludeNewerPackage, ExcludeNewerPackageEntry, ExcludeNewerTimestamp,
};
pub use exclusions::Exclusions;
pub use flat_index::{FlatDistributions, FlatIndex};
pub use fork_strategy::ForkStrategy;
pub use lock::{
    Installable, Lock, LockError, LockVersion, Package, PackageMap, PylockToml,
    PylockTomlErrorKind, RequirementsTxtExport, ResolverManifest, SatisfiesResult, TreeDisplay,
    VERSION,
};
pub use manifest::Manifest;
pub use options::{Flexibility, Options, OptionsBuilder};
pub use preferences::{Preference, PreferenceError, Preferences};
pub use prerelease::PrereleaseMode;
pub use python_requirement::PythonRequirement;
pub use resolution::{
    AnnotationStyle, ConflictingDistributionError, DisplayResolutionGraph, ResolverOutput,
};
pub use resolution_mode::ResolutionMode;
pub use resolver::{
    BuildId, DefaultResolverProvider, DerivationChainBuilder, InMemoryIndex, MetadataResponse,
    PackageVersionsResult, Reporter as ResolverReporter, Resolver, ResolverEnvironment,
    ResolverProvider, VariantProviderResult, VersionsResponse, WheelMetadataResult,
};
pub use universal_marker::{ConflictMarker, UniversalMarker};
pub use version_map::VersionMap;
pub use yanks::AllowedYanks;

/// A custom `HashSet` using `hashbrown`.
///
/// We use `hashbrown` instead of `std` to get access to its `Equivalent`
/// trait. This lets use store things like `ConflictItem`, but refer to it via
/// `ConflictItemRef`. i.e., We can avoid allocs on lookups.
type FxHashbrownSet<T> = hashbrown::HashSet<T, rustc_hash::FxBuildHasher>;

type FxHashbrownMap<K, V> = hashbrown::HashMap<K, V, rustc_hash::FxBuildHasher>;

mod candidate_selector;
mod dependency_mode;
mod dependency_provider;
mod error;
mod exclude_newer;
mod exclusions;
mod flat_index;
mod fork_indexes;
mod fork_strategy;
mod fork_urls;
mod graph_ops;
mod lock;
mod manifest;
mod marker;
mod options;
mod pins;
mod preferences;
mod prerelease;
pub mod pubgrub;
mod python_requirement;
mod redirect;
mod resolution;
mod resolution_mode;
mod resolver;
mod universal_marker;
mod version_map;
mod yanks;

pub use dependency_mode::DependencyMode;
pub use error::{NoSolutionError, NoSolutionHeader, ResolveError, SentinelRange};
pub use exclude_newer::ExcludeNewer;
pub use exclusions::Exclusions;
pub use flat_index::{FlatDistributions, FlatIndex};
pub use lock::{
    InstallTarget, Lock, LockError, LockVersion, PackageMap, RequirementsTxtExport,
    ResolverManifest, SatisfiesResult, TreeDisplay, VERSION,
};
pub use manifest::Manifest;
pub use options::{Flexibility, Options, OptionsBuilder};
pub use preferences::{Preference, PreferenceError, Preferences};
pub use prerelease::PrereleaseMode;
pub use python_requirement::PythonRequirement;
pub use requires_python::{RequiresPython, RequiresPythonRange};
pub use resolution::{
    AnnotationStyle, ConflictingDistributionError, DisplayResolutionGraph, ResolverOutput,
};
pub use resolution_mode::ResolutionMode;
pub use resolver::{
    BuildId, DefaultResolverProvider, DerivationChainBuilder, InMemoryIndex, MetadataResponse,
    PackageVersionsResult, Reporter as ResolverReporter, Resolver, ResolverEnvironment,
    ResolverProvider, VersionsResponse, WheelMetadataResult,
};
pub use version_map::VersionMap;
pub use yanks::AllowedYanks;

/// A custom `HashSet` using `hashbrown`.
///
/// We use `hashbrown` instead of `std` to get access to its `Equivalent`
/// trait. This lets use store things like `ConflictItem`, but refer to it via
/// `ConflictItemRef`. i.e., We can avoid allocs on lookups.
type FxHashbrownSet<T> = hashbrown::HashSet<T, rustc_hash::FxBuildHasher>;

mod candidate_selector;
mod dependency_mode;
mod dependency_provider;
mod error;
mod exclude_newer;
mod exclusions;
mod flat_index;
mod fork_indexes;
mod fork_urls;
mod graph_ops;
mod lock;
mod manifest;
mod marker;
mod options;
mod pins;
mod preferences;
mod prerelease;
mod pubgrub;
mod python_requirement;
mod redirect;
mod requires_python;
mod resolution;
mod resolution_mode;
mod resolver;
mod version_map;
mod yanks;

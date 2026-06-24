use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

use dashmap::DashMap;

use uv_configuration::{BuildKind, NoSources};
use uv_distribution_types::{Requirement, Resolution, ResolvedDist, SourceDist};
use uv_normalize::{ExtraName, PackageName};
use uv_pep440::Version;
use uv_pep508::MarkerTree;
use uv_pypi_types::HashDigest;
use uv_python::PythonEnvironment;

/// Whether to enforce build isolation when building source distributions.
#[derive(Debug, Default, Copy, Clone)]
pub enum BuildIsolation<'a> {
    #[default]
    Isolated,
    Shared(&'a PythonEnvironment),
    SharedPackage(&'a PythonEnvironment, &'a [PackageName]),
}

impl BuildIsolation<'_> {
    /// Returns `true` if build isolation is enforced for the given package name.
    pub fn is_isolated(&self, package: Option<&PackageName>) -> bool {
        match self {
            Self::Isolated => true,
            Self::Shared(_) => false,
            Self::SharedPackage(_, packages) => {
                package.is_none_or(|package| !packages.iter().any(|p| p == package))
            }
        }
    }

    /// Returns the shared environment for a given package, if build isolation is not enforced.
    pub fn shared_environment(&self, package: Option<&PackageName>) -> Option<&PythonEnvironment> {
        match self {
            Self::Isolated => None,
            Self::Shared(env) => Some(env),
            Self::SharedPackage(env, packages) => {
                if package.is_some_and(|package| packages.iter().any(|p| p == package)) {
                    Some(env)
                } else {
                    None
                }
            }
        }
    }
}

/// A key for the build cache, which includes the interpreter, source root, subdirectory, source
/// strategy, and build kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BuildKey {
    pub base_python: Box<Path>,
    pub source_root: Box<Path>,
    pub subdirectory: Option<Box<Path>>,
    pub no_sources: NoSources,
    pub build_kind: BuildKind,
}

/// An arena of in-process builds.
#[derive(Debug)]
pub struct BuildArena<T>(Arc<DashMap<BuildKey, T>>);

impl<T> Default for BuildArena<T> {
    fn default() -> Self {
        Self(Arc::new(DashMap::new()))
    }
}

impl<T> Clone for BuildArena<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> BuildArena<T> {
    /// Insert a build entry into the arena.
    pub fn insert(&self, key: BuildKey, value: T) {
        self.0.insert(key, value);
    }

    /// Remove a build entry from the arena.
    pub fn remove(&self, key: &BuildKey) -> Option<T> {
        self.0.remove(key).map(|entry| entry.1)
    }
}

/// A resolved build dependency with its marker (an edge from the resolution root).
#[derive(Debug, Clone)]
pub struct ResolvedBuildDependency {
    /// The resolved distribution.
    pub dist: ResolvedDist,
    /// The hashes for verification.
    pub hashes: Vec<HashDigest>,
    /// The marker indicating when this dependency is needed.
    pub marker: MarkerTree,
    /// The extras requested on this direct build requirement.
    pub extras: BTreeSet<ExtraName>,
}

/// A dependency edge in the build resolution graph.
#[derive(Debug, Clone)]
pub struct BuildDependencyEdge {
    /// The resolved distribution.
    pub dist: ResolvedDist,
    /// The marker for when this dependency is needed.
    pub marker: MarkerTree,
    /// The extras requested on this dependency edge.
    pub extras: BTreeSet<ExtraName>,
}

/// A package in the build resolution graph, with its direct dependencies.
#[derive(Debug, Clone)]
pub struct BuildDependencyPackage {
    /// The resolved distribution.
    pub dist: ResolvedDist,
    /// The hashes for verification.
    pub hashes: Vec<HashDigest>,
    /// The marker environments in which this package is reachable.
    pub marker: MarkerTree,
    /// This package's direct dependencies with markers.
    pub dependencies: Vec<BuildDependencyEdge>,
    /// This package's extra dependencies with markers.
    pub optional_dependencies: BTreeMap<ExtraName, Vec<BuildDependencyEdge>>,
}

/// The build resolution graph for a single package: direct build requirements
/// and all packages with their dependency edges.
#[derive(Debug, Clone, Default)]
pub struct BuildResolutionGraph {
    /// Direct build requirements (edges from the resolution root to the
    /// packages that `build-system.requires` lists).
    pub direct_dependencies: Vec<ResolvedBuildDependency>,
    /// All packages in the resolution with their direct dependencies.
    pub packages: Vec<BuildDependencyPackage>,
}

/// A source discriminator for a package key used by build dependency locking.
///
/// This intentionally stores canonicalized strings so the key can be serialized,
/// compared, and reconstructed across crates without carrying resolver-specific
/// source types.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BuildPackageSource {
    Registry(String),
    DirectUrl(String),
    Git(String),
    Path(String),
    Directory(String),
    Editable(String),
    Virtual(String),
}

impl BuildPackageSource {
    /// Construct a source discriminator from a [`SourceDist`].
    pub fn from_source_dist(source: &SourceDist) -> Self {
        match source {
            SourceDist::Registry(dist) => {
                Self::Registry(dist.index.without_credentials().as_ref().to_string())
            }
            SourceDist::DirectUrl(dist) => Self::DirectUrl(dist.url.to_string()),
            SourceDist::GitDirectory(dist) => Self::Git(dist.url.to_string()),
            SourceDist::GitPath(dist) => Self::Git(dist.url.to_string()),
            SourceDist::Path(dist) => Self::Path(dist.url.to_string()),
            SourceDist::Directory(dist) => {
                if dist.editable.unwrap_or(false) {
                    Self::Editable(dist.url.to_string())
                } else if dist.r#virtual.unwrap_or(false) {
                    Self::Virtual(dist.url.to_string())
                } else {
                    Self::Directory(dist.url.to_string())
                }
            }
        }
    }
}

/// A key identifying a package by name, optional version, and source.
///
/// This mirrors `PackageId` semantics for build-locking, while allowing
/// `version = None` for dynamic local sources whose version is unknown until
/// metadata is built.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BuildPackageKey {
    /// The package name.
    pub name: PackageName,
    /// The package version, if known.
    ///
    /// This is `None` for source trees with dynamic metadata (for example,
    /// `dynamic = ["version"]`) before metadata is available.
    pub version: Option<Version>,
    /// The package source discriminator.
    pub source: Option<BuildPackageSource>,
}

impl BuildPackageKey {
    /// Create a new key from package name and version.
    pub fn new(name: PackageName, version: Option<Version>) -> Self {
        Self {
            name,
            version,
            source: None,
        }
    }

    /// Create a new key from package fields.
    pub fn with_source(
        name: PackageName,
        version: Option<Version>,
        source: Option<BuildPackageSource>,
    ) -> Self {
        Self {
            name,
            version,
            source,
        }
    }

    /// Create a new key from package fields and an optional source distribution.
    pub fn from_source_dist(
        name: PackageName,
        version: Option<Version>,
        source: Option<&SourceDist>,
    ) -> Self {
        Self {
            name,
            version,
            source: source.map(BuildPackageSource::from_source_dist),
        }
    }
}

/// The PEP 517 requirement-discovery stage captured by a build resolution graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BuildResolutionStage {
    /// The environment used before calling `get_requires_for_build_*`.
    Bootstrap,
    /// The environment used after adding requirements returned by `get_requires_for_build_*`.
    Build,
}

impl BuildResolutionStage {
    /// Return the serialized stage name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::Build => "build",
        }
    }
}

/// The build operation captured by a build resolution graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BuildResolutionOperation {
    /// A PEP 517 wheel build.
    Wheel,
    /// A PEP 660 editable wheel build.
    Editable,
}

impl BuildResolutionOperation {
    /// Return the operation for a source distribution.
    pub fn from_source_dist(source_dist: &SourceDist) -> Self {
        if source_dist.is_editable() {
            Self::Editable
        } else {
            Self::Wheel
        }
    }

    /// Return the serialized operation name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wheel => "wheel",
            Self::Editable => "editable",
        }
    }
}

/// A captured build resolution graph key.
///
/// A source package can have multiple independently resolved build graphs when
/// it is built for different target, executor, or requirement-discovery stage
/// contexts. The optional context distinguishes those captures while preserving
/// the legacy package-only key for callers that have not been moved to
/// first-class resolution contexts yet.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BuildResolutionGraphKey {
    /// The source package being built.
    pub package: BuildPackageKey,
    /// The operation used to build the source package.
    pub operation: BuildResolutionOperation,
    /// The captured resolution context identity, when known.
    pub context: Option<String>,
    /// The requirement-discovery stage captured by the graph.
    pub stage: Option<BuildResolutionStage>,
    /// The target marker reachability used to derive the context, when known.
    pub target_marker: Option<MarkerTree>,
}

impl BuildResolutionGraphKey {
    /// Create a package-only build resolution graph key.
    pub fn package(package: BuildPackageKey) -> Self {
        Self {
            package,
            operation: BuildResolutionOperation::Wheel,
            context: None,
            stage: None,
            target_marker: None,
        }
    }

    /// Create a context-qualified build resolution graph key.
    pub fn context(package: BuildPackageKey, context: String) -> Self {
        Self {
            package,
            operation: BuildResolutionOperation::Wheel,
            context: Some(context),
            stage: Some(BuildResolutionStage::Build),
            target_marker: None,
        }
    }

    /// Create a context-qualified build resolution graph key with target reachability.
    pub fn context_with_marker(
        package: BuildPackageKey,
        context: String,
        stage: BuildResolutionStage,
        target_marker: Option<MarkerTree>,
    ) -> Self {
        Self::context_with_marker_and_operation(
            package,
            BuildResolutionOperation::Wheel,
            context,
            stage,
            target_marker,
        )
    }

    /// Create an operation- and context-qualified build resolution graph key with target
    /// reachability.
    pub fn context_with_marker_and_operation(
        package: BuildPackageKey,
        operation: BuildResolutionOperation,
        context: String,
        stage: BuildResolutionStage,
        target_marker: Option<MarkerTree>,
    ) -> Self {
        Self {
            package,
            operation,
            context: Some(context),
            stage: Some(stage),
            target_marker,
        }
    }

    /// Return a copy of the key with a new stage and context.
    #[must_use]
    pub fn with_stage(mut self, context: String, stage: BuildResolutionStage) -> Self {
        self.context = Some(context);
        self.stage = Some(stage);
        self
    }
}

/// Map of build resolution graph keys to their captured graphs.
pub type BuildResolutionGraphMap = BTreeMap<BuildResolutionGraphKey, BuildResolutionGraph>;

fn get_unambiguous_key<'a, T>(
    map: &'a BTreeMap<BuildPackageKey, T>,
    package: &BuildPackageKey,
) -> Option<&'a T> {
    if let Some(value) = map.get(package) {
        return Some(value);
    }

    let mut version_matches = map
        .iter()
        .filter(|(key, _)| key.name == package.name && key.version == package.version)
        .map(|(_, value)| value);

    if let Some(first) = version_matches.next() {
        if version_matches.next().is_none() {
            return Some(first);
        }
        return None;
    }

    if package.version.is_none() {
        let mut name_matches = map
            .iter()
            .filter(|(key, _)| key.name == package.name)
            .map(|(_, value)| value);
        let first = name_matches.next()?;
        if name_matches.next().is_none() {
            return Some(first);
        }
    }

    None
}

fn get_unambiguous_graph<'a>(
    map: &'a BuildResolutionGraphMap,
    package: &BuildPackageKey,
) -> Option<&'a BuildResolutionGraph> {
    if let Some(value) = map.get(&BuildResolutionGraphKey::package(package.clone())) {
        return Some(value);
    }

    let mut version_matches = map
        .iter()
        .filter(|(key, _)| {
            key.package.name == package.name && key.package.version == package.version
        })
        .map(|(_, value)| value);

    if let Some(first) = version_matches.next() {
        if version_matches.next().is_none() {
            return Some(first);
        }
        return None;
    }

    if package.version.is_none() {
        let mut name_matches = map
            .iter()
            .filter(|(key, _)| key.package.name == package.name)
            .map(|(_, value)| value);
        let first = name_matches.next()?;
        if name_matches.next().is_none() {
            return Some(first);
        }
    }

    None
}

/// Locked build dependency resolutions, indexed by package key.
#[derive(Debug, Default, Clone)]
pub struct LockedBuildResolutions(BTreeMap<BuildPackageKey, LockedBuildResolution>);

/// A locked build dependency resolution and its direct requirements.
#[derive(Debug, Clone)]
pub struct LockedBuildResolution {
    resolution: Resolution,
    direct_dependencies: Vec<LockedBuildDependency>,
    bootstrap_direct_dependencies: Option<Vec<LockedBuildDependency>>,
    initial_requirements: Option<Vec<Requirement>>,
}

impl LockedBuildResolution {
    /// Create a locked build dependency resolution.
    pub fn new(
        resolution: Resolution,
        direct_dependencies: Vec<LockedBuildDependency>,
        initial_requirements: Option<Vec<Requirement>>,
    ) -> Self {
        Self {
            resolution,
            direct_dependencies,
            bootstrap_direct_dependencies: None,
            initial_requirements,
        }
    }

    /// Attach the direct dependencies for the initial backend-hook environment.
    #[must_use]
    pub fn with_bootstrap_direct_dependencies(
        mut self,
        bootstrap_direct_dependencies: Vec<LockedBuildDependency>,
    ) -> Self {
        self.bootstrap_direct_dependencies = Some(bootstrap_direct_dependencies);
        self
    }

    /// Return the installable resolution.
    pub fn resolution(&self) -> &Resolution {
        &self.resolution
    }

    /// Return the direct build dependencies.
    pub fn direct_dependencies(&self) -> &[LockedBuildDependency] {
        &self.direct_dependencies
    }

    /// Return the direct build dependencies used before backend hooks are called.
    pub fn bootstrap_direct_dependencies(&self) -> Option<&[LockedBuildDependency]> {
        self.bootstrap_direct_dependencies.as_deref()
    }

    /// Return the requirements used for the initial backend environment, if recorded.
    pub fn initial_requirements(&self) -> Option<&[Requirement]> {
        self.initial_requirements.as_deref()
    }
}

/// A direct dependency in a locked build resolution.
#[derive(Debug, Clone)]
pub struct LockedBuildDependency {
    /// The resolved distribution.
    pub dist: ResolvedDist,
    /// The extras requested on this direct build requirement.
    pub extras: BTreeSet<ExtraName>,
    /// The selected distribution and its transitive installable dependencies.
    resolution: Resolution,
}

impl LockedBuildDependency {
    /// Create a locked direct build dependency and its installable closure.
    pub fn new(dist: ResolvedDist, extras: BTreeSet<ExtraName>, resolution: Resolution) -> Self {
        Self {
            dist,
            extras,
            resolution,
        }
    }

    /// Return the installable closure for this direct dependency.
    pub fn resolution(&self) -> &Resolution {
        &self.resolution
    }
}

impl LockedBuildResolutions {
    /// Create locked build resolutions from a map keyed by [`BuildPackageKey`].
    pub fn new(map: BTreeMap<BuildPackageKey, LockedBuildResolution>) -> Self {
        Self(map)
    }

    /// Get the pre-built resolution for a package key.
    pub fn get(&self, package: &BuildPackageKey) -> Option<&LockedBuildResolution> {
        get_unambiguous_key(&self.0, package)
    }
}

/// A list of `(name, version)` pairs representing preferred build dependency versions.
type BuildDependencyVersions = Vec<(PackageName, Version)>;

/// Build dependency version preferences, indexed by package key.
#[derive(Debug, Default, Clone)]
pub struct BuildPreferences(BTreeMap<BuildPackageKey, BuildDependencyVersions>);

impl BuildPreferences {
    /// Create build preferences from a map keyed by [`BuildPackageKey`].
    pub fn new(map: BTreeMap<BuildPackageKey, BuildDependencyVersions>) -> Self {
        Self(map)
    }

    /// Get the build dependency preferences for a package key.
    pub fn get(&self, package: &BuildPackageKey) -> Option<&[(PackageName, Version)]> {
        get_unambiguous_key(&self.0, package).map(Vec::as_slice)
    }
}

/// Captured build dependency resolutions with markers.
#[derive(Debug, Default, Clone)]
pub struct BuildResolutions(Arc<Mutex<BuildResolutionGraphMap>>);

impl BuildResolutions {
    /// Record a build resolution for the given package key.
    pub fn insert(&self, package: BuildPackageKey, graph: BuildResolutionGraph) {
        self.insert_key(BuildResolutionGraphKey::package(package), graph);
    }

    /// Record a build resolution for the given context-qualified key.
    pub fn insert_key(&self, key: BuildResolutionGraphKey, graph: BuildResolutionGraph) {
        let mut graphs = self.0.lock().unwrap();
        graphs.insert(key, graph);
    }

    /// Get the exact graph for a package key, or the only graph with a matching
    /// name and version when source identity is unavailable.
    pub fn get_unambiguous(&self, package: &BuildPackageKey) -> Option<BuildResolutionGraph> {
        let graphs = self.0.lock().unwrap();
        get_unambiguous_graph(&graphs, package).cloned()
    }

    /// Get the exact graph for a build resolution graph key.
    pub fn get(&self, key: &BuildResolutionGraphKey) -> Option<BuildResolutionGraph> {
        let graphs = self.0.lock().unwrap();
        graphs.get(key).cloned()
    }

    /// Get a legacy package-keyed snapshot of unqualified build resolutions.
    pub fn snapshot(&self) -> BTreeMap<BuildPackageKey, BuildResolutionGraph> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(key, _)| key.context.is_none())
            .map(|(key, graph)| (key.package.clone(), graph.clone()))
            .collect()
    }

    /// Get all captured build resolutions, including context-qualified graphs.
    pub fn snapshot_contexts(&self) -> BuildResolutionGraphMap {
        self.0.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn package_key() -> BuildPackageKey {
        BuildPackageKey::new(
            PackageName::from_str("dep").expect("valid package name"),
            Some(Version::from_str("1.0.0").expect("valid version")),
        )
    }

    #[test]
    fn build_resolutions_retain_contextual_graphs_for_same_package() {
        let package = package_key();
        let build_resolutions = BuildResolutions::default();

        build_resolutions.insert_key(
            BuildResolutionGraphKey::context(package.clone(), "build:dep:wheel:one".to_string()),
            BuildResolutionGraph::default(),
        );
        build_resolutions.insert_key(
            BuildResolutionGraphKey::context(package.clone(), "build:dep:wheel:two".to_string()),
            BuildResolutionGraph::default(),
        );

        let graphs = build_resolutions.snapshot_contexts();
        assert_eq!(graphs.len(), 2);
        let one =
            BuildResolutionGraphKey::context(package.clone(), "build:dep:wheel:one".to_string());
        let two =
            BuildResolutionGraphKey::context(package.clone(), "build:dep:wheel:two".to_string());
        assert!(graphs.contains_key(&one));
        assert!(graphs.contains_key(&two));
        assert!(build_resolutions.get(&one).is_some());
        assert!(build_resolutions.get(&two).is_some());
        assert!(build_resolutions.get_unambiguous(&package).is_none());
        assert!(build_resolutions.snapshot().is_empty());
    }

    #[test]
    fn build_resolutions_retain_stage_qualified_context_captures() {
        let package = package_key();
        let build_resolutions = BuildResolutions::default();
        let context = "build:dep:wheel:one".to_string();
        let first = BuildResolutionGraphKey::context(package.clone(), context.clone());
        let second = BuildResolutionGraphKey::context_with_marker(
            package,
            context,
            BuildResolutionStage::Bootstrap,
            Some(MarkerTree::TRUE),
        );

        build_resolutions.insert_key(first.clone(), BuildResolutionGraph::default());
        build_resolutions.insert_key(second.clone(), BuildResolutionGraph::default());

        let graphs = build_resolutions.snapshot_contexts();
        assert_eq!(graphs.len(), 2);
        assert!(graphs.contains_key(&first));
        assert!(graphs.contains_key(&second));
    }

    #[test]
    fn build_resolutions_keep_legacy_package_keyed_snapshot() {
        let package = package_key();
        let build_resolutions = BuildResolutions::default();

        build_resolutions.insert(package.clone(), BuildResolutionGraph::default());

        assert!(build_resolutions.get_unambiguous(&package).is_some());
        assert_eq!(build_resolutions.snapshot().len(), 1);
        assert_eq!(build_resolutions.snapshot_contexts().len(), 1);
    }
}

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use dashmap::DashMap;

use uv_configuration::{BuildKind, NoSources};
use uv_distribution_types::{Resolution, ResolvedDist, SourceDist};
use uv_normalize::PackageName;
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

/// A resolved build dependency, representing an edge from the resolution root.
#[derive(Debug, Clone)]
pub struct ResolvedBuildDependency {
    /// The resolved distribution.
    pub dist: ResolvedDist,
    /// The hashes for verification.
    pub hashes: Vec<HashDigest>,
    /// The marker indicating when this dependency is needed.
    pub marker: MarkerTree,
}

/// A dependency edge in the build resolution graph.
#[derive(Debug, Clone)]
pub struct BuildDependencyEdge {
    /// The package name of the dependency.
    pub name: PackageName,
    /// The version of the dependency.
    pub version: Version,
    /// The marker for when this dependency is needed.
    pub marker: MarkerTree,
}

/// A package in the build resolution graph, with its direct dependencies.
#[derive(Debug, Clone)]
pub struct BuildDependencyPackage {
    /// The resolved distribution.
    pub dist: ResolvedDist,
    /// The hashes for verification.
    pub hashes: Vec<HashDigest>,
    /// This package's direct dependencies with markers.
    pub dependencies: Vec<BuildDependencyEdge>,
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
            SourceDist::Git(dist) => Self::Git(dist.url.to_string()),
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BuildPackageKey {
    /// The package name.
    pub name: PackageName,
    /// The package version, if known.
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

/// Map of package keys to their build resolution graphs.
type BuildResolutionGraphMap = BTreeMap<BuildPackageKey, BuildResolutionGraph>;

/// Locked build dependency resolutions, indexed by package key.
#[derive(Debug, Default, Clone)]
pub struct LockedBuildResolutions(BTreeMap<BuildPackageKey, Resolution>);

impl LockedBuildResolutions {
    /// Create locked build resolutions from a map keyed by [`BuildPackageKey`].
    pub fn new(map: BTreeMap<BuildPackageKey, Resolution>) -> Self {
        Self(map)
    }

    /// Get the pre-built resolution for a package key.
    pub fn get(&self, package: &BuildPackageKey) -> Option<&Resolution> {
        if let Some(resolution) = self.0.get(package) {
            return Some(resolution);
        }

        let mut version_matches = self
            .0
            .iter()
            .filter(|(key, _)| key.name == package.name && key.version == package.version)
            .map(|(_, resolution)| resolution);

        if let Some(first) = version_matches.next() {
            if version_matches.next().is_none() {
                return Some(first);
            }
            return None;
        }

        if package.version.is_none() {
            let mut name_matches = self
                .0
                .iter()
                .filter(|(key, _)| key.name == package.name)
                .map(|(_, resolution)| resolution);
            let first = name_matches.next()?;
            if name_matches.next().is_none() {
                return Some(first);
            }
        }

        None
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
        if let Some(preferences) = self.0.get(package) {
            return Some(preferences.as_slice());
        }

        let mut version_matches = self
            .0
            .iter()
            .filter(|(key, _)| key.name == package.name && key.version == package.version)
            .map(|(_, preferences)| preferences.as_slice());

        if let Some(first) = version_matches.next() {
            if version_matches.next().is_none() {
                return Some(first);
            }
            return None;
        }

        if package.version.is_none() {
            let mut name_matches = self
                .0
                .iter()
                .filter(|(key, _)| key.name == package.name)
                .map(|(_, preferences)| preferences.as_slice());
            let first = name_matches.next()?;
            if name_matches.next().is_none() {
                return Some(first);
            }
        }

        None
    }
}

/// Captured build dependency resolutions with markers.
#[derive(Debug, Default, Clone)]
pub struct BuildResolutions(Arc<Mutex<BuildResolutionGraphMap>>);

impl BuildResolutions {
    /// Record a build resolution for the given package key.
    pub fn insert(&self, package: BuildPackageKey, graph: BuildResolutionGraph) {
        self.0.lock().unwrap().insert(package, graph);
    }

    /// Get a snapshot of the current build resolutions.
    pub fn snapshot(&self) -> BuildResolutionGraphMap {
        self.0.lock().unwrap().clone()
    }
}

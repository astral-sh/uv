use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use dashmap::DashMap;

use uv_configuration::{BuildKind, NoSources};
use uv_distribution_types::{Resolution, ResolvedDist};
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

/// A resolved build dependency with its marker (an edge from the resolution root).
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
/// (roots) and all packages with their dependency edges.
#[derive(Debug, Clone, Default)]
pub struct BuildResolutionGraph {
    /// Direct build requirements (edges from the resolution root).
    pub roots: Vec<ResolvedBuildDependency>,
    /// All packages in the resolution with their direct dependencies.
    pub packages: Vec<BuildDependencyPackage>,
}

/// Map of (package name, optional version) to their build resolution graphs.
type BuildResolutionGraphMap = BTreeMap<(PackageName, Option<Version>), BuildResolutionGraph>;

/// Locked build dependency resolutions, indexed by (package name, optional version).
#[derive(Debug, Default, Clone)]
pub struct LockedBuildResolutions(BTreeMap<(PackageName, Option<Version>), Resolution>);

impl LockedBuildResolutions {
    /// Create locked build resolutions from a map keyed by `(name, optional version)`.
    pub fn new(map: BTreeMap<(PackageName, Option<Version>), Resolution>) -> Self {
        Self(map)
    }

    /// Get the pre-built resolution for a given package name and version.
    pub fn get(&self, package: &PackageName, version: Option<&Version>) -> Option<&Resolution> {
        self.0.get(&(package.clone(), version.cloned()))
    }
}

/// A list of `(name, version)` pairs representing preferred build dependency versions.
type BuildDependencyVersions = Vec<(PackageName, Version)>;

/// Build dependency version preferences, indexed by (package name, optional version).
#[derive(Debug, Default, Clone)]
pub struct BuildPreferences(BTreeMap<(PackageName, Option<Version>), BuildDependencyVersions>);

impl BuildPreferences {
    /// Create build preferences from a map keyed by `(name, optional version)`.
    pub fn new(map: BTreeMap<(PackageName, Option<Version>), BuildDependencyVersions>) -> Self {
        Self(map)
    }

    /// Get the build dependency preferences for a given package name and version.
    pub fn get(
        &self,
        package: &PackageName,
        version: Option<&Version>,
    ) -> Option<&[(PackageName, Version)]> {
        self.0
            .get(&(package.clone(), version.cloned()))
            .map(Vec::as_slice)
    }
}

/// Captured build dependency resolutions with markers.
#[derive(Debug, Default, Clone)]
pub struct BuildResolutions(Arc<Mutex<BuildResolutionGraphMap>>);

impl BuildResolutions {
    /// Record a build resolution for the given package name and optional version.
    pub fn insert(
        &self,
        package: PackageName,
        version: Option<Version>,
        graph: BuildResolutionGraph,
    ) {
        self.0.lock().unwrap().insert((package, version), graph);
    }

    /// Get a snapshot of the current build resolutions.
    pub fn snapshot(&self) -> BuildResolutionGraphMap {
        self.0.lock().unwrap().clone()
    }
}

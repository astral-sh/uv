use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use dashmap::DashMap;

use uv_configuration::{BuildKind, NoSources};
use uv_distribution_types::Resolution;
use uv_normalize::PackageName;
use uv_pep440::Version;
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

/// Previously resolved build dependencies from the lock file, stored as complete
/// [`Resolution`] objects that can be used directly without re-resolving.
///
/// Used during `uv sync` to skip the resolver entirely for build dependencies.
#[derive(Debug, Default, Clone)]
pub struct LockedBuildResolutions(BTreeMap<(PackageName, Option<Version>), Resolution>);

impl LockedBuildResolutions {
    /// Create locked build resolutions from a map of (package name, optional version)
    /// to their pre-built resolutions.
    pub fn new(map: BTreeMap<(PackageName, Option<Version>), Resolution>) -> Self {
        Self(map)
    }

    /// Get the pre-built resolution for a given package name and version.
    ///
    /// First tries an exact (name, version) match, then falls back to a (name, None) match.
    pub fn get(&self, package: &PackageName, version: Option<&Version>) -> Option<&Resolution> {
        if let Some(version) = version {
            self.0
                .get(&(package.clone(), Some(version.clone())))
                .or_else(|| self.0.get(&(package.clone(), None)))
        } else {
            self.0.get(&(package.clone(), None))
        }
    }
}

/// Key for build dependency maps: (package name, optional version).
type BuildDepKey = (PackageName, Option<Version>);

/// Map of build dependency keys to their resolved build dependency (name, version) pairs.
type BuildPreferencesMap = BTreeMap<BuildDepKey, Vec<(PackageName, Version)>>;

/// Map of build dependency keys to their full resolutions.
type BuildResolutionsMap = BTreeMap<BuildDepKey, Resolution>;

/// Previously resolved build dependency versions, used as preferences during
/// subsequent resolutions to prefer the same versions.
///
/// Used during `uv lock` so the resolver prefers previously locked build dep
/// versions but can deviate if needed (e.g., when constraints change).
#[derive(Debug, Default, Clone)]
pub struct BuildPreferences(BuildPreferencesMap);

impl BuildPreferences {
    /// Create build preferences from a map of (package name, optional version) to their
    /// resolved build dependency (name, version) pairs.
    pub fn new(map: BuildPreferencesMap) -> Self {
        Self(map)
    }

    /// Get the build dependency preferences for a given package name and version.
    ///
    /// First tries an exact (name, version) match, then falls back to a (name, None) match.
    pub fn get(
        &self,
        package: &PackageName,
        version: Option<&Version>,
    ) -> Option<&Vec<(PackageName, Version)>> {
        if let Some(version) = version {
            self.0
                .get(&(package.clone(), Some(version.clone())))
                .or_else(|| self.0.get(&(package.clone(), None)))
        } else {
            self.0.get(&(package.clone(), None))
        }
    }
}

/// A collection of build dependency resolutions, keyed by (package name, optional version).
///
/// During the resolution process, when source distributions need to be built,
/// their build dependencies are resolved. This type captures those full resolutions
/// (including distribution URLs, hashes, etc.) so they can be persisted in the lock file
/// and reused directly during subsequent builds without re-resolving.
#[derive(Debug, Default, Clone)]
pub struct BuildResolutions(Arc<Mutex<BuildResolutionsMap>>);

impl BuildResolutions {
    /// Record a build resolution for the given package name and optional version.
    pub fn insert(&self, package: PackageName, version: Option<Version>, resolution: Resolution) {
        self.0
            .lock()
            .unwrap()
            .insert((package, version), resolution);
    }

    /// Get a snapshot of the current build resolutions.
    pub fn snapshot(&self) -> BuildResolutionsMap {
        self.0.lock().unwrap().clone()
    }
}

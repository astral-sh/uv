//! Reusable PEP 517 build environments.
//!
//! Reused environments have a stable, digest-addressed cache path, allowing native build backends
//! to retain incremental caches that include build-environment paths.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use fs_err as fs;
use tracing::debug;

use uv_cache::{Cache, CacheBucket, CacheEntry};
use uv_cache_key::{CacheKey, CacheKeyHasher, cache_digest};
use uv_configuration::{BuildKind, NoSources};
use uv_distribution_types::{ConfigSettings, IndexLocations, Requirement};
use uv_fs::{LockedFile, Simplified, write_atomic_sync};
use uv_normalize::PackageName;
use uv_python::{Interpreter, PythonEnvironment};

use crate::{BackendPath, Error, Pep517Backend};

/// The reusable build environment layout version.
const MARKER_VERSION: u8 = 1;

/// Cache key for a reusable PEP 517 build environment.
///
/// Uses declared build requirements to avoid resolution on cache hits. Includes every field from
/// [`uv_types::BuildKey`] to match in-process reuse.
pub(crate) struct BuildEnvironmentKey<'a> {
    pub(crate) base_python: &'a Path,
    pub(crate) python_full_version: &'a str,
    pub(crate) implementation_name: &'a str,
    pub(crate) source_root: &'a Path,
    pub(crate) subdirectory: Option<&'a Path>,
    pub(crate) no_sources: &'a NoSources,
    pub(crate) build_kind: BuildKind,
    pub(crate) pep517_backend: &'a Pep517Backend,
    pub(crate) extra_build_dependencies: &'a [Requirement],
    pub(crate) config_settings: &'a ConfigSettings,
    /// Ordered, so that the digest does not depend on hash map iteration order.
    pub(crate) environment_variables: BTreeMap<&'a OsStr, &'a OsStr>,
    pub(crate) index_locations: &'a IndexLocations,
}

impl CacheKey for BuildEnvironmentKey<'_> {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        MARKER_VERSION.cache_key(state);
        self.base_python.cache_key(state);
        self.python_full_version.cache_key(state);
        self.implementation_name.cache_key(state);
        self.source_root.cache_key(state);
        self.subdirectory.cache_key(state);

        // `NoSources` and `BuildKind` do not implement `CacheKey`.
        match self.no_sources {
            NoSources::None => 0u8.cache_key(state),
            NoSources::All => 1u8.cache_key(state),
            NoSources::Packages(packages) => {
                2u8.cache_key(state);
                for package in packages {
                    package.as_ref().cache_key(state);
                }
            }
        }
        self.build_kind.to_string().cache_key(state);

        self.pep517_backend.backend.cache_key(state);
        for path in self
            .pep517_backend
            .backend_path
            .iter()
            .flat_map(BackendPath::iter)
        {
            path.cache_key(state);
        }
        self.pep517_backend.requirements.cache_key(state);
        self.extra_build_dependencies.cache_key(state);
        self.config_settings.cache_key(state);

        for (key, value) in &self.environment_variables {
            key.to_string_lossy().cache_key(state);
            value.to_string_lossy().cache_key(state);
        }

        self.index_locations.no_index().cache_key(state);
        for index in self.index_locations.allowed_indexes() {
            index.url().url().cache_key(state);
        }
    }
}

/// Marker for a fully provisioned build environment.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct BuildEnvironmentMarker {
    version: u8,
    /// The uv version that provisioned the environment.
    uv_version: String,
    /// The installed distributions.
    requirements: Vec<String>,
}

/// A reusable PEP 517 build environment.
#[derive(Debug)]
pub(crate) struct BuildEnvironment {
    /// The package being built, used by `--refresh-package`.
    package_name: PackageName,
    /// The environment path.
    root: PathBuf,
    /// The completion marker, stored beside the environment because setup clears its directory.
    marker: CacheEntry,
    /// The provisioning lock, stored beside the environment because setup clears its directory.
    lock: CacheEntry,
}

impl BuildEnvironment {
    /// Locate the build environment for the given package and key.
    pub(crate) fn new(
        cache: &Cache,
        package_name: &PackageName,
        key: &BuildEnvironmentKey,
    ) -> Self {
        let digest = cache_digest(key);
        let shard = cache.shard(CacheBucket::BuildEnvironments, package_name.to_string());
        Self {
            package_name: package_name.clone(),
            root: shard.join(&digest),
            marker: shard.entry(format!("{digest}.json")),
            lock: shard.entry(format!("{digest}.lock")),
        }
    }

    /// Acquire the lock that serializes concurrent provisioning.
    pub(crate) async fn lock(&self) -> Option<LockedFile> {
        self.lock
            .lock()
            .await
            .inspect_err(|err| debug!("Failed to acquire build environment lock: {err}"))
            .ok()
    }

    /// Return the existing environment, if usable.
    pub(crate) fn reuse(&self, cache: &Cache) -> Option<PythonEnvironment> {
        let contents = match fs::read(self.marker.path()) {
            Ok(contents) => contents,
            Err(err) => {
                debug!("Failed to read build environment marker: {err}");
                return None;
            }
        };

        let marker: BuildEnvironmentMarker = match serde_json::from_slice(&contents) {
            Ok(marker) => marker,
            Err(err) => {
                debug!("Failed to parse build environment marker: {err}");
                return None;
            }
        };

        if marker.version != MARKER_VERSION {
            debug!(
                "Discarding build environment from an incompatible layout (v{})",
                marker.version
            );
            return None;
        }

        // Do not pass the source tree: local paths are refreshed on every build.
        match cache.freshness(&self.marker, Some(&self.package_name), None) {
            Ok(freshness) if freshness.is_fresh() => {}
            Ok(_) => {
                debug!("Discarding build environment because a refresh was requested");
                return None;
            }
            Err(err) => {
                debug!("Failed to check build environment freshness: {err}");
                return None;
            }
        }

        let environment = match PythonEnvironment::from_root(&self.root, cache) {
            Ok(environment) => environment,
            Err(err) => {
                debug!("Failed to load build environment: {err}");
                return None;
            }
        };

        debug!(
            "Reusing build environment at: `{}`, provisioned by uv {} ({})",
            self.root.user_display(),
            marker.uv_version,
            marker.requirements.join(", ")
        );
        Some(environment)
    }

    /// Create the environment, replacing whatever is currently at its path.
    pub(crate) fn create(&self, interpreter: &Interpreter) -> Result<PythonEnvironment, Error> {
        // Remove the marker before replacing the environment.
        match fs::remove_file(self.marker.path()) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(Error::Io(err)),
        }

        fs::create_dir_all(&self.root)?;

        debug!(
            "Creating build environment at: `{}`",
            self.root.user_display()
        );
        Ok(uv_virtualenv::create_venv(
            &self.root,
            interpreter.clone(),
            uv_virtualenv::Prompt::None,
            false,
            uv_virtualenv::OnExisting::Remove(uv_virtualenv::RemovalReason::StaleBuildEnvironment),
            // Native build backends need stable absolute paths.
            false,
            uv_virtualenv::Seed::Disabled,
            false,
        )?)
    }

    /// Mark the fully provisioned environment as reusable.
    pub(crate) fn commit(&self, requirements: &[String]) -> Result<(), Error> {
        let marker = BuildEnvironmentMarker {
            version: MARKER_VERSION,
            uv_version: uv_version::version().to_string(),
            requirements: requirements.to_vec(),
        };
        let contents = serde_json::to_vec(&marker).map_err(Error::BuildEnvironmentMarker)?;
        write_atomic_sync(self.marker.path(), contents)?;
        Ok(())
    }
}

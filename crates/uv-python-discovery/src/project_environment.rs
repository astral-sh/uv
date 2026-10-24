use std::borrow::Cow;
use std::fmt::Arguments;
use std::io;
use std::path::{Path, PathBuf};

use uv_cache::{Cache, CacheBucket, CacheEntry};
use uv_cache_key::{cache_digest, cache_name};
use uv_python_interpreter::{Interpreter, virtualenv_python_executable};
use uv_python_managed::{PythonMinorVersionLink, UpgradePolicy};
use uv_python_types::{LenientImplementationName, PythonSource};
use uv_workspace::Workspace;

use crate::discovery::{Error as DiscoveryError, python_installation_from_executable};
use crate::installation::PythonInstallation;

/// A centralized project environment to consider when discovering Python.
pub struct ProjectEnvironmentDiscovery<'a> {
    pub(crate) cache: ProjectEnvironmentCache<'a>,
    pub(crate) start: &'a Path,
    pub(crate) workspace_root: &'a Path,
}

impl<'a> ProjectEnvironmentDiscovery<'a> {
    /// Discover the workspace's environment while searching upward from `start`.
    pub fn new(workspace: &'a Workspace, start: &'a Path, cache: &'a Cache) -> Self {
        Self {
            cache: ProjectEnvironmentCache::new(
                workspace.install_path(),
                workspace
                    .pyproject_toml()
                    .project
                    .as_ref()
                    .map(|project| project.name.as_ref()),
                cache,
            ),
            start,
            workspace_root: workspace.install_path(),
        }
    }
}

/// The cache entries for a project's centralized environments.
pub struct ProjectEnvironmentCache<'a> {
    cache: &'a Cache,
    workspace_path: PathBuf,
    name: Option<String>,
}

impl<'a> ProjectEnvironmentCache<'a> {
    /// Identify the cached environments for a project root.
    pub fn new(install_path: &Path, project_name: Option<&str>, cache: &'a Cache) -> Self {
        let workspace_path =
            fs_err::canonicalize(install_path).unwrap_or_else(|_| install_path.to_path_buf());
        let name = project_name
            .and_then(|name| cache_name(name, Some(100)))
            .or_else(|| {
                workspace_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| cache_name(name, Some(100)))
            })
            .map(Cow::into_owned);
        Self {
            cache,
            workspace_path,
            name,
        }
    }

    /// Return the selected cache directory and its interpreter, ignoring an unusable interpreter.
    pub fn discover_current(
        &self,
    ) -> Result<Option<(PathBuf, PythonInstallation)>, DiscoveryError> {
        let Ok(root) = self.resolve_current() else {
            return Ok(None);
        };
        match python_installation_from_executable(
            PythonSource::DiscoveredEnvironment,
            virtualenv_python_executable(&root),
            self.cache,
        ) {
            Ok(installation) => Ok(Some((root, installation))),
            Err(err) if !err.is_critical() => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// The interpreter-independent reference to the selected project environment.
    pub fn current(&self) -> CacheEntry {
        // A path can first name a script and later a workspace; keep this key distinct from the
        // script's path-only cache key.
        let digest = cache_digest(&("current", &self.workspace_path));
        self.entry(format_args!("current-{digest}"))
    }

    /// Record the selected project environment.
    ///
    /// On Unix, the reference is a relative symlink. On Windows, it is a file containing the
    /// sibling's name so updating the reference does not require a junction or symlink privileges.
    pub fn set_current(&self, target: &Path) -> io::Result<()> {
        let current = self.current();
        let Some((parent, name)) = target.parent().zip(target.file_name()) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "The link target must be a sibling cache directory",
            ));
        };
        if parent != current.dir() && !same_file::is_same_file(parent, current.dir())? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "The link target must be a sibling cache directory",
            ));
        }
        if Self::read_current(&current).is_ok_and(|existing| existing.as_os_str() == name) {
            return Ok(());
        }

        #[cfg(unix)]
        {
            uv_fs::replace_symlink(name, current.path())
        }
        #[cfg(windows)]
        {
            let name = name.to_str().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Cache entry name is not UTF-8")
            })?;
            uv_fs::write_atomic_sync(current.path(), name)
        }
    }

    /// Resolve the current project environment without following another link.
    pub fn resolve_current(&self) -> io::Result<PathBuf> {
        let current = self.current();
        let name = Self::read_current(&current)?;
        if name.file_name() != Some(name.as_os_str()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The link target is not a sibling cache directory",
            ));
        }
        let target = current.dir().join(name);
        if !fs_err::symlink_metadata(&target)?.is_dir()
            || !uv_fs::is_virtualenv_base(&target)
            || !virtualenv_python_executable(&target).is_file()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The link target is not a project environment",
            ));
        }
        Ok(target)
    }

    fn read_current(current: &CacheEntry) -> io::Result<PathBuf> {
        #[cfg(unix)]
        {
            fs_err::read_link(current.path())
        }
        #[cfg(windows)]
        {
            fs_err::read_to_string(current.path()).map(PathBuf::from)
        }
    }

    /// The environment for the given interpreter.
    pub fn for_interpreter(
        &self,
        interpreter: &Interpreter,
        upgrade_policy: UpgradePolicy,
    ) -> CacheEntry {
        let interpreter_key = interpreter.key();
        // Use the workspace path to isolate projects and the interpreter key to maximize
        // intra-project environment re-use while avoiding clashes with incompatible environments.
        // Ignoring the patch version allows upgradeable managed environments to be re-used after
        // an upgrade.
        let (digest, python_version) = if let Some(link) =
            PythonMinorVersionLink::from_interpreter(interpreter, upgrade_policy)
        {
            (
                cache_digest(&(&self.workspace_path, link.key())),
                interpreter.python_minor_version(),
            )
        } else {
            (
                cache_digest(&(&self.workspace_path, &interpreter_key)),
                interpreter.python_version().clone(),
            )
        };
        let implementation = interpreter_key.implementation();
        let implementation = match implementation.as_ref() {
            LenientImplementationName::Known(implementation) => implementation
                .short_name()
                .unwrap_or_else(|| implementation.long_name()),
            LenientImplementationName::Unknown(implementation) => implementation,
        };
        self.entry(format_args!("{implementation}{python_version}-{digest}"))
    }

    fn entry(&self, suffix: Arguments<'_>) -> CacheEntry {
        // A virtual workspace can be nameless if its directory has no cache-safe characters.
        let name = self
            .name
            .as_ref()
            .map_or_else(|| suffix.to_string(), |name| format!("{name}-{suffix}"));
        CacheEntry::new(self.cache.bucket(CacheBucket::Environments), name)
    }
}

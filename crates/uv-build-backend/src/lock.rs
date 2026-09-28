use std::io;
use std::path::{Path, PathBuf};

use uv_cache::Cache;
use uv_configuration::{DependencyGroupsWithDefaults, ExtrasSpecification, InstallOptions};
use uv_lock::{Installable, Lock, PylockToml};
use uv_normalize::{DefaultExtras, PackageName};
use uv_pep440::release_specifiers_to_ranges;
use uv_preview::PreviewFeature;
use uv_static::{EnvVars, parse_boolish_environment_variable};
use uv_workspace::{
    DiscoveryOptions, MemberDiscovery, ProjectWorkspace, WorkspaceCache, WorkspaceErrorKind,
};
use version_ranges::Ranges;

use crate::{Error, PyProjectToml};

pub(crate) struct ExportedLock {
    pub(crate) path: PathBuf,
    pub(crate) pylock: String,
}

/// Export runtime dependencies without the project itself, which is supplied by the wheel.
pub(crate) fn export_lock(
    source_tree: &Path,
    pyproject: &PyProjectToml,
) -> Result<Option<ExportedLock>, Error> {
    let enabled = parse_boolish_environment_variable(EnvVars::UV_BUILD_BACKEND_EXPORT_LOCK)?
        .or_else(|| {
            pyproject
                .settings()
                .and_then(|settings| settings.export_lock)
        });
    if enabled == Some(false) {
        return Ok(None);
    }
    if !uv_preview::is_enabled(PreviewFeature::LockedTools) {
        if enabled == Some(true) {
            return Err(Error::InvalidBuildLock(
                "Exporting locks requires the `locked-tools` preview feature".to_string(),
            ));
        }
        return Ok(None);
    }

    let root = discover_workspace_root(source_tree)?;
    let path = root.join("uv.lock");
    let contents = match fs_err::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == io::ErrorKind::NotFound && enabled.is_none() => return Ok(None),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Err(Error::InvalidBuildLock(
                "Cannot export a lock: `uv.lock` was not found; run `uv lock` before building"
                    .to_string(),
            ));
        }
        Err(err) => return Err(err.into()),
    };
    let lock = Lock::from_toml(&contents)?;
    let package = lock
        .find_by_name(pyproject.name())
        .map_err(Error::InvalidBuildLock)?
        .ok_or_else(|| {
            Error::InvalidBuildLock(format!("`uv.lock` does not contain `{}`", pyproject.name()))
        })?;
    if package.version() != Some(pyproject.version()) {
        return Err(Error::InvalidBuildLock(format!(
            "`uv.lock` does not match version {} of `{}`; run `uv lock` before building",
            pyproject.version(),
            pyproject.name()
        )));
    }

    let target = BuildLock {
        root: &root,
        lock: &lock,
        name: pyproject.name(),
    };
    let install_options =
        InstallOptions::new(true, false, false, false, false, false, vec![], vec![]);
    let extras = ExtrasSpecification::default().with_defaults(DefaultExtras::default());
    let groups = DependencyGroupsWithDefaults::none();
    let workspace_dependency =
        PylockToml::workspace_dependency(&target, &extras, &groups, &install_options)?;
    if let Some(dependency) = workspace_dependency {
        return Err(Error::InvalidBuildLock(format!(
            "Cannot export a lock with workspace dependency `{dependency}`"
        )));
    }
    if !lock.members().is_empty() {
        let project_python = pyproject
            .requires_python()
            .cloned()
            .map(release_specifiers_to_ranges)
            .unwrap_or_else(Ranges::full);
        let locked_python =
            release_specifiers_to_ranges(lock.requires_python().specifiers().clone());
        if !project_python.subset_of(&locked_python) {
            if enabled.is_none() {
                return Ok(None);
            }
            return Err(Error::InvalidBuildLock(format!(
                "Cannot export a workspace lock that does not cover the Python versions supported by `{}`",
                pyproject.name()
            )));
        }
    }
    if enabled.is_none() && !lock.has_only_pypi_and_workspace_sources() {
        return Ok(None);
    }
    let pylock = PylockToml::from_lock(
        &target,
        &root,
        &[],
        &extras,
        &groups,
        false,
        None,
        &install_options,
    )?;
    if pylock.has_missing_hashes() {
        return Err(Error::InvalidBuildLock(
            "Cannot export a lock with missing artifact hashes; regenerate `uv.lock` with artifact hashes before building"
                .to_string(),
        ));
    }
    if pylock.has_relative_paths() {
        return Err(Error::InvalidBuildLock(
            "Cannot export a lock with relative dependency paths; use remote sources or absolute paths before building"
                .to_string(),
        ));
    }
    Ok(Some(ExportedLock {
        path,
        pylock: pylock.to_toml()?,
    }))
}

/// Workspace discovery is async, but PEP 517 build hooks are synchronous and can run on a Tokio
/// runtime thread. Use a separate thread so discovery also works in that context.
fn discover_workspace_root(source_tree: &Path) -> Result<PathBuf, Error> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("uv-build-workspace".to_string())
            .spawn_scoped(scope, || {
                let source_tree = std::path::absolute(source_tree)?;
                let source_tree = uv_fs::normalize_path(&source_tree).into_owned();
                let cache_path = std::env::var_os(EnvVars::UV_CACHE_DIR)
                    .map(PathBuf::from)
                    .or_else(|| uv_dirs::legacy_user_cache_dir().filter(|path| path.exists()))
                    .or_else(|| {
                        uv_dirs::user_cache_dir().map(|path| {
                            if cfg!(windows) {
                                path.join("cache")
                            } else {
                                path
                            }
                        })
                    })
                    .unwrap_or_else(|| PathBuf::from(".uv_cache"));
                let cache = Cache::from_path(cache_path);
                // A source distribution carries its own lock and must not pick up a workspace
                // from the directory into which it was extracted.
                let source_dist = source_tree.join("PKG-INFO").is_file()
                    && source_tree.join("pyproject.toml.orig").is_file();
                let options = DiscoveryOptions {
                    members: MemberDiscovery::None,
                    stop_discovery_at: source_dist.then_some(source_tree.clone()),
                };
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                match runtime.block_on(ProjectWorkspace::discover(
                    &source_tree,
                    &options,
                    &cache,
                    &WorkspaceCache::default(),
                )) {
                    Ok(workspace) => Ok(workspace.workspace().install_path().clone()),
                    Err(error) if matches!(error.as_ref(), WorkspaceErrorKind::NonWorkspace(_)) => {
                        Ok(source_tree)
                    }
                    Err(error) => Err(error.into()),
                }
            })?
            .join()
            .map_err(|_| io::Error::other("workspace discovery thread panicked"))?
    })
}

struct BuildLock<'lock> {
    root: &'lock Path,
    lock: &'lock Lock,
    name: &'lock PackageName,
}

impl<'lock> Installable<'lock> for BuildLock<'lock> {
    fn install_path(&self) -> &'lock Path {
        self.root
    }

    fn lock(&self) -> &'lock Lock {
        self.lock
    }

    fn roots(&self) -> impl Iterator<Item = &PackageName> {
        std::iter::once(self.name)
    }

    fn project_name(&self) -> Option<&PackageName> {
        Some(self.name)
    }
}

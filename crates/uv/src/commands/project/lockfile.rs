use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use uv_configuration::{DependencyGroups, DependencyGroupsWithDefaults};
use uv_lock::{Lock, Package};
use uv_normalize::PackageName;
use uv_preview::{Preview, PreviewFeature};
use uv_warnings::warn_user;
use uv_workspace::pyproject::PyProjectToml;

/// A frozen workspace resolution and the root used to interpret its relative paths.
#[derive(Debug, Clone)]
pub(crate) struct FrozenWorkspace {
    root: PathBuf,
    lock: Lock,
}

impl FrozenWorkspace {
    /// Discover a frozen lockfile when its workspace manifest is missing.
    ///
    /// A remaining member manifest is identified by its path in the lockfile. Other nested manifests
    /// take precedence, so an unrelated project cannot use an ancestor's lockfile.
    pub(crate) async fn discover(project_dir: &Path, preview: Preview) -> Result<Option<Self>> {
        let absolute = std::path::absolute(project_dir)?;
        let project_dir = uv_fs::normalize_path(&absolute);
        let mut manifest_root = None;
        for directory in project_dir.ancestors() {
            let pyproject_path = directory.join("pyproject.toml");
            if pyproject_path.is_file() {
                // Like manifest discovery, stop at the first manifest above the current project.
                if manifest_root.is_some() {
                    return Ok(None);
                }
                // An explicit workspace root is a discovery boundary, even without a lockfile.
                // Let ordinary discovery report invalid manifests and missing workspace locks.
                let contents = fs_err::tokio::read_to_string(&pyproject_path).await?;
                let Ok(pyproject) = PyProjectToml::from_string(contents, &pyproject_path) else {
                    return Ok(None);
                };
                if pyproject.is_workspace_root() {
                    return Ok(None);
                }
                manifest_root = Some(directory);
            }
            let path = directory.join("uv.lock");
            if path.is_file() {
                if directory.join("pyproject.toml").is_file() {
                    return Ok(None);
                }

                let workspace = match Self::read(&path).await {
                    Ok(workspace) => workspace,
                    Err(_) if manifest_root.is_some() => return Ok(None),
                    Err(error) => return Err(error),
                };
                if let Some(manifest_root) = manifest_root {
                    let manifest_root = normalize_member_path(manifest_root);
                    if !workspace
                        .member_paths()
                        .any(|(_, path)| path == manifest_root)
                    {
                        return Ok(None);
                    }
                }
                if workspace.lock.configured_member_default_groups().is_none() {
                    bail!(
                        "Frozen lockfile discovery requires a lockfile with revision 5 or later; run `uv lock` to update it"
                    );
                }
                if !preview.is_enabled(PreviewFeature::FrozenLockfile) {
                    warn_user!(
                        "Using `uv.lock` without a `pyproject.toml` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
                        PreviewFeature::FrozenLockfile
                    );
                }
                return Ok(Some(workspace));
            }
        }
        Ok(None)
    }

    /// Read a lockfile, using its parent as the base for relative sources.
    async fn read(path: &Path) -> Result<Self> {
        let absolute = std::path::absolute(path)?;
        let path = uv_fs::normalize_path(&absolute);
        let root = path
            .parent()
            .context("The lockfile path has no parent directory")?
            .to_path_buf();
        let contents = fs_err::tokio::read_to_string(&path)
            .await
            .with_context(|| format!("Failed to read lockfile `{}`", path.display()))?;
        let lock = Lock::from_toml(&contents)
            .with_context(|| format!("Failed to parse lockfile `{}`", path.display()))?;
        Ok(Self { root, lock })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn lock(&self) -> &Lock {
        &self.lock
    }

    /// Select the nearest workspace member, falling back to the root project.
    pub(crate) fn current_project(&self, project_dir: &Path) -> Option<&PackageName> {
        let project_dir = normalize_member_path(project_dir);
        self.member_paths()
            .filter(|(_, path)| project_dir.starts_with(path))
            .max_by_key(|(_, path)| path.components().count())
            .map(|(name, _)| name)
            .or_else(|| self.lock.root().map(Package::name))
    }

    /// Return workspace members with absolute paths for discovery and project selection.
    fn member_paths(&self) -> impl Iterator<Item = (&PackageName, PathBuf)> {
        self.lock.workspace_member_paths().map(|(name, path)| {
            let path = uv_fs::normalize_path(self.root.join(path));
            (name, normalize_member_path(&path))
        })
    }

    /// Resolve groups using the selected member's or non-project root's recorded defaults.
    pub(crate) fn resolve_groups(
        &self,
        groups: &DependencyGroups,
        project: Option<&PackageName>,
    ) -> Result<DependencyGroupsWithDefaults> {
        let defaults = match project {
            Some(name) => self.lock.member_default_groups(name),
            None => self.lock.workspace_default_groups(),
        }
        .context("The lockfile does not record default dependency groups")?;
        Ok(groups.with_defaults(defaults))
    }

    /// Validate the selected packages against the workspace recorded in the lockfile.
    pub(crate) fn validate_packages(&self, names: &[PackageName]) -> Result<()> {
        for name in names {
            if !(self.lock.members().contains(name)
                || self.lock.members().is_empty()
                    && self.lock.root().is_some_and(|root| root.name() == name))
            {
                bail!("Package `{name}` not found in lockfile workspace");
            }
        }
        Ok(())
    }
}

/// Resolve symlinks for member comparisons, allowing missing directories in frozen workspaces.
fn normalize_member_path(path: &Path) -> PathBuf {
    fs_err::canonicalize(path).unwrap_or_else(|_| uv_fs::normalize_path(path).into_owned())
}

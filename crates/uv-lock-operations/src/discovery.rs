use std::path::Path;

use anyhow::Result;

use uv_cache::Cache;
use uv_normalize::PackageName;
use uv_preview::Preview;
use uv_settings::FrozenSource;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache};

use crate::FrozenWorkspace;

/// A project discovered from its manifests or a frozen workspace lockfile.
#[derive(Debug)]
pub enum DiscoveredProject {
    Manifest(VirtualProject),
    Lockfile(Box<FrozenWorkspace>),
}

impl DiscoveredProject {
    /// Discover a project, allowing frozen operations to use a workspace without its root manifest.
    ///
    /// The package selection and discovery options apply to manifest discovery. Lockfile discovery
    /// retains the recorded workspace so callers can select packages without their manifests.
    pub async fn discover(
        project_dir: &Path,
        options: &DiscoveryOptions,
        package: Option<&PackageName>,
        frozen: Option<FrozenSource>,
        preview: Preview,
        cache: &Cache,
        workspace_cache: &WorkspaceCache,
    ) -> Result<Self> {
        if frozen.is_some()
            && let Some(workspace) = FrozenWorkspace::discover(project_dir, preview).await?
        {
            return Ok(Self::Lockfile(Box::new(workspace)));
        }

        let project = if let Some(package) = package {
            VirtualProject::discover_with_package(
                project_dir,
                options,
                cache,
                workspace_cache,
                package.clone(),
            )
            .await?
        } else {
            VirtualProject::discover(project_dir, options, cache, workspace_cache).await?
        };
        Ok(Self::Manifest(project))
    }
}

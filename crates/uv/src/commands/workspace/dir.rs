use std::path::Path;

use anyhow::{Result, bail};

use owo_colors::OwoColorize;
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_preview::{Preview, PreviewFeatures};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

use crate::commands::ExitStatus;

/// Print the path to the workspace dir
pub(crate) async fn dir(
    package_name: Option<PackageName>,
    project_dir: &Path,
    preview: Preview,
) -> Result<ExitStatus> {
    if preview.is_enabled(PreviewFeatures::WORKSPACE_METADATA) {
        warn_user!(
            "The `uv workspace dir` command is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeatures::WORKSPACE_METADATA
        );
    }

    let workspace_cache = WorkspaceCache::default();
    let workspace =
        Workspace::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache).await?;

    let dir: &Path = match package_name {
        None => workspace.install_path().as_path(),
        Some(package) => {
            if let Some(p) = workspace.packages().get(&package) {
                p.root().as_path()
            } else {
                bail!("Package `{package}` not found in workspace.")
            }
        }
    };

    println!("{}", dir.simplified_display().cyan());

    Ok(ExitStatus::Success)
}

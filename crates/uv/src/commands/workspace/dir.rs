use std::fmt::Write;
use std::path::Path;

use anyhow::{Result, bail};

use owo_colors::OwoColorize;
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Print the path to the workspace dir
pub(crate) async fn dir(
    package_name: Option<PackageName>,
    project_dir: &Path,
    printer: Printer,
) -> Result<ExitStatus> {
    let workspace_cache = WorkspaceCache::default();
    let workspace =
        Workspace::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache).await?;

    let dir = match package_name {
        None => workspace.install_path(),
        Some(package) => {
            if let Some(p) = workspace.packages().get(&package) {
                p.root()
            } else {
                bail!("Package `{package}` not found in workspace.")
            }
        }
    };

    writeln!(printer.stdout(), "{}", dir.simplified_display().cyan())?;

    Ok(ExitStatus::Success)
}

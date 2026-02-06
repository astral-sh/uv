use std::fmt::Write;
use std::path::Path;

use anyhow::Result;

use owo_colors::OwoColorize;
use uv_fs::Simplified;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// List workspace members
pub(crate) async fn list(project_dir: &Path, paths: bool, printer: Printer) -> Result<ExitStatus> {
    let workspace_cache = WorkspaceCache::default();
    let workspace =
        Workspace::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache).await?;

    for (name, member) in workspace.packages() {
        if paths {
            writeln!(
                printer.stdout(),
                "{}",
                member.root().simplified_display().cyan()
            )?;
        } else {
            writeln!(printer.stdout(), "{}", name.cyan())?;
        }
    }

    Ok(ExitStatus::Success)
}

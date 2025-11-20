use std::fmt::Write;
use std::path::Path;

use anyhow::Result;

use owo_colors::OwoColorize;
use uv_fs::Simplified;
use uv_preview::{Preview, PreviewFeatures};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// List workspace members
pub(crate) async fn list(
    project_dir: &Path,
    paths: bool,
    preview: Preview,
    printer: Printer,
) -> Result<ExitStatus> {
    if !preview.is_enabled(PreviewFeatures::WORKSPACE_LIST) {
        warn_user!(
            "The `uv workspace list` command is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeatures::WORKSPACE_LIST
        );
    }

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

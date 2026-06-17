use std::fmt::Write;
use std::path::Path;

use anyhow::{Context, Result, bail};

use owo_colors::OwoColorize;
use uv_cache::Cache;
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

use crate::commands::ExitStatus;
use crate::commands::project::lock_target::LockTarget;
use crate::printer::Printer;

/// List workspace members
pub(crate) async fn list(
    project_dir: &Path,
    paths: bool,
    depends_on: Option<PackageName>,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
) -> Result<ExitStatus> {
    let workspace = Workspace::discover(
        project_dir,
        &DiscoveryOptions::default(),
        cache,
        workspace_cache,
    )
    .await?;

    let dependents = if let Some(target) = depends_on {
        let lock_path = workspace.install_path().join("uv.lock");
        let lock = LockTarget::from(workspace.as_ref())
            .read()
            .await?
            .with_context(|| {
                format!(
                    "No `{}` found; run `uv lock` to create it",
                    lock_path.simplified_display()
                )
            })?;
        let Some(dependents) = lock.workspace_members_depending_on(&target) else {
            bail!(
                "Package `{target}` was not found in `{}`",
                lock_path.simplified_display()
            );
        };
        Some(dependents)
    } else {
        None
    };

    for (name, member) in workspace.packages() {
        if dependents
            .as_ref()
            .is_some_and(|dependents| !dependents.contains(name))
        {
            continue;
        }
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

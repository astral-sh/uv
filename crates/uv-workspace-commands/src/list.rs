use std::fmt::Write;
use std::path::Path;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;

use uv_cache::Cache;
use uv_command_support::{ExitStatus, Printer};
use uv_fs::Simplified;
use uv_preview::{Preview, PreviewFeature};
use uv_scripts::{ScriptDiscoveryError, find_scripts};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

/// List workspace members or PEP 723 scripts.
pub async fn list(
    project_dir: &Path,
    paths: bool,
    scripts: bool,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    if scripts && !preview.is_enabled(PreviewFeature::WorkspaceListScripts) {
        warn_user!(
            "The `--scripts` option is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::WorkspaceListScripts
        );
    }

    let workspace = Workspace::discover(
        project_dir,
        &DiscoveryOptions::default(),
        cache,
        workspace_cache,
    )
    .await?;

    if scripts {
        let mut scripts = find_scripts(workspace.install_path(), cache.root())
            .filter_map(|script| match script {
                Ok(script) => Some(Ok(script)),
                Err(ScriptDiscoveryError::Parse { path, source }) => {
                    warn_user!(
                        "Skipping invalid PEP 723 script `{}`: {source}",
                        path.simplified_display()
                    );
                    None
                }
                Err(
                    error @ (ScriptDiscoveryError::Walk(_) | ScriptDiscoveryError::Read { .. }),
                ) => Some(Err(error)),
            })
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| {
                format!(
                    "Failed to discover PEP 723 scripts under workspace root `{}`",
                    workspace.install_path().simplified_display()
                )
            })?;
        scripts.sort_unstable();
        for script in scripts {
            let script = script
                .strip_prefix(workspace.install_path())
                .context("PEP 723 script was discovered outside the workspace root")?;
            writeln!(printer.stdout(), "{}", script.simplified_display().cyan())?;
        }
        return Ok(ExitStatus::Success);
    }

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

use std::fmt::Write;
use std::path::Path;

use anyhow::{Result, bail};

use uv_cli::DirArgs;
use uv_fs::{PortablePathBuf};
use uv_preview::{Preview, PreviewFeatures};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Print the path to the workspace dir
pub(crate) async fn dir(
    args: &DirArgs,
    project_dir: &Path,
    preview: Preview,
    printer: Printer,
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

    let dir = match &args.package {
        None => {
            PortablePathBuf::from(workspace.install_path().as_path()).to_string()
        },
        Some(package) => {
            if let Some(p) = workspace.packages().get(&package) {
                PortablePathBuf::from(p.root().as_path()).to_string()
            } else {
                bail!("Package {} does not exist.", package)
            }
        }
    };

    writeln!(
        printer.stdout(),
        "{}",
        dir
    )?;

    Ok(ExitStatus::Success)
}

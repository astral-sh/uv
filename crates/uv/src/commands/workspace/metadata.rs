use std::fmt::Write;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use uv_fs::PortablePathBuf;
use uv_normalize::PackageName;
use uv_preview::{Preview, PreviewFeatures};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// The schema version for the metadata report.
#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "snake_case")]
enum SchemaVersion {
    /// An unstable, experimental schema.
    #[default]
    Preview,
}

/// The schema metadata for the metadata report.
#[derive(Serialize, Debug, Default)]
struct SchemaReport {
    /// The version of the schema.
    version: SchemaVersion,
}

/// Report for a single workspace member.
#[derive(Serialize, Debug)]
struct WorkspaceMemberReport {
    /// The name of the workspace member.
    name: PackageName,
    /// The path to the workspace member's root directory.
    path: PortablePathBuf,
}

/// The report for a metadata operation.
#[derive(Serialize, Debug)]
struct MetadataReport {
    /// The schema of this report.
    schema: SchemaReport,
    /// The workspace root directory.
    workspace_root: PortablePathBuf,
    /// The workspace members.
    members: Vec<WorkspaceMemberReport>,
}

/// Display metadata about the workspace.
pub(crate) async fn metadata(
    project_dir: &Path,
    preview: Preview,
    printer: Printer,
) -> Result<ExitStatus> {
    if !preview.is_enabled(PreviewFeatures::WORKSPACE_METADATA) {
        warn_user!(
            "The `uv workspace metadata` command is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeatures::WORKSPACE_METADATA
        );
    }

    let workspace_cache = WorkspaceCache::default();
    let workspace =
        Workspace::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache).await?;

    let members = workspace
        .packages()
        .values()
        .map(|package| WorkspaceMemberReport {
            name: package.project().name.clone(),
            path: PortablePathBuf::from(package.root().as_path()),
        })
        .collect();

    let report = MetadataReport {
        schema: SchemaReport::default(),
        workspace_root: PortablePathBuf::from(workspace.install_path().as_path()),
        members,
    };

    writeln!(
        printer.stdout(),
        "{}",
        serde_json::to_string_pretty(&report)?
    )?;

    Ok(ExitStatus::Success)
}

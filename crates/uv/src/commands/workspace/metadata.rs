use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Result;
use serde::Serialize;

use uv_fs::PortablePathBuf;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_preview::{Preview, PreviewFeatures};
use uv_pypi_types::VerbatimParsedUrl;
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

/// A dependency of a workspace member.
#[derive(Serialize, Debug)]
struct DependencyReport {
    /// The name of the dependency.
    name: PackageName,
    /// Whether this dependency is another workspace member.
    workspace: bool,
    /// The extra that requires this dependency, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    extra: Option<ExtraName>,
    /// The dependency group that requires this dependency, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<GroupName>,
}

/// Report for a single workspace member.
#[derive(Serialize, Debug)]
struct WorkspaceMemberReport {
    /// The name of the workspace member.
    name: PackageName,
    /// The path to the workspace member's root directory.
    path: PathBuf,
    /// All dependencies of this workspace member.
    dependencies: Vec<DependencyReport>,
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

/// Extract all dependencies from a workspace member.
///
/// This function examines the member's regular dependencies, optional dependencies (extras),
/// and dependency groups, marking which dependencies are workspace members.
fn extract_dependencies(
    package: &uv_workspace::WorkspaceMember,
    workspace_names: &BTreeSet<PackageName>,
) -> Vec<DependencyReport> {
    let mut dependencies = Vec::new();

    // Extract regular dependencies
    if let Some(deps) = package.project().dependencies.as_ref() {
        for dep in deps {
            if let Ok(req) = uv_pep508::Requirement::<VerbatimParsedUrl>::from_str(dep) {
                dependencies.push(DependencyReport {
                    name: req.name.clone(),
                    workspace: workspace_names.contains(&req.name),
                    extra: None,
                    group: None,
                });
            }
        }
    }

    // Extract optional dependencies (extras)
    if let Some(optional_dependencies) = package.project().optional_dependencies.as_ref() {
        for (extra_name, deps) in optional_dependencies {
            for dep in deps {
                if let Ok(req) = uv_pep508::Requirement::<VerbatimParsedUrl>::from_str(dep) {
                    dependencies.push(DependencyReport {
                        name: req.name.clone(),
                        workspace: workspace_names.contains(&req.name),
                        extra: Some(extra_name.clone()),
                        group: None,
                    });
                }
            }
        }
    }

    // Extract dependency groups
    if let Some(dependency_groups) = package.pyproject_toml().dependency_groups.as_ref() {
        for (group_name, deps) in dependency_groups {
            for dep_spec in deps {
                // Only process Requirement variants, skip IncludeGroup
                if let uv_pypi_types::DependencyGroupSpecifier::Requirement(dep) = dep_spec {
                    if let Ok(req) = uv_pep508::Requirement::<VerbatimParsedUrl>::from_str(dep) {
                        dependencies.push(DependencyReport {
                            name: req.name.clone(),
                            workspace: workspace_names.contains(&req.name),
                            extra: None,
                            group: Some(group_name.clone()),
                        });
                    }
                }
            }
        }
    }

    dependencies
}

/// Display package metadata.
pub(crate) async fn metadata(
    project_dir: &Path,
    preview: Preview,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_enabled(PreviewFeatures::WORKSPACE_METADATA) {
        warn_user!(
            "The `uv workspace metadata` command is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeatures::WORKSPACE_METADATA
        );
    }

    let workspace_cache = WorkspaceCache::default();
    let workspace =
        Workspace::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache).await?;

    // Collect all workspace member names for filtering
    let workspace_names: BTreeSet<PackageName> = workspace
        .packages()
        .values()
        .map(|package| package.project().name.clone())
        .collect();

    let members = workspace
        .packages()
        .values()
        .map(|package| WorkspaceMemberReport {
            name: package.project().name.clone(),
            path: package.root().clone(),
            dependencies: extract_dependencies(package, &workspace_names),
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

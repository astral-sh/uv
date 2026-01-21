use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use tokio::process::Command;

use uv_bin_install::{
    BinVersion, Binary, bin_install, bin_install_resolved, find_matching_version,
};
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_preview::{Preview, PreviewFeatures};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache, WorkspaceError};

use crate::child::run_to_completion;
use crate::commands::ExitStatus;
use crate::commands::reporters::BinaryDownloadReporter;
use crate::printer::Printer;

/// Run the formatter.
pub(crate) async fn format(
    project_dir: &Path,
    check: bool,
    diff: bool,
    extra_args: Vec<String>,
    version: Option<String>,
    exclude_newer: Option<jiff::Timestamp>,
    client_builder: BaseClientBuilder<'_>,
    cache: Cache,
    printer: Printer,
    preview: Preview,
    no_project: bool,
) -> Result<ExitStatus> {
    // Check if the format feature is in preview
    if !preview.is_enabled(PreviewFeatures::FORMAT) {
        warn_user!(
            "`uv format` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeatures::FORMAT
        );
    }

    let workspace_cache = WorkspaceCache::default();
    // If `no_project` is provided, we use the provided directory
    // Otherwise, we discover the project and use the project root.
    let target_dir = if no_project {
        project_dir.to_owned()
    } else {
        match VirtualProject::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache)
            .await
        {
            // If we found a project, we use the project root
            Ok(proj) => proj.root().to_owned(),
            // If there is a problem finding a project, we just use the provided directory,
            // e.g., for unmanaged projects
            Err(
                WorkspaceError::MissingPyprojectToml
                | WorkspaceError::MissingProject(_)
                | WorkspaceError::NonWorkspace(_),
            ) => project_dir.to_owned(),
            Err(err) => return Err(err.into()),
        }
    };

    let retry_policy = client_builder.retry_policy();
    // Python downloads are performing their own retries to catch stream errors, disable the
    // default retries to avoid the middleware from performing uncontrolled retries.
    let client = client_builder.retries(0).build();

    // Determine the version to use and get the path to Ruff.
    let reporter = BinaryDownloadReporter::single(printer);
    let bin_version = version
        .as_deref()
        .map(BinVersion::from_str)
        .transpose()?
        .unwrap_or(BinVersion::Default);

    let ruff_path = match bin_version {
        BinVersion::Default => {
            // Use the default pinned version
            let version = Binary::Ruff.default_version();
            bin_install(
                Binary::Ruff,
                &version,
                &client,
                &retry_policy,
                &cache,
                &reporter,
            )
            .await
            .with_context(|| format!("Failed to install ruff {version}"))?
        }
        BinVersion::Pinned(version) => {
            // Use the exact version directly without manifest lookup.
            // Note: `exclude_newer` is not respected for pinned versions.
            bin_install(
                Binary::Ruff,
                &version,
                &client,
                &retry_policy,
                &cache,
                &reporter,
            )
            .await
            .with_context(|| format!("Failed to install ruff {version}"))?
        }
        BinVersion::Latest => {
            // Fetch the latest version from the manifest
            let resolved = find_matching_version(Binary::Ruff, None, exclude_newer, &client)
                .await
                .with_context(|| "Failed to find latest ruff version")?;
            bin_install_resolved(
                Binary::Ruff,
                &resolved,
                &client,
                &retry_policy,
                &cache,
                &reporter,
            )
            .await
            .with_context(|| format!("Failed to install ruff {}", resolved.version))?
        }
        BinVersion::Constraint(constraints) => {
            // Find the best version matching the constraints
            let resolved =
                find_matching_version(Binary::Ruff, Some(&constraints), exclude_newer, &client)
                    .await
                    .with_context(|| {
                        format!("Failed to find ruff version matching: {constraints}")
                    })?;
            bin_install_resolved(
                Binary::Ruff,
                &resolved,
                &client,
                &retry_policy,
                &cache,
                &reporter,
            )
            .await
            .with_context(|| format!("Failed to install ruff {}", resolved.version))?
        }
    };

    let mut command = Command::new(&ruff_path);
    command.current_dir(target_dir);
    command.arg("format");

    if check {
        command.arg("--check");
    }
    if diff {
        command.arg("--diff");
    }

    // Add any additional arguments passed after `--`
    command.args(extra_args.iter());

    let handle = command.spawn().context("Failed to spawn `ruff format`")?;
    run_to_completion(handle).await
}

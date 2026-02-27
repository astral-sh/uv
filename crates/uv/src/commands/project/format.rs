use std::fmt::Write;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::debug;

use uv_bin_install::{BinVersion, Binary, ResolvedVersion, bin_install, find_matching_version};
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_preview::{Preview, PreviewFeature};
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache, WorkspaceError};

use crate::child::run_to_completion;
use crate::commands::ExitStatus;
use crate::commands::reporters::BinaryDownloadReporter;
use crate::printer::Printer;

/// Run the formatter.
#[expect(clippy::fn_params_excessive_bools)]
pub(crate) async fn format(
    project_dir: &Path,
    check: bool,
    diff: bool,
    extra_args: Vec<String>,
    version: Option<String>,
    exclude_newer: Option<jiff::Timestamp>,
    show_version: bool,
    client_builder: BaseClientBuilder<'_>,
    cache: Cache,
    printer: Printer,
    preview: Preview,
    no_project: bool,
) -> Result<ExitStatus> {
    // Check if the format feature is in preview
    if !preview.is_enabled(PreviewFeature::Format) {
        warn_user!(
            "`uv format` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::Format
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

    let resolved = match bin_version {
        BinVersion::Default => {
            // Find the best version matching the default constraints
            let constraints = Binary::Ruff.default_constraints();
            let resolved = find_matching_version(
                Binary::Ruff,
                Some(&constraints),
                exclude_newer,
                &client,
                &retry_policy,
            )
            .await
            .with_context(|| {
                format!("Failed to find ruff version matching default constraints: {constraints}")
            })?;
            debug!(
                "Resolved `ruff@{constraints}` to `ruff=={}`",
                resolved.version
            );
            resolved
        }
        BinVersion::Pinned(version) => {
            // Use the exact version directly without manifest lookup.
            if exclude_newer.is_some() {
                debug!("`--exclude-newer` is ignored for pinned version `{version}`");
            }
            ResolvedVersion::from_version(Binary::Ruff, version)?
        }
        BinVersion::Latest => {
            // Fetch the latest version from the manifest
            let resolved =
                find_matching_version(Binary::Ruff, None, exclude_newer, &client, &retry_policy)
                    .await
                    .with_context(|| "Failed to find latest ruff version")?;
            debug!("Resolved `ruff@latest` to `ruff=={}`", resolved.version);
            resolved
        }
        BinVersion::Constraint(constraints) => {
            // Find the best version matching the constraints
            let resolved = find_matching_version(
                Binary::Ruff,
                Some(&constraints),
                exclude_newer,
                &client,
                &retry_policy,
            )
            .await
            .with_context(|| format!("Failed to find ruff version matching: {constraints}"))?;
            debug!(
                "Resolved `ruff@{constraints}` to `ruff=={}`",
                resolved.version
            );
            resolved
        }
    };

    if show_version {
        writeln!(printer.stderr(), "ruff {}", resolved.version)?;
    }

    let ruff_path = bin_install(
        Binary::Ruff,
        &resolved,
        &client,
        &retry_policy,
        &cache,
        &reporter,
    )
    .await
    .with_context(|| format!("Failed to install ruff {}", resolved.version))?;

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

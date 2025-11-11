use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use tokio::process::Command;

use uv_bin_install::{Binary, bin_install};
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_pep440::Version;
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

    // Parse version if provided
    let version = version.as_deref().map(Version::from_str).transpose()?;

    let retry_policy = client_builder.retry_policy();
    // Python downloads are performing their own retries to catch stream errors, disable the
    // default retries to avoid the middleware from performing uncontrolled retries.
    let client = client_builder.retries(0).build();

    // Get the path to Ruff, downloading it if necessary
    let reporter = BinaryDownloadReporter::single(printer);
    let default_version = Binary::Ruff.default_version();
    let version = version.as_ref().unwrap_or(&default_version);
    let ruff_path = bin_install(
        Binary::Ruff,
        version,
        &client,
        &retry_policy,
        &cache,
        &reporter,
    )
    .await
    .with_context(|| format!("Failed to install ruff {version}"))?;

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

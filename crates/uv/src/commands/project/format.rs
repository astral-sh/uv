use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use tokio::process::Command;

use uv_bin_install::{Binary, bin_install};
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::{Preview, PreviewFeatures};
use uv_pep440::Version;
use uv_warnings::warn_user;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache};

use crate::child::run_to_completion;
use crate::commands::ExitStatus;
use crate::commands::reporters::BinaryDownloadReporter;
use crate::printer::Printer;
use crate::settings::NetworkSettings;

/// Run the formatter.
pub(crate) async fn format(
    project_dir: &Path,
    check: bool,
    diff: bool,
    extra_args: Vec<String>,
    version: Option<String>,
    network_settings: NetworkSettings,
    cache: Cache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    // Check if the format feature is in preview
    if !preview.is_enabled(PreviewFeatures::FORMAT) {
        warn_user!(
            "`uv format` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeatures::FORMAT
        );
    }

    let workspace_cache = WorkspaceCache::default();
    let project =
        VirtualProject::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache)
            .await?;

    // Parse version if provided
    let version = version.as_deref().map(Version::from_str).transpose()?;

    let client = BaseClientBuilder::new()
        .retries_from_env()?
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .allow_insecure_host(network_settings.allow_insecure_host.clone())
        .build();

    // Get the path to Ruff, downloading it if necessary
    let reporter = BinaryDownloadReporter::single(printer);
    let default_version = Binary::Ruff.default_version();
    let version = version.as_ref().unwrap_or(&default_version);
    let ruff_path = bin_install(Binary::Ruff, version, &client, &cache, &reporter)
        .await
        .context("Failed to install ruff {version}")?;

    let mut command = Command::new(&ruff_path);
    // Run ruff in the project root
    command.current_dir(project.root());
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

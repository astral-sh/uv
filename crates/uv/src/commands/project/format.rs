use std::fmt::Write;
use std::io::Write as IoWrite;
use std::str::FromStr;

use anyhow::{Context, Result};
use tokio::process::Command;

use uv_bin_install::{Binary, install};
use uv_cache::Cache;
use uv_cli::ExternalCommand;
use uv_client::BaseClientBuilder;
use uv_pep440::Version;

use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::NetworkSettings;

/// Format Python source files using Ruff.
pub(crate) async fn format(
    check: bool,
    diff: bool,
    args: Option<ExternalCommand>,
    version: Option<String>,
    network_settings: NetworkSettings,
    cache: Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Check if we're in offline mode
    if network_settings.connectivity.is_offline() && version.is_none() {
        // In offline mode without a specific version, we can't determine the latest version
        writeln!(
            printer.stderr(),
            "Ruff formatting is not available in offline mode without a specific version"
        )?;
        return Ok(ExitStatus::Failure);
    }

    // Parse version if provided
    let version = version
        .as_deref()
        .map(Version::from_str)
        .transpose()
        .context("Invalid version format")?;

    // Create HTTP client
    let client = BaseClientBuilder::new()
        .retries_from_env()?
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .allow_insecure_host(network_settings.allow_insecure_host.clone())
        .build();

    // Download or retrieve Ruff binary from cache
    let ruff_path = install(Binary::Ruff, version.as_ref(), &client, &cache)
        .await
        .context("Failed to install Ruff")?;

    // Construct the ruff format command.
    let mut command = Command::new(&ruff_path);
    command.arg("format");

    // Add check flag if requested.
    if check {
        command.arg("--check");
    }

    // Add diff flag if requested.
    if diff {
        command.arg("--diff");
    }

    // Ruff format defaults to the current directory when no files are specified.
    // If the user wants to format specific files, they can pass them after --
    // e.g., uv format -- src/main.py

    // Add any additional arguments passed after --.
    if let Some(args) = args {
        for arg in args.iter() {
            command.arg(arg);
        }
    }

    // Run the ruff format command.
    let output = command
        .output()
        .await
        .context("Failed to run ruff format")?;

    // Stream stdout and stderr.
    if !output.stdout.is_empty() {
        std::io::stdout().write_all(&output.stdout)?;
    }
    if !output.stderr.is_empty() {
        std::io::stderr().write_all(&output.stderr)?;
    }

    // Return the exit status.
    if output.status.success() {
        Ok(ExitStatus::Success)
    } else {
        Ok(ExitStatus::Failure)
    }
}

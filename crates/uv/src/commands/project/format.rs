use std::fmt::Write;
use std::io::Write as IoWrite;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::debug;

use uv_cache::Cache;
use uv_cli::ExternalCommand;
use uv_client::BaseClientBuilder;
use uv_python::platform::{Arch, Libc, Os};

use crate::commands::project::ruff_download::RuffDownload;
use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::NetworkSettings;

/// Format Python source files using Ruff.
pub(crate) async fn format(
    check: bool,
    diff: bool,
    files: Vec<PathBuf>,
    args: Option<ExternalCommand>,
    version: Option<String>,
    network_settings: NetworkSettings,
    cache: Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    debug!("format command called with check={}, diff={}, files={:?}, version={:?}", check, diff, files, version);
    // Check if we're in offline mode
    if network_settings.connectivity.is_offline() && version.is_none() {
        // In offline mode without a specific version, we can't determine the latest version
        writeln!(
            printer.stderr(),
            "Ruff formatting is not available in offline mode without a specific version"
        )?;
        return Ok(ExitStatus::Failure);
    }

    // Get current platform information
    debug!("Getting platform information");
    let os = Os::from_env();
    let arch = Arch::from_env();
    let libc = if cfg!(target_env = "musl") {
        Libc::Some(target_lexicon::Environment::Musl)
    } else if cfg!(target_os = "linux") {
        Libc::Some(target_lexicon::Environment::Gnu)
    } else {
        Libc::None
    };
    debug!("Platform: os={}, arch={}, libc={}", os, arch, libc);

    // Create HTTP client
    debug!("Creating HTTP client");
    let client = BaseClientBuilder::new()
        .retries_from_env()?
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .allow_insecure_host(network_settings.allow_insecure_host.clone())
        .build();
    debug!("HTTP client created");

    // Download or retrieve Ruff binary from cache
    debug!("Calling RuffDownload::download");
    let ruff_path = RuffDownload::download(
        version.as_deref(),
        &os,
        &arch,
        &libc,
        &client,
        &cache,
    )
    .await
    .context("Failed to download Ruff")?;
    debug!("Got ruff binary at: {}", ruff_path.display());

    // Construct the ruff format command.
    debug!("Constructing ruff command with binary: {}", ruff_path.display());
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

    // Add files or directories to format.
    if files.is_empty() {
        // If no files specified, format the current directory.
        command.arg(".");
    } else {
        for file in &files {
            command.arg(file);
        }
    }

    // Add any additional arguments passed after --.
    if let Some(args) = args {
        for arg in args.iter() {
            command.arg(arg);
        }
    }

    debug!("Full ruff format command: {:?}", command);
    debug!("About to execute command");

    // Run the ruff format command.
    debug!("Executing command.output()");
    let output = command.output().await
        .map_err(|e| {
            debug!("Command execution failed: {}", e);
            anyhow::anyhow!("Failed to run ruff format at {}: {}", ruff_path.display(), e)
        })?;
    debug!("Command executed successfully, status: {}", output.status);

    // Stream stdout and stderr.
    if !output.stdout.is_empty() {
        std::io::stdout().write_all(&output.stdout)?;
    }
    if !output.stderr.is_empty() {
        std::io::stderr().write_all(&output.stderr)?;
    }

    // Return the exit status.
    if output.status.success() {
        debug!("Ruff format completed successfully");
        Ok(ExitStatus::Success)
    } else {
        debug!("Ruff format failed with non-zero exit code");
        Ok(ExitStatus::Failure)
    }
}
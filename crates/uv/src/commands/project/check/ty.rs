use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;
use tokio::process::{ChildStdin, Command};
use tracing::debug;

use uv_bin_install::{BinVersion, Binary, ResolvedVersion, bin_install, find_matching_version};
use uv_cache::Cache;
use uv_client::BaseClientBuilder;

use crate::child::run_to_completion;
use crate::commands::ExitStatus;
use crate::commands::reporters::BinaryDownloadReporter;
use crate::printer::Printer;

/// Limit how long uv can block if a version of ty does not consume metadata from stdin.
const WORKSPACE_METADATA_WRITE_TIMEOUT: Duration = Duration::from_mins(1);

async fn write_workspace_metadata(mut stdin: ChildStdin, workspace_metadata: String) -> Result<()> {
    match tokio::time::timeout(
        WORKSPACE_METADATA_WRITE_TIMEOUT,
        stdin.write_all(workspace_metadata.as_bytes()),
    )
    .await
    {
        Err(err) => Err(err).context("Timed out while writing workspace metadata to `ty check`"),
        Ok(Err(err)) if err.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Ok(Err(err)) => Err(err).context("Failed to write workspace metadata to `ty check`"),
        Ok(Ok(())) => Ok(()),
    }
}

/// Run a type check powered by ty.
pub(super) async fn run(
    version: Option<String>,
    ty_path: Option<PathBuf>,
    target_dir: &Path,
    check_target: Option<&Path>,
    venv_path: Option<&Path>,
    workspace_metadata: Option<String>,
    exclude_newer: Option<jiff::Timestamp>,
    show_version: bool,
    client_builder: &BaseClientBuilder<'_>,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let ty_path = if let Some(ty_path) = ty_path {
        if show_version {
            let output = Command::new(&ty_path)
                .arg("--version")
                .output()
                .await
                .context("Failed to query ty version")?;
            if !output.status.success() {
                anyhow::bail!("Failed to query ty version");
            }
            let version = String::from_utf8_lossy(&output.stdout);
            writeln!(printer.stderr(), "Using {}", version.trim())?;
        }
        ty_path
    } else {
        let retry_policy = client_builder.retry_policy();
        let ty_client = client_builder.clone().retries(0).build()?;

        let reporter = BinaryDownloadReporter::single(printer);
        let bin_version = version
            .as_deref()
            .map(BinVersion::from_str)
            .transpose()?
            .unwrap_or(BinVersion::Default);

        let resolved = match bin_version {
            BinVersion::Default => {
                let constraints = Binary::Ty.default_constraints();
                let resolved = find_matching_version(
                    Binary::Ty,
                    Some(&constraints),
                    exclude_newer,
                    &ty_client,
                    &retry_policy,
                )
                .await
                .with_context(|| {
                    format!("Failed to find ty version matching default constraints: {constraints}")
                })?;
                debug!("Resolved `ty@{constraints}` to `ty=={}`", resolved.version);
                resolved
            }
            BinVersion::Pinned(version) => {
                if exclude_newer.is_some() {
                    debug!("`--exclude-newer` is ignored for pinned version `{version}`");
                }
                let resolved = ResolvedVersion::from_version(Binary::Ty, version)?;
                debug!("Using `ty=={}`", resolved.version);
                resolved
            }
            BinVersion::Latest => {
                let resolved = find_matching_version(
                    Binary::Ty,
                    None,
                    exclude_newer,
                    &ty_client,
                    &retry_policy,
                )
                .await
                .with_context(|| "Failed to find latest ty version")?;
                debug!("Resolved `ty@latest` to `ty=={}`", resolved.version);
                resolved
            }
            BinVersion::Constraint(constraints) => {
                let resolved = find_matching_version(
                    Binary::Ty,
                    Some(&constraints),
                    exclude_newer,
                    &ty_client,
                    &retry_policy,
                )
                .await
                .with_context(|| format!("Failed to find ty version matching: {constraints}"))?;
                debug!("Resolved `ty@{constraints}` to `ty=={}`", resolved.version);
                resolved
            }
        };

        if show_version {
            writeln!(printer.stderr(), "Using ty {}", resolved.version)?;
        }

        bin_install(
            Binary::Ty,
            &resolved,
            &ty_client,
            &retry_policy,
            cache,
            &reporter,
        )
        .await
        .with_context(|| format!("Failed to install ty {}", resolved.version))?
    };

    let mut command = Command::new(&ty_path);
    command.current_dir(target_dir);
    command.arg("check");
    if let Some(check_target) = check_target {
        command.arg("--");
        command.arg(
            check_target
                .strip_prefix(target_dir)
                .unwrap_or(check_target),
        );
    }
    // Opt into ty querying uv for project metadata.
    command.env("TY_UV", "1");

    if let Some(venv_path) = venv_path {
        command.env("VIRTUAL_ENV", venv_path);
    }

    if workspace_metadata.is_some() {
        // Tell `ty` to expect uv metadata on stdin.
        // This is an environment variable so older ty's don't complain about an unknown CLI flag.
        command.env("TY_UV_METADATA", "1");
        command.stdin(Stdio::piped());
        command.kill_on_drop(true);
    } else {
        // Do not let the calling environment opt ty into a protocol uv cannot supply.
        command.env_remove("TY_UV_METADATA");
    }

    let mut handle = command.spawn().context("Failed to spawn `ty check`")?;
    let writer = if let Some(workspace_metadata) = workspace_metadata {
        debug!("Passing workspace metadata to `ty check` via stdin");
        let stdin = handle
            .stdin
            .take()
            .context("Failed to open stdin for `ty check`")?;
        Some(write_workspace_metadata(stdin, workspace_metadata))
    } else {
        None
    };

    if let Some(writer) = writer {
        let (status, ()) = tokio::try_join!(run_to_completion(handle), writer)?;
        Ok(status)
    } else {
        run_to_completion(handle).await
    }
}

use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::debug;

use uv_bin_install::{BinVersion, Binary, ResolvedVersion, bin_install, find_matching_version};
use uv_cache::Cache;
use uv_client::BaseClientBuilder;

use crate::child::run_to_completion;
use crate::commands::ExitStatus;
use crate::commands::reporters::BinaryDownloadReporter;
use crate::printer::Printer;

/// Run a type check powered by ty.
pub(super) async fn run(
    version: Option<String>,
    ty_path: Option<PathBuf>,
    target_dir: &Path,
    check_targets: &[PathBuf],
    excluded_targets: &[PathBuf],
    venv_path: Option<&Path>,
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
    for excluded_target in excluded_targets {
        command.arg("--exclude");
        command.arg(
            excluded_target
                .strip_prefix(target_dir)
                .unwrap_or(excluded_target),
        );
    }
    if !check_targets.is_empty() {
        // Keep paths relative to the working directory for stable diagnostics, and use `--` so
        // option-like filenames are treated as paths.
        command.arg("--");
        for check_target in check_targets {
            command.arg(
                check_target
                    .strip_prefix(target_dir)
                    .unwrap_or(check_target),
            );
        }
    }
    // Opt into ty querying uv for project metadata.
    command.env("TY_UV", "1");

    if let Some(venv_path) = venv_path {
        command.env("VIRTUAL_ENV", venv_path);
    }

    let handle = command.spawn().context("Failed to spawn `ty check`")?;
    run_to_completion(handle).await
}

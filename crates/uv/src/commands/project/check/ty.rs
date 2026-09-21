use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::debug;

use uv_bin_install::{BinVersion, Binary, ResolvedVersion, bin_install, find_matching_version};
use uv_cache::Cache;
use uv_cli::ColorChoice;
use uv_client::BaseClientBuilder;
use uv_fs::Simplified;
use uv_pep440::Version;
use uv_shell::shlex_posix;

use crate::child::run_to_completion;
use crate::commands::ExitStatus;
use crate::commands::reporters::BinaryDownloadReporter;
use crate::commands::workspace::list::{ScriptDiscoveryError, find_scripts};
use crate::printer::Printer;

/// Run a type check powered by ty.
pub(super) async fn run(
    version: Option<String>,
    ty_path: Option<PathBuf>,
    fix: bool,
    target_dir: &Path,
    workspace_root: Option<&Path>,
    check_targets: &[PathBuf],
    excluded_targets: &[PathBuf],
    venv_path: Option<&Path>,
    exclude_newer: Option<jiff::Timestamp>,
    show_version: bool,
    show_command: bool,
    client_builder: &BaseClientBuilder<'_>,
    cache: &Cache,
    color: ColorChoice,
    printer: Printer,
) -> Result<ExitStatus> {
    let (ty_path, ty_version) = if let Some(ty_path) = ty_path {
        let output = Command::new(&ty_path)
            .arg("--version")
            .output()
            .await
            .context("Failed to query ty version")?;
        if !output.status.success() {
            anyhow::bail!("Failed to query ty version");
        }
        let version = String::from_utf8_lossy(&output.stdout);
        let ty_version = version
            .split_whitespace()
            .nth(1)
            .context("Failed to parse ty version")?
            .parse::<Version>()
            .context("Failed to parse ty version")?;

        if show_version {
            writeln!(printer.stderr(), "Using {}", version.trim())?;
        }

        (ty_path, ty_version)
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

        let ty_path = bin_install(
            Binary::Ty,
            &resolved,
            &ty_client,
            &retry_policy,
            cache,
            &reporter,
        )
        .await
        .with_context(|| format!("Failed to install ty {}", resolved.version))?;

        (ty_path, resolved.version)
    };

    let mut command = Command::new(&ty_path);
    command.current_dir(target_dir);
    command.arg("check");
    command.arg("--color").arg(color.as_str());
    if printer.suppresses_progress() {
        command.arg("--no-progress");
    }
    if fix {
        command.arg("--fix");
    }
    // PEP 723 scripts have independent environments and must be checked explicitly with
    // `uv check --script`. This still allows explicitly selected script paths to be checked.
    // Older versions of ty do not support `--exclude-scripts`, so discover and exclude their
    // workspace scripts individually instead.
    let mut excluded_scripts = Vec::new();
    if ty_version >= Version::new([0, 0, 64]) {
        command.arg("--exclude-scripts");
    } else if let Some(workspace_root) = workspace_root {
        excluded_scripts.extend(
            find_scripts(workspace_root, cache)
                .filter_map(|script| match script {
                    Ok(script) => check_targets
                        .iter()
                        .any(|target| script.starts_with(target))
                        .then_some(Ok(script)),
                    Err(ScriptDiscoveryError::Parse { path, source }) => {
                        debug!(
                            "Excluding invalid PEP 723 script `{}` while checking project root `{}`: {source}",
                            path.simplified_display(),
                            target_dir.simplified_display(),
                        );
                        check_targets
                            .iter()
                            .any(|target| path.starts_with(target))
                            .then_some(Ok(path))
                    }
                    Err(error) => Some(Err(error)),
                })
                .collect::<Result<Vec<_>, _>>()
                .with_context(|| {
                    format!(
                        "Failed to discover PEP 723 scripts while checking project root `{}`",
                        target_dir.simplified_display()
                    )
                })?,
        );
    }

    for excluded_target in excluded_targets.iter().chain(&excluded_scripts) {
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

    if show_command {
        let mut stderr = printer.stderr_important();
        write!(stderr, "Running `ty")?;
        for argument in command.as_std().get_args() {
            write!(stderr, " {}", shlex_posix(argument))?;
        }
        writeln!(stderr, "`")?;
    }

    let handle = command.spawn().context("Failed to spawn `ty check`")?;
    run_to_completion(handle).await
}

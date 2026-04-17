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

/// Run the type checker.
pub(crate) async fn check(
    project_dir: &Path,
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
    if !preview.is_enabled(PreviewFeature::Check) {
        warn_user!(
            "`uv check` is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::Check
        );
    }

    let workspace_cache = WorkspaceCache::default();
    let target_dir = if no_project {
        project_dir.to_owned()
    } else {
        match VirtualProject::discover(project_dir, &DiscoveryOptions::default(), &workspace_cache)
            .await
        {
            Ok(proj) => proj.root().to_owned(),
            Err(
                WorkspaceError::MissingPyprojectToml
                | WorkspaceError::MissingProject(_)
                | WorkspaceError::NonWorkspace(_),
            ) => project_dir.to_owned(),
            Err(err) => return Err(err.into()),
        }
    };

    let retry_policy = client_builder.retry_policy();
    let client = client_builder.retries(0).build()?;

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
                &client,
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
            ResolvedVersion::from_version(Binary::Ty, version)?
        }
        BinVersion::Latest => {
            let resolved =
                find_matching_version(Binary::Ty, None, exclude_newer, &client, &retry_policy)
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
                &client,
                &retry_policy,
            )
            .await
            .with_context(|| format!("Failed to find ty version matching: {constraints}"))?;
            debug!("Resolved `ty@{constraints}` to `ty=={}`", resolved.version);
            resolved
        }
    };

    if show_version {
        writeln!(printer.stderr(), "ty {}", resolved.version)?;
    }

    let ty_path = bin_install(
        Binary::Ty,
        &resolved,
        &client,
        &retry_policy,
        &cache,
        &reporter,
    )
    .await
    .with_context(|| format!("Failed to install ty {}", resolved.version))?;

    let mut command = Command::new(&ty_path);
    command.current_dir(&target_dir);
    command.arg("check");

    let venv_dir = target_dir.join(".venv");
    if venv_dir.is_dir() {
        command.env("VIRTUAL_ENV", &venv_dir);
    }

    command.args(extra_args.iter());

    let handle = command.spawn().context("Failed to spawn `ty check`")?;
    run_to_completion(handle).await
}

use std::path::Path;
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

/// A type check powered by ty.
pub(super) struct TyCheck {
    version: Option<String>,
}

impl TyCheck {
    pub(super) fn new(version: Option<String>) -> Self {
        Self { version }
    }

    pub(super) async fn run(
        self,
        target_dir: &Path,
        venv_path: Option<&Path>,
        exclude_newer: Option<jiff::Timestamp>,
        client_builder: &BaseClientBuilder<'_>,
        cache: &Cache,
        printer: Printer,
    ) -> Result<ExitStatus> {
        let Self { version } = self;

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
                ResolvedVersion::from_version(Binary::Ty, version)?
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

        let mut command = Command::new(&ty_path);
        command.current_dir(target_dir);
        command.arg("check");

        if let Some(venv_path) = venv_path {
            command.env("VIRTUAL_ENV", venv_path);
        }

        let handle = command.spawn().context("Failed to spawn `ty check`")?;
        run_to_completion(handle).await
    }
}

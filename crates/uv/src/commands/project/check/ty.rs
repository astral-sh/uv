use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::io::AsyncWriteExt;
use tokio::process::{ChildStdin, Command};
use tracing::debug;

use uv_bin_install::{BinVersion, Binary, ResolvedVersion, bin_install, find_matching_version};
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::NoSources;
use uv_normalize::{DEV_DEPENDENCIES, PackageName};
use uv_python::{Interpreter, PythonEnvironment};
use uv_resolver::Lock;
use uv_workspace::VirtualProject;
use uv_workspace::dependency_groups::FlatDependencyGroups;
use uv_workspace::pyproject::{Source, ToolUvSources};

use crate::child::run_to_completion;
use crate::commands::ExitStatus;
use crate::commands::reporters::BinaryDownloadReporter;
use crate::printer::Printer;

/// Limit how long uv can block if a version of ty does not consume metadata from stdin.
const WORKSPACE_METADATA_WRITE_TIMEOUT: Duration = Duration::from_mins(1);

/// An active `ty` declaration in the current project's development dependencies.
pub(super) struct ActiveDeclaration {
    package_name: PackageName,
}

/// Find and validate an active `ty` declaration for the selected interpreter.
pub(super) fn active_declaration(
    project: &VirtualProject,
    interpreter: &Interpreter,
    no_sources: &NoSources,
) -> Result<Option<ActiveDeclaration>> {
    let package_name = PackageName::from_str("ty")?;
    let dependency_groups =
        FlatDependencyGroups::from_pyproject_toml(project.root(), project.pyproject_toml())?;
    let Some(dev_dependencies) = dependency_groups.get(&DEV_DEPENDENCIES) else {
        return Ok(None);
    };

    let active_requirements = dev_dependencies
        .requirements
        .iter()
        .filter(|requirement| {
            requirement.name == package_name
                && requirement.evaluate_markers(interpreter.markers(), &[])
        })
        .collect::<Vec<_>>();
    if active_requirements.is_empty() {
        return Ok(None);
    }

    let source = if no_sources.for_package(&package_name) {
        None
    } else {
        project
            .pyproject_toml()
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.sources.as_ref())
            .map(ToolUvSources::inner)
            .and_then(|sources| sources.get(&package_name))
            .or_else(|| project.workspace().sources().get(&package_name))
            .and_then(|sources| {
                sources.iter().find(|source| {
                    source.extra().is_none()
                        && source
                            .group()
                            .is_none_or(|group| group == &*DEV_DEPENDENCIES)
                        && source.marker().evaluate(interpreter.markers(), &[])
                })
            })
    };

    match source {
        Some(Source::Registry { .. }) => {}
        Some(source) => {
            bail!(
                "The active `ty` development dependency uses the non-registry source `{}`, but `uv check` can only install standalone `ty` releases by version; use a registry source, `--ty-version`, or the `TY` environment variable",
                source_reference(source)
            );
        }
        None if let Some(url) = active_requirements.iter().find_map(|requirement| {
            let Some(uv_pep508::VersionOrUrl::Url(url)) = requirement.version_or_url.as_ref()
            else {
                return None;
            };
            Some(url)
        }) =>
        {
            bail!(
                "The active `ty` development dependency uses the direct URL `{url}`, but `uv check` can only install standalone `ty` releases by version; use a registry requirement, `--ty-version`, or the `TY` environment variable"
            );
        }
        None => {}
    }

    Ok(Some(ActiveDeclaration { package_name }))
}

fn source_reference(source: &Source) -> String {
    match source {
        Source::Git { git, .. } => git.to_string(),
        Source::Url { url, .. } => url.to_string(),
        Source::Path { path, .. } => path.to_string(),
        Source::Registry { index, .. } => index.to_string(),
        Source::Workspace { workspace, .. } => format!("workspace = {workspace}"),
    }
}

/// Select the exact registry version of `ty` reachable from the current project in the lockfile.
pub(super) fn version_from_lock(
    declaration: &ActiveDeclaration,
    project: &VirtualProject,
    lock: &Lock,
    environment: &PythonEnvironment,
) -> Result<String> {
    let marker_environment = environment.interpreter().resolver_marker_environment();
    let package = lock
        .find_dependency_group_package(
            project.project_name(),
            &DEV_DEPENDENCIES,
            &declaration.package_name,
            marker_environment.markers(),
        )
        .map_err(anyhow::Error::msg)?;
    let Some(package) = package else {
        bail!(
            "The active `ty` development dependency is not present in the lockfile for the selected Python environment; update `uv.lock`, or use `--ty-version` or the `TY` environment variable"
        );
    };
    if package.index(project.workspace().install_path())?.is_none() {
        bail!(
            "The locked `ty` package uses a non-registry source, but `uv check` can only install standalone `ty` releases by version; use a registry source, `--ty-version`, or the `TY` environment variable"
        );
    }
    let Some(version) = package.version() else {
        bail!(
            "The locked `ty` package has no version, but `uv check` can only install standalone `ty` releases by version; use a registry source, `--ty-version`, or the `TY` environment variable"
        );
    };
    Ok(version.to_string())
}

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
    venv_path: Option<&Path>,
    workspace_metadata: Option<String>,
    exclude_newer: Option<jiff::Timestamp>,
    client_builder: &BaseClientBuilder<'_>,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let ty_path = if let Some(ty_path) = ty_path {
        if tracing::enabled!(tracing::Level::DEBUG) {
            let output = Command::new(&ty_path)
                .arg("--version")
                .output()
                .await
                .context("Failed to query ty version")?;
            if !output.status.success() {
                anyhow::bail!("Failed to query ty version");
            }
            debug!("Using `{}`", String::from_utf8_lossy(&output.stdout).trim());
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

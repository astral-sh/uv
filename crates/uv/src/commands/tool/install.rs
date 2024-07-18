use std::fmt::Write;
use std::str::FromStr;

use anyhow::{bail, Result};
use distribution_types::UnresolvedRequirementSpecification;
use owo_colors::OwoColorize;
use tracing::debug;

use pypi_types::Requirement;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::{Concurrency, PreviewMode};
use uv_normalize::PackageName;
use uv_python::{
    EnvironmentPreference, PythonFetch, PythonInstallation, PythonPreference, PythonRequest,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_tool::InstalledTools;
use uv_warnings::{warn_user, warn_user_once};

use crate::commands::{
    project::{resolve_environment, resolve_names, sync_environment, update_environment},
    tool::common::InstallAction,
};
use crate::commands::{reporters::PythonDownloadReporter, tool::common::install_executables};
use crate::commands::{ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Install a tool.
pub(crate) async fn install(
    package: String,
    from: Option<String>,
    with: &[RequirementsSource],
    python: Option<String>,
    force: bool,
    settings: ResolverInstallerSettings,
    preview: PreviewMode,
    python_preference: PythonPreference,
    python_fetch: PythonFetch,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv tool install` is experimental and may change without warning");
    }

    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls);

    let reporter = PythonDownloadReporter::single(printer);

    let python_request = python.as_deref().map(PythonRequest::parse);

    // Pre-emptively identify a Python interpreter. We need an interpreter to resolve any unnamed
    // requirements, even if we end up using a different interpreter for the tool install itself.
    let interpreter = PythonInstallation::find_or_fetch(
        python_request.clone(),
        EnvironmentPreference::OnlySystem,
        python_preference,
        python_fetch,
        &client_builder,
        cache,
        Some(&reporter),
    )
    .await?
    .into_interpreter();

    // Initialize any shared state.
    let state = SharedState::default();

    // Resolve the `from` requirement.
    let from = if let Some(from) = from {
        // Parse the positional name. If the user provided more than a package name, it's an error
        // (e.g., `uv install foo==1.0 --from foo`).
        let Ok(package) = PackageName::from_str(&package) else {
            bail!("Package requirement (`{from}`) provided with `--from` conflicts with install request (`{package}`)", from = from.cyan(), package = package.cyan())
        };

        let from_requirement = {
            resolve_names(
                vec![RequirementsSpecification::parse_package(&from)?],
                &interpreter,
                &settings,
                &state,
                preview,
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer,
            )
            .await?
            .pop()
            .unwrap()
        };

        // Check if the positional name conflicts with `--from`.
        if from_requirement.name != package {
            // Determine if it's an entirely different package (e.g., `uv install foo --from bar`).
            bail!(
                "Package name (`{}`) provided with `--from` does not match install request (`{}`)",
                from_requirement.name.cyan(),
                package.cyan()
            );
        }

        from_requirement
    } else {
        resolve_names(
            vec![RequirementsSpecification::parse_package(&package)?],
            &interpreter,
            &settings,
            &state,
            preview,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
        )
        .await?
        .pop()
        .unwrap()
    };

    // Read the `--with` requirements.
    let spec = {
        let client_builder = BaseClientBuilder::new()
            .connectivity(connectivity)
            .native_tls(native_tls);
        RequirementsSpecification::from_simple_sources(with, &client_builder).await?
    };

    // Resolve the `--from` and `--with` requirements.
    let requirements = {
        let mut requirements = Vec::with_capacity(1 + with.len());
        requirements.push(from.clone());
        requirements.extend(
            resolve_names(
                spec.requirements.clone(),
                &interpreter,
                &settings,
                &state,
                preview,
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer,
            )
            .await?,
        );
        requirements
    };

    let installed_tools = InstalledTools::from_settings()?.init()?;
    let _lock = installed_tools.acquire_lock()?;

    // Find the existing receipt, if it exists. If the receipt is present but malformed, we'll
    // remove the environment and continue with the install.
    //
    // Later on, we want to replace entrypoints if the tool already exists, regardless of whether
    // the receipt was valid.
    //
    // (If we find existing entrypoints later on, and the tool _doesn't_ exist, we'll avoid removing
    // the external tool's entrypoints (without `--force`).)
    let (existing_tool_receipt, reinstall_entry_points) =
        match installed_tools.get_tool_receipt(&from.name) {
            Ok(None) => (None, false),
            Ok(Some(receipt)) => (Some(receipt), true),
            Err(_) => {
                // If the tool is not installed properly, remove the environment and continue.
                match installed_tools.remove_environment(&from.name) {
                    Ok(()) => {
                        warn_user!(
                            "Removed existing `{from}` with invalid receipt",
                            from = from.name.cyan()
                        );
                    }
                    Err(uv_tool::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(err) => {
                        return Err(err.into());
                    }
                }
                (None, true)
            }
        };

    let existing_environment =
        installed_tools
            .get_environment(&from.name, cache)?
            .filter(|environment| {
                python_request.as_ref().map_or(true, |python_request| {
                    if python_request.satisfied(environment.interpreter(), cache) {
                        debug!("Found existing environment for `{from}`", from = from.name.cyan());
                        true
                    } else {
                        let _ = writeln!(
                            printer.stderr(),
                            "Existing environment for `{from}` does not satisfy the requested Python interpreter",
                            from = from.name.cyan(),
                        );
                        false
                    }
                })
            });

    // If the requested and receipt requirements are the same...
    if existing_environment.is_some() {
        if let Some(tool_receipt) = existing_tool_receipt.as_ref() {
            let receipt = tool_receipt
                .requirements()
                .iter()
                .cloned()
                .map(Requirement::from)
                .collect::<Vec<_>>();
            if requirements == receipt {
                // And the user didn't request a reinstall or upgrade...
                if !force && settings.reinstall.is_none() && settings.upgrade.is_none() {
                    // We're done.
                    writeln!(
                        printer.stderr(),
                        "`{from}` is already installed",
                        from = from.cyan()
                    )?;
                    return Ok(ExitStatus::Success);
                }
            }
        }
    }

    // Create a `RequirementsSpecification` from the resolved requirements, to avoid re-resolving.
    let spec = RequirementsSpecification {
        requirements: requirements
            .iter()
            .cloned()
            .map(UnresolvedRequirementSpecification::from)
            .collect(),
        ..spec
    };

    // TODO(zanieb): Build the environment in the cache directory then copy into the tool directory.
    // This lets us confirm the environment is valid before removing an existing install. However,
    // entrypoints always contain an absolute path to the relevant Python interpreter, which would
    // be invalidated by moving the environment.
    let environment = if let Some(environment) = existing_environment {
        update_environment(
            environment,
            spec,
            &settings,
            &state,
            preview,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
        )
        .await?
    } else {
        // If we're creating a new environment, ensure that we can resolve the requirements prior
        // to removing any existing tools.
        let resolution = resolve_environment(
            &interpreter,
            spec,
            settings.as_ref().into(),
            &state,
            preview,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
        )
        .await?;

        let environment = installed_tools.create_environment(&from.name, interpreter)?;

        // Sync the environment with the resolved requirements.
        sync_environment(
            environment,
            &resolution.into(),
            settings.as_ref().into(),
            &state,
            preview,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
        )
        .await?
    };
    install_executables(
        &environment,
        &from.name,
        &installed_tools,
        printer,
        force,
        reinstall_entry_points,
        python,
        requirements,
        &InstallAction::Install,
    )
}

use std::fmt::Write;
use std::str::FromStr;

use anyhow::{bail, Result};
use owo_colors::OwoColorize;
use tracing::{debug, trace};
use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::{Concurrency, TrustedHost, Upgrade};
use uv_distribution_types::UnresolvedRequirementSpecification;
use uv_normalize::PackageName;
use uv_pep440::{VersionSpecifier, VersionSpecifiers};
use uv_pep508::MarkerTree;
use uv_pypi_types::{Requirement, RequirementSource};
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonInstallation, PythonPreference, PythonRequest,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_settings::{ResolverInstallerOptions, ToolOptions};
use uv_tool::InstalledTools;
use uv_warnings::warn_user;

use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger};

use crate::commands::project::{
    resolve_environment, resolve_names, sync_environment, update_environment,
    EnvironmentSpecification,
};
use crate::commands::tool::common::remove_entrypoints;
use crate::commands::tool::Target;
use crate::commands::{reporters::PythonDownloadReporter, tool::common::install_executables};
use crate::commands::{ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Install a tool.
pub(crate) async fn install(
    package: String,
    editable: bool,
    from: Option<String>,
    with: &[RequirementsSource],
    python: Option<String>,
    force: bool,
    options: ResolverInstallerOptions,
    settings: ResolverInstallerSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    cache: Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls)
        .allow_insecure_host(allow_insecure_host.to_vec());

    let reporter = PythonDownloadReporter::single(printer);

    let python_request = python.as_deref().map(PythonRequest::parse);

    // Pre-emptively identify a Python interpreter. We need an interpreter to resolve any unnamed
    // requirements, even if we end up using a different interpreter for the tool install itself.
    let interpreter = PythonInstallation::find_or_download(
        python_request.as_ref(),
        EnvironmentPreference::OnlySystem,
        python_preference,
        python_downloads,
        &client_builder,
        &cache,
        Some(&reporter),
    )
    .await?
    .into_interpreter();

    // Initialize any shared state.
    let state = SharedState::default();

    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls)
        .allow_insecure_host(allow_insecure_host.to_vec());

    // Parse the input requirement.
    let target = Target::parse(&package, from.as_deref());

    // If the user passed, e.g., `ruff@latest`, refresh the cache.
    let cache = if target.is_latest() {
        cache.with_refresh(Refresh::All(Timestamp::now()))
    } else {
        cache
    };

    // Resolve the `--from` requirement.
    let from = match target {
        // Ex) `ruff`
        Target::Unspecified(name) => {
            let source = if editable {
                RequirementsSource::Editable(name.to_string())
            } else {
                RequirementsSource::Package(name.to_string())
            };
            let requirements = RequirementsSpecification::from_source(&source, &client_builder)
                .await?
                .requirements;
            resolve_names(
                requirements,
                &interpreter,
                &settings,
                &state,
                connectivity,
                concurrency,
                native_tls,
                allow_insecure_host,
                &cache,
                printer,
            )
            .await?
            .pop()
            .unwrap()
        }
        // Ex) `ruff@0.6.0`
        Target::Version(name, ref version) | Target::FromVersion(_, name, ref version) => {
            if editable {
                bail!("`--editable` is only supported for local packages");
            }

            Requirement {
                name: PackageName::from_str(name)?,
                extras: vec![],
                marker: MarkerTree::default(),
                source: RequirementSource::Registry {
                    specifier: VersionSpecifiers::from(VersionSpecifier::equals_version(
                        version.clone(),
                    )),
                    index: None,
                },
                origin: None,
            }
        }
        // Ex) `ruff@latest`
        Target::Latest(name) | Target::FromLatest(_, name) => {
            if editable {
                bail!("`--editable` is only supported for local packages");
            }

            Requirement {
                name: PackageName::from_str(name)?,
                extras: vec![],
                marker: MarkerTree::default(),
                source: RequirementSource::Registry {
                    specifier: VersionSpecifiers::empty(),
                    index: None,
                },
                origin: None,
            }
        }
        // Ex) `ruff>=0.6.0`
        Target::From(package, from) => {
            // Parse the positional name. If the user provided more than a package name, it's an error
            // (e.g., `uv install foo==1.0 --from foo`).
            let Ok(package) = PackageName::from_str(package) else {
                bail!("Package requirement (`{from}`) provided with `--from` conflicts with install request (`{package}`)", from = from.cyan(), package = package.cyan())
            };

            let source = if editable {
                RequirementsSource::Editable(from.to_string())
            } else {
                RequirementsSource::Package(from.to_string())
            };
            let requirements = RequirementsSpecification::from_source(&source, &client_builder)
                .await?
                .requirements;

            // Parse the `--from` requirement.
            let from_requirement = resolve_names(
                requirements,
                &interpreter,
                &settings,
                &state,
                connectivity,
                concurrency,
                native_tls,
                allow_insecure_host,
                &cache,
                printer,
            )
            .await?
            .pop()
            .unwrap();

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
        }
    };

    // If the user passed, e.g., `ruff@latest`, we need to mark it as upgradable.
    let settings = if target.is_latest() {
        ResolverInstallerSettings {
            upgrade: settings
                .upgrade
                .combine(Upgrade::package(from.name.clone())),
            ..settings
        }
    } else {
        settings
    };

    // Read the `--with` requirements.
    let spec = RequirementsSpecification::from_simple_sources(with, &client_builder).await?;

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
                connectivity,
                concurrency,
                native_tls,
                allow_insecure_host,
                &cache,
                printer,
            )
            .await?,
        );
        requirements
    };

    // Convert to tool options.
    let options = ToolOptions::from(options);

    let installed_tools = InstalledTools::from_settings()?.init()?;
    let _lock = installed_tools.lock().await?;

    // Find the existing receipt, if it exists. If the receipt is present but malformed, we'll
    // remove the environment and continue with the install.
    //
    // Later on, we want to replace entrypoints if the tool already exists, regardless of whether
    // the receipt was valid.
    //
    // (If we find existing entrypoints later on, and the tool _doesn't_ exist, we'll avoid removing
    // the external tool's entrypoints (without `--force`).)
    let (existing_tool_receipt, invalid_tool_receipt) =
        match installed_tools.get_tool_receipt(&from.name) {
            Ok(None) => (None, false),
            Ok(Some(receipt)) => (Some(receipt), false),
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
            .get_environment(&from.name, &cache)?
            .filter(|environment| {
                if environment.uses(&interpreter) {
                    trace!(
                        "Existing interpreter matches the requested interpreter for `{}`: {}",
                        from.name,
                        environment.interpreter().sys_executable().display()
                    );
                    true
                } else {
                    let _ = writeln!(
                        printer.stderr(),
                        "Ignoring existing environment for `{from}`: the requested Python interpreter does not match the environment interpreter",
                        from = from.name.cyan(),
                    );
                    false
                }
            });

    // If the requested and receipt requirements are the same...
    if existing_environment
        .as_ref()
        .filter(|_| {
            // And the user didn't request a reinstall or upgrade...
            !force
                && !target.is_latest()
                && settings.reinstall.is_none()
                && settings.upgrade.is_none()
        })
        .is_some()
    {
        if let Some(tool_receipt) = existing_tool_receipt.as_ref() {
            let receipt = tool_receipt.requirements().to_vec();
            if requirements == receipt {
                if *tool_receipt.options() != options {
                    // ...but the options differ, we need to update the receipt.
                    installed_tools
                        .add_tool_receipt(&from.name, tool_receipt.clone().with_options(options))?;
                }

                // We're done, though we might need to update the receipt.
                writeln!(
                    printer.stderr(),
                    "`{from}` is already installed",
                    from = from.cyan()
                )?;

                return Ok(ExitStatus::Success);
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
        let environment = update_environment(
            environment,
            spec,
            &settings,
            &state,
            Box::new(DefaultResolveLogger),
            Box::new(DefaultInstallLogger),
            connectivity,
            concurrency,
            native_tls,
            allow_insecure_host,
            &cache,
            printer,
        )
        .await?
        .into_environment();

        // At this point, we updated the existing environment, so we should remove any of its
        // existing executables.
        if let Some(existing_receipt) = existing_tool_receipt {
            remove_entrypoints(&existing_receipt);
        }

        environment
    } else {
        // If we're creating a new environment, ensure that we can resolve the requirements prior
        // to removing any existing tools.
        let resolution = resolve_environment(
            EnvironmentSpecification::from(spec),
            &interpreter,
            settings.as_ref().into(),
            &state,
            Box::new(DefaultResolveLogger),
            connectivity,
            concurrency,
            native_tls,
            allow_insecure_host,
            &cache,
            printer,
        )
        .await?;

        let environment = installed_tools.create_environment(&from.name, interpreter)?;

        // At this point, we removed any existing environment, so we should remove any of its
        // executables.
        if let Some(existing_receipt) = existing_tool_receipt {
            remove_entrypoints(&existing_receipt);
        }

        // Sync the environment with the resolved requirements.
        sync_environment(
            environment,
            &resolution.into(),
            settings.as_ref().into(),
            &state,
            Box::new(DefaultInstallLogger),
            connectivity,
            concurrency,
            native_tls,
            allow_insecure_host,
            &cache,
            printer,
        )
        .await
        .inspect_err(|_| {
            // If we failed to sync, remove the newly created environment.
            debug!("Failed to sync environment; removing `{}`", from.name);
            let _ = installed_tools.remove_environment(&from.name);
        })?
    };

    install_executables(
        &environment,
        &from.name,
        &installed_tools,
        options,
        force || invalid_tool_receipt,
        python,
        requirements,
        printer,
    )
}

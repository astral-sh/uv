use std::collections::BTreeMap;
use std::fmt::Write;
use std::str::FromStr;

use anyhow::{bail, Result};
use owo_colors::OwoColorize;
use tracing::{debug, trace};

use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_client::BaseClientBuilder;
use uv_configuration::{Concurrency, DryRun, PreviewMode, Reinstall, Upgrade};
use uv_distribution_types::{NameRequirementSpecification, UnresolvedRequirementSpecification};
use uv_normalize::PackageName;
use uv_pep440::{VersionSpecifier, VersionSpecifiers};
use uv_pep508::MarkerTree;
use uv_pypi_types::{Requirement, RequirementSource};
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonInstallation, PythonPreference, PythonRequest,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_settings::{PythonInstallMirrors, ResolverInstallerOptions, ToolOptions};
use uv_tool::InstalledTools;
use uv_warnings::warn_user;
use uv_workspace::WorkspaceCache;

use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger};
use crate::commands::pip::operations::Modifications;
use crate::commands::project::{
    resolve_environment, resolve_names, sync_environment, update_environment,
    EnvironmentSpecification, PlatformState, ProjectError,
};
use crate::commands::tool::common::{install_executables, refine_interpreter, remove_entrypoints};
use crate::commands::tool::{Target, ToolRequest};
use crate::commands::ExitStatus;
use crate::commands::{diagnostics, reporters::PythonDownloadReporter};
use crate::printer::Printer;
use crate::settings::{NetworkSettings, ResolverInstallerSettings, ResolverSettings};

/// Install a tool.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn install(
    package: String,
    editable: bool,
    from: Option<String>,
    with: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    force: bool,
    options: ResolverInstallerOptions,
    settings: ResolverInstallerSettings,
    network_settings: NetworkSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    cache: Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    let client_builder = BaseClientBuilder::new()
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .allow_insecure_host(network_settings.allow_insecure_host.clone());

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
        install_mirrors.python_install_mirror.as_deref(),
        install_mirrors.pypy_install_mirror.as_deref(),
    )
    .await?
    .into_interpreter();

    // Initialize any shared state.
    let state = PlatformState::default();
    let workspace_cache = WorkspaceCache::default();

    let client_builder = BaseClientBuilder::new()
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .allow_insecure_host(network_settings.allow_insecure_host.clone());

    // Parse the input requirement.
    let request = ToolRequest::parse(&package, from.as_deref());

    // If the user passed, e.g., `ruff@latest`, refresh the cache.
    let cache = if request.is_latest() {
        cache.with_refresh(Refresh::All(Timestamp::now()))
    } else {
        cache
    };

    // Resolve the `--from` requirement.
    let from = match &request.target {
        // Ex) `ruff`
        Target::Unspecified(from) => {
            let source = if editable {
                RequirementsSource::from_editable(from)?
            } else {
                RequirementsSource::from_package(from)?
            };
            let requirement = RequirementsSpecification::from_source(&source, &client_builder)
                .await?
                .requirements;

            // If the user provided an executable name, verify that it matches the `--from` requirement.
            let executable = if let Some(executable) = request.executable {
                let Ok(executable) = PackageName::from_str(executable) else {
                    bail!("Package requirement (`{from}`) provided with `--from` conflicts with install request (`{executable}`)", from = from.cyan(), executable = executable.cyan())
                };
                Some(executable)
            } else {
                None
            };

            let requirement = resolve_names(
                requirement,
                &interpreter,
                &settings,
                &network_settings,
                &state,
                concurrency,
                &cache,
                &workspace_cache,
                printer,
                preview,
            )
            .await?
            .pop()
            .unwrap();

            // Determine if it's an entirely different package (e.g., `uv install foo --from bar`).
            if let Some(executable) = executable {
                if requirement.name != executable {
                    bail!(
                        "Package name (`{}`) provided with `--from` does not match install request (`{}`)",
                        requirement.name.cyan(),
                        executable.cyan()
                    );
                }
            }

            requirement
        }
        // Ex) `ruff@0.6.0`
        Target::Version(.., name, ref extras, ref version) => {
            if editable {
                bail!("`--editable` is only supported for local packages");
            }

            Requirement {
                name: name.clone(),
                extras: extras.clone(),
                groups: vec![],
                marker: MarkerTree::default(),
                source: RequirementSource::Registry {
                    specifier: VersionSpecifiers::from(VersionSpecifier::equals_version(
                        version.clone(),
                    )),
                    index: None,
                    conflict: None,
                },
                origin: None,
            }
        }
        // Ex) `ruff@latest`
        Target::Latest(.., name, ref extras) => {
            if editable {
                bail!("`--editable` is only supported for local packages");
            }

            Requirement {
                name: name.clone(),
                extras: extras.clone(),
                groups: vec![],
                marker: MarkerTree::default(),
                source: RequirementSource::Registry {
                    specifier: VersionSpecifiers::empty(),
                    index: None,
                    conflict: None,
                },
                origin: None,
            }
        }
    };

    if from.name.as_str().eq_ignore_ascii_case("python") {
        return Err(anyhow::anyhow!(
            "Cannot install Python with `{}`. Did you mean to use `{}`?",
            "uv tool install".cyan(),
            "uv python install".cyan(),
        ));
    }

    // If the user passed, e.g., `ruff@latest`, we need to mark it as upgradable.
    let settings = if request.is_latest() {
        ResolverInstallerSettings {
            resolver: ResolverSettings {
                upgrade: settings
                    .resolver
                    .upgrade
                    .combine(Upgrade::package(from.name.clone())),
                ..settings.resolver
            },
            ..settings
        }
    } else {
        settings
    };

    // If the user passed `--force`, it implies `--reinstall-package <from>`
    let settings = if force {
        ResolverInstallerSettings {
            reinstall: settings
                .reinstall
                .combine(Reinstall::package(from.name.clone())),
            ..settings
        }
    } else {
        settings
    };

    // Read the `--with` requirements.
    let spec = RequirementsSpecification::from_sources(
        with,
        constraints,
        overrides,
        BTreeMap::default(),
        &client_builder,
    )
    .await?;

    // Resolve the `--from` and `--with` requirements.
    let requirements = {
        let mut requirements = Vec::with_capacity(1 + with.len());
        requirements.push(from.clone());
        requirements.extend(
            resolve_names(
                spec.requirements.clone(),
                &interpreter,
                &settings,
                &network_settings,
                &state,
                concurrency,
                &cache,
                &workspace_cache,
                printer,
                preview,
            )
            .await?,
        );
        requirements
    };

    // Resolve the constraints.
    let constraints = spec
        .constraints
        .into_iter()
        .map(|constraint| constraint.requirement)
        .collect::<Vec<_>>();

    // Resolve the overrides.
    let overrides = resolve_names(
        spec.overrides,
        &interpreter,
        &settings,
        &network_settings,
        &state,
        concurrency,
        &cache,
        &workspace_cache,
        printer,
        preview,
    )
    .await?;

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
            !request.is_latest()
                && settings.reinstall.is_none()
                && settings.resolver.upgrade.is_none()
        })
        .is_some()
    {
        if let Some(tool_receipt) = existing_tool_receipt.as_ref() {
            if requirements == tool_receipt.requirements()
                && constraints == tool_receipt.constraints()
                && overrides == tool_receipt.overrides()
            {
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
        constraints: constraints
            .iter()
            .cloned()
            .map(NameRequirementSpecification::from)
            .collect(),
        overrides: overrides
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
        let environment = match update_environment(
            environment,
            spec,
            Modifications::Exact,
            &settings,
            &network_settings,
            &state,
            Box::new(DefaultResolveLogger),
            Box::new(DefaultInstallLogger),
            installer_metadata,
            concurrency,
            &cache,
            workspace_cache,
            DryRun::Disabled,
            printer,
            preview,
        )
        .await
        {
            Ok(update) => update.into_environment(),
            Err(ProjectError::Operation(err)) => {
                return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                    .report(err)
                    .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
            }
            Err(err) => return Err(err.into()),
        };

        // At this point, we updated the existing environment, so we should remove any of its
        // existing executables.
        if let Some(existing_receipt) = existing_tool_receipt {
            remove_entrypoints(&existing_receipt);
        }

        environment
    } else {
        let spec = EnvironmentSpecification::from(spec);

        // If we're creating a new environment, ensure that we can resolve the requirements prior
        // to removing any existing tools.
        let resolution = resolve_environment(
            spec.clone(),
            &interpreter,
            &settings.resolver,
            &network_settings,
            &state,
            Box::new(DefaultResolveLogger),
            concurrency,
            &cache,
            printer,
            preview,
        )
        .await;

        // If the resolution failed, retry with the inferred `requires-python` constraint.
        let (resolution, interpreter) = match resolution {
            Ok(resolution) => (resolution, interpreter),
            Err(err) => match err {
                ProjectError::Operation(err) => {
                    // If the resolution failed due to the discovered interpreter not satisfying the
                    // `requires-python` constraint, we can try to refine the interpreter.
                    //
                    // For example, if we discovered a Python 3.8 interpreter on the user's machine,
                    // but the tool requires Python 3.10 or later, we can try to download a
                    // Python 3.10 interpreter and re-resolve.
                    let Some(interpreter) = refine_interpreter(
                        &interpreter,
                        python_request.as_ref(),
                        &err,
                        &client_builder,
                        &reporter,
                        &install_mirrors,
                        python_preference,
                        python_downloads,
                        &cache,
                    )
                    .await
                    .ok()
                    .flatten() else {
                        return diagnostics::OperationDiagnostic::native_tls(
                            network_settings.native_tls,
                        )
                        .report(err)
                        .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
                    };

                    debug!(
                        "Re-resolving with Python {} (`{}`)",
                        interpreter.python_version(),
                        interpreter.sys_executable().display()
                    );

                    match resolve_environment(
                        spec,
                        &interpreter,
                        &settings.resolver,
                        &network_settings,
                        &state,
                        Box::new(DefaultResolveLogger),
                        concurrency,
                        &cache,
                        printer,
                        preview,
                    )
                    .await
                    {
                        Ok(resolution) => (resolution, interpreter),
                        Err(ProjectError::Operation(err)) => {
                            return diagnostics::OperationDiagnostic::native_tls(
                                network_settings.native_tls,
                            )
                            .report(err)
                            .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
                        }
                        Err(err) => return Err(err.into()),
                    }
                }
                err => return Err(err.into()),
            },
        };

        let environment = installed_tools.create_environment(&from.name, interpreter)?;

        // At this point, we removed any existing environment, so we should remove any of its
        // executables.
        if let Some(existing_receipt) = existing_tool_receipt {
            remove_entrypoints(&existing_receipt);
        }

        // Sync the environment with the resolved requirements.
        match sync_environment(
            environment,
            &resolution.into(),
            Modifications::Exact,
            (&settings).into(),
            &network_settings,
            &state,
            Box::new(DefaultInstallLogger),
            installer_metadata,
            concurrency,
            &cache,
            printer,
            preview,
        )
        .await
        .inspect_err(|_| {
            // If we failed to sync, remove the newly created environment.
            debug!("Failed to sync environment; removing `{}`", from.name);
            let _ = installed_tools.remove_environment(&from.name);
        }) {
            Ok(environment) => environment,
            Err(ProjectError::Operation(err)) => {
                return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                    .report(err)
                    .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
            }
            Err(err) => return Err(err.into()),
        }
    };

    install_executables(
        &environment,
        &from.name,
        &installed_tools,
        options,
        force || invalid_tool_receipt,
        python,
        requirements,
        constraints,
        overrides,
        printer,
    )
}

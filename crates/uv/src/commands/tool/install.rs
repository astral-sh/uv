use std::fmt::Write;
use std::str::FromStr;

use anyhow::{Result, bail};
use owo_colors::OwoColorize;
use tracing::{debug, trace};

use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, Constraints, DryRun, GitLfsSetting, Reinstall, TargetTriple, Upgrade,
};
use uv_distribution::LoweredExtraBuildDependencies;
use uv_distribution_types::{
    ExtraBuildRequires, IndexCapabilities, NameRequirementSpecification, Requirement,
    RequirementSource, Resolution, UnresolvedRequirement, UnresolvedRequirementSpecification,
};
use uv_installer::{InstallationStrategy, SatisfiesResult, SitePackages};
use uv_normalize::PackageName;
use uv_pep440::{VersionSpecifier, VersionSpecifiers};
use uv_pep508::MarkerTree;
use uv_preview::Preview;
use uv_python::{
    EnvironmentPreference, Interpreter, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_settings::{PythonInstallMirrors, ResolverInstallerOptions, ToolOptions};
use uv_tool::InstalledTools;
use uv_types::SourceTreeEditablePolicy;
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::WorkspaceCache;

use crate::commands::ExitStatus;
use crate::commands::pip::latest::LatestClient;
use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger};
use crate::commands::pip::operations::{self, Modifications};
use crate::commands::pip::{resolution_markers, resolution_tags};
use crate::commands::project::{
    EnvironmentSpecification, PlatformState, ProjectError, resolve_environment, resolve_names,
    sync_environment, update_environment,
};
use crate::commands::tool::common::{
    ToolPython, finalize_tool_install, refine_interpreter, remove_entrypoints,
    resolve_registry_tool,
};
use crate::commands::tool::{Target, ToolRequest};
use crate::commands::{diagnostics, reporters::PythonDownloadReporter};
use crate::printer::Printer;
use crate::settings::{ResolverInstallerSettings, ResolverSettings};

/// Install a tool.
#[expect(clippy::fn_params_excessive_bools)]
pub(crate) async fn install(
    package: String,
    editable: bool,
    from: Option<String>,
    with: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    excludes: &[RequirementsSource],
    build_constraints: &[RequirementsSource],
    entrypoints: &[PackageName],
    lfs: GitLfsSetting,
    python: Option<String>,
    python_platform: Option<TargetTriple>,
    install_mirrors: PythonInstallMirrors,
    force: bool,
    options: ResolverInstallerOptions,
    settings: ResolverInstallerSettings,
    client_builder: BaseClientBuilder<'_>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    no_config: bool,
    cache: Cache,
    refresh: Refresh,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    Box::pin(install_inner(
        package,
        editable,
        from,
        with,
        constraints,
        overrides,
        excludes,
        build_constraints,
        entrypoints,
        lfs,
        python,
        python_platform,
        install_mirrors,
        force,
        options,
        settings,
        client_builder,
        python_preference,
        python_downloads,
        installer_metadata,
        concurrency,
        no_config,
        cache,
        refresh,
        workspace_cache,
        printer,
        preview,
    ))
    .await
}

#[expect(clippy::fn_params_excessive_bools)]
async fn install_inner(
    package: String,
    editable: bool,
    from: Option<String>,
    with: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    excludes: &[RequirementsSource],
    build_constraints: &[RequirementsSource],
    entrypoints: &[PackageName],
    lfs: GitLfsSetting,
    python: Option<String>,
    python_platform: Option<TargetTriple>,
    install_mirrors: PythonInstallMirrors,
    force: bool,
    options: ResolverInstallerOptions,
    settings: ResolverInstallerSettings,
    client_builder: BaseClientBuilder<'_>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    no_config: bool,
    cache: Cache,
    refresh: Refresh,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    if settings.resolver.torch_backend.is_some() {
        warn_user_once!(
            "The `--torch-backend` option is experimental and may change without warning."
        );
    }

    let reporter = PythonDownloadReporter::single(printer);

    // Initialize any shared state.
    let state = PlatformState::default();

    // Parse the input requirement.
    let request = ToolRequest::parse(&package, from.as_deref())?;

    // If the user passed, e.g., `ruff@latest`, refresh the cache.
    let refresh = if request.is_latest() {
        refresh.combine(Refresh::All(Timestamp::now()))
    } else {
        refresh
    };
    let cache = cache.with_refresh(refresh.clone());

    let unresolved_target_requirements = match &request {
        ToolRequest::Package {
            target: Target::Unspecified(requirement),
            ..
        } => {
            let source = if editable {
                RequirementsSource::from_editable(requirement)?
            } else {
                RequirementsSource::from_package(requirement)?
            };
            Some(
                RequirementsSpecification::from_source(&source, &client_builder)
                    .await?
                    .requirements,
            )
        }
        _ => None,
    };
    let has_registry_target = match &request {
        ToolRequest::Package {
            target: Target::Version(..) | Target::Latest(..),
            ..
        } => true,
        ToolRequest::Package {
            target: Target::Unspecified(_),
            ..
        } => unresolved_target_requirements
            .as_ref()
            .is_some_and(|requirements| {
                requirements.first().is_some_and(|requirement| {
                    matches!(
                        &requirement.requirement,
                        UnresolvedRequirement::Named(requirement)
                            if matches!(&requirement.source, RequirementSource::Registry { .. })
                    )
                })
            }),
        ToolRequest::Python { .. } => false,
    };

    let tool_python = ToolPython::from_request(
        python.as_deref().map(PythonRequest::parse),
        unresolved_target_requirements
            .as_ref()
            .and_then(|requirements| requirements.first())
            .map(|requirement| &requirement.requirement),
        no_config,
        lfs,
        state.git(),
        &client_builder,
        &cache,
    )
    .await?;
    let explicit_python_request = tool_python.is_explicit();
    let python_request = tool_python.python_request;

    // Pre-emptively identify a Python interpreter. We need an interpreter to resolve any unnamed
    // requirements, even if we end up using a different interpreter for the tool install itself.
    let mut interpreter = PythonInstallation::find_or_download(
        python_request.as_ref(),
        EnvironmentPreference::OnlySystem,
        python_preference,
        python_downloads,
        &client_builder,
        &cache,
        Some(&reporter),
        install_mirrors.python_install_mirror.as_deref(),
        install_mirrors.pypy_install_mirror.as_deref(),
        install_mirrors.python_downloads_json_url.as_deref(),
    )
    .await?
    .into_interpreter();

    // Resolve the `--from` requirement.
    let requirement = match &request {
        // Ex) `ruff`
        ToolRequest::Package {
            executable,
            target: Target::Unspecified(from),
        } => {
            let requirements = unresolved_target_requirements.clone().ok_or_else(|| {
                anyhow::anyhow!("Expected parsed requirements for unresolved target `{from}`")
            })?;

            // If the user provided an executable name, verify that it matches the `--from` requirement.
            let executable = if let Some(executable) = executable {
                let Ok(executable) = PackageName::from_str(executable) else {
                    bail!(
                        "Package requirement (`{from}`) provided with `--from` conflicts with install request (`{executable}`)",
                        from = from.cyan(),
                        executable = executable.cyan()
                    )
                };
                Some(executable)
            } else {
                None
            };

            let requirement = resolve_names(
                requirements,
                &interpreter,
                &settings,
                &client_builder,
                &state,
                &concurrency,
                &cache,
                workspace_cache,
                printer,
                preview,
                lfs,
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
        ToolRequest::Package {
            target: Target::Version(.., name, extras, version),
            ..
        } => {
            if editable {
                bail!("`--editable` is only supported for local packages");
            }

            Requirement {
                name: name.clone(),
                extras: extras.clone(),
                groups: Box::new([]),
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
        ToolRequest::Package {
            target: Target::Latest(.., name, extras),
            ..
        } => {
            if editable {
                bail!("`--editable` is only supported for local packages");
            }

            Requirement {
                name: name.clone(),
                extras: extras.clone(),
                groups: Box::new([]),
                marker: MarkerTree::default(),
                source: RequirementSource::Registry {
                    specifier: VersionSpecifiers::empty(),
                    index: None,
                    conflict: None,
                },
                origin: None,
            }
        }
        // Ex) `python`
        ToolRequest::Python { .. } => {
            bail!(
                "Cannot install Python with `{}`. Did you mean to use `{}`?",
                "uv tool install".cyan(),
                "uv python install".cyan(),
            );
        }
    };

    // For `@latest`, fetch the latest version and create a constraint.
    let latest = if let ToolRequest::Package {
        target: Target::Latest(_, name, _),
        ..
    } = &request
    {
        // Build the registry client to fetch the latest version.
        let client = RegistryClientBuilder::new(
            client_builder
                .clone()
                .keyring(settings.resolver.keyring_provider),
            cache.clone(),
        )
        .index_locations(settings.resolver.index_locations.clone())
        .index_strategy(settings.resolver.index_strategy)
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build()?;

        // Initialize the capabilities.
        let capabilities = IndexCapabilities::default();
        let download_concurrency = concurrency.downloads_semaphore.clone();

        // Initialize the client to fetch the latest version.
        let latest_client = LatestClient {
            client: &client,
            capabilities: &capabilities,
            prerelease: settings.resolver.prerelease,
            exclude_newer: &settings.resolver.exclude_newer,
            index_locations: &settings.resolver.index_locations,
            tags: None,
            requires_python: None,
        };

        // Fetch the latest version.
        if let Some(dist_filename) = latest_client
            .find_latest(name, None, &download_concurrency)
            .await?
        {
            let version = dist_filename.version().clone();
            debug!("Resolved `{name}@latest` to `{name}=={version}`");

            // The constraint pins the version during resolution to prevent backtracking.
            Some(Requirement {
                name: name.clone(),
                extras: vec![].into_boxed_slice(),
                groups: Box::new([]),
                marker: MarkerTree::default(),
                source: RequirementSource::Registry {
                    specifier: VersionSpecifiers::from(VersionSpecifier::equals_version(version)),
                    index: None,
                    conflict: None,
                },
                origin: None,
            })
        } else {
            None
        }
    } else {
        None
    };

    let package_name = &requirement.name;

    // If the user passed, e.g., `ruff@latest`, we need to mark it as upgradable.
    let settings = if request.is_latest() {
        ResolverInstallerSettings {
            resolver: ResolverSettings {
                upgrade: Upgrade::package(package_name.clone()).combine(settings.resolver.upgrade),
                ..settings.resolver
            },
            ..settings
        }
    } else {
        settings
    };

    // If the user passed `--force`, it implies `--reinstall-package <from>`.
    let settings = if force {
        ResolverInstallerSettings {
            reinstall: Reinstall::package(package_name.clone()).combine(settings.reinstall),
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
        excludes,
        None,
        &client_builder,
    )
    .await?;

    // Resolve the `--from` and `--with` requirements.
    let requirements = {
        let mut requirements = Vec::with_capacity(1 + with.len());
        requirements.push(requirement.clone());
        requirements.extend(
            resolve_names(
                spec.requirements.clone(),
                &interpreter,
                &settings,
                &client_builder,
                &state,
                &concurrency,
                &cache,
                workspace_cache,
                printer,
                preview,
                lfs,
            )
            .await?,
        );
        requirements
    };

    // Explicit local directory requirements should always be rebuilt and reinstalled, matching
    // `uv pip install`. At this point, all unnamed requirements have been resolved to package names,
    // including any requirements provided via `--with`.
    let explicit_local_packages = requirements
        .iter()
        .filter(|requirement| {
            requirement.origin.is_none()
                && matches!(requirement.source, RequirementSource::Directory { .. })
        })
        .map(|requirement| requirement.name.clone())
        .collect::<Vec<_>>();
    let (settings, cache) = if explicit_local_packages.is_empty() {
        (settings, cache)
    } else {
        let reinstall = explicit_local_packages
            .into_iter()
            .fold(Reinstall::None, |reinstall, package_name| {
                reinstall.with_package(package_name)
            })
            .combine(settings.reinstall);
        let cache = cache.with_refresh(refresh.combine(Refresh::from(reinstall.clone())));
        (
            ResolverInstallerSettings {
                reinstall,
                ..settings
            },
            cache,
        )
    };

    // Resolve the constraints.
    let constraints: Vec<_> = spec
        .constraints
        .into_iter()
        .map(|constraint| constraint.requirement)
        .collect();

    // Resolve the overrides.
    let overrides = resolve_names(
        spec.overrides,
        &interpreter,
        &settings,
        &client_builder,
        &state,
        &concurrency,
        &cache,
        workspace_cache,
        printer,
        preview,
        lfs,
    )
    .await?;

    // Resolve the excludes.
    let excludes = spec.excludes.clone();

    // Resolve the build constraints.
    let build_constraints: Vec<Requirement> =
        operations::read_constraints(build_constraints, &client_builder)
            .await?
            .into_iter()
            .map(|constraint| constraint.requirement)
            .collect();

    // Convert to tool options.
    let tool_options = ToolOptions::from(options.clone());

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
        match installed_tools.get_tool_receipt(package_name) {
            Ok(None) => (None, false),
            Ok(Some(receipt)) => (Some(receipt), false),
            Err(_) => {
                // If the tool is not installed properly, remove the environment and continue.
                match installed_tools.remove_environment(package_name) {
                    Ok(()) => {
                        warn_user!(
                            "Removed existing `{}` with invalid receipt",
                            package_name.cyan()
                        );
                    }
                    Err(err)
                        if err
                            .as_io_error()
                            .is_some_and(|err| err.kind() == std::io::ErrorKind::NotFound) => {}
                    Err(err) => {
                        return Err(err.into());
                    }
                }
                (None, true)
            }
        };
    let mut existing_environment = if force {
        None
    } else {
        installed_tools
            .get_environment(package_name, &cache)?
            .filter(|environment| {
                existing_environment_usable(
                    environment.environment(),
                    &interpreter,
                    package_name,
                    explicit_python_request,
                    &settings,
                    existing_tool_receipt.as_ref(),
                    printer,
                )
            })
    };

    // If the requested and receipt requirements are the same...
    if let Some(environment) = existing_environment.as_ref().filter(|_| {
        // And the user didn't request a reinstall or upgrade...
        !request.is_latest() && settings.reinstall.is_none() && settings.resolver.upgrade.is_none()
    }) {
        if let Some(tool_receipt) = existing_tool_receipt.as_ref() {
            if requirements == tool_receipt.requirements()
                && constraints == tool_receipt.constraints()
                && overrides == tool_receipt.overrides()
                && build_constraints == tool_receipt.build_constraints()
            {
                let ResolverInstallerSettings {
                    resolver:
                        ResolverSettings {
                            config_setting,
                            config_settings_package,
                            extra_build_dependencies,
                            extra_build_variables,
                            ..
                        },
                    ..
                } = &settings;

                // Lower the extra build dependencies, if any.
                let extra_build_requires = LoweredExtraBuildDependencies::from_non_lowered(
                    extra_build_dependencies.clone(),
                )
                .into_inner();

                // Determine the markers and tags to use for the resolution. We use the existing
                // environment for markers here — above we filter the environment to `None` if
                // `existing_environment_usable` is `false`, so we've determined it's valid.
                let markers = resolution_markers(
                    None,
                    python_platform.as_ref(),
                    environment.environment().interpreter(),
                );
                let tags = resolution_tags(
                    None,
                    python_platform.as_ref(),
                    environment.environment().interpreter(),
                )?;

                // Check if the installed packages meet the requirements.
                let site_packages = SitePackages::from_environment(environment.environment())?;
                // TODO(charlie): This fast path only validates the explicit requested
                // requirements. It can miss editable-mode drift for implicit workspace members.
                if matches!(
                    site_packages.satisfies_requirements(
                        requirements.iter(),
                        constraints.iter().chain(latest.iter()),
                        overrides.iter(),
                        InstallationStrategy::Permissive,
                        &markers,
                        &tags,
                        config_setting,
                        config_settings_package,
                        &extra_build_requires,
                        extra_build_variables,
                    ),
                    Ok(SatisfiesResult::Fresh { .. })
                ) {
                    // Then we're done! Though we might need to update the receipt.
                    if *tool_receipt.options() != tool_options {
                        installed_tools.add_tool_receipt(
                            package_name,
                            tool_receipt.clone().with_options(tool_options),
                        )?;
                    }

                    writeln!(
                        printer.stderr(),
                        "`{}` is already installed",
                        requirement.cyan()
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
        constraints: constraints
            .iter()
            .cloned()
            .chain(latest)
            .map(NameRequirementSpecification::from)
            .collect(),
        overrides: overrides
            .iter()
            .cloned()
            .map(UnresolvedRequirementSpecification::from)
            .collect(),
        ..spec
    };

    let mut registry_resolution = None;
    if has_registry_target
        && !explicit_python_request
        && (existing_environment.is_none()
            || request.is_latest()
            || !settings.resolver.upgrade.is_none())
    {
        // Prefer the existing tool interpreter when multiple Python targets produce the same
        // solution. This preserves the established update-in-place behavior while still allowing
        // a higher tool version to select a different interpreter.
        let registry_interpreter = existing_environment
            .as_ref()
            .map(|environment| environment.environment().interpreter())
            .unwrap_or(&interpreter)
            .clone();
        let selected = match Box::pin(resolve_registry_tool(
            package_name,
            EnvironmentSpecification::from(spec.clone()),
            &registry_interpreter,
            python_platform.as_ref(),
            SourceTreeEditablePolicy::Tool,
            Constraints::from_requirements(build_constraints.iter().cloned()),
            &settings,
            &client_builder,
            &state,
            &reporter,
            &install_mirrors,
            python_preference,
            python_downloads,
            &concurrency,
            &cache,
            workspace_cache,
            Box::new(DefaultResolveLogger),
            printer,
            preview,
        ))
        .await
        {
            Ok(selected) => selected,
            Err(ProjectError::Operation(err)) => {
                return diagnostics::OperationDiagnostic::with_system_certs(
                    client_builder.system_certs(),
                )
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
            }
            Err(err) => return Err(err.into()),
        };
        if let Some(selected) = selected {
            let reuses_existing_environment = existing_environment
                .as_ref()
                .is_some_and(|environment| environment.environment().uses(&selected.interpreter));
            if !reuses_existing_environment {
                existing_environment = None;
                registry_resolution = Some(selected.resolution);
            }
            interpreter = selected.interpreter;
        }
    }

    // TODO(zanieb): Build the environment in the cache directory then copy into the tool directory.
    // This lets us confirm the environment is valid before removing an existing install. However,
    // entrypoints always contain an absolute path to the relevant Python interpreter, which would
    // be invalidated by moving the environment.
    let environment = if let Some(environment) = existing_environment {
        let environment = update_environment(
            environment.into_environment(),
            spec,
            Modifications::Exact,
            python_platform.as_ref(),
            SourceTreeEditablePolicy::Tool,
            Constraints::from_requirements(build_constraints.iter().cloned()),
            ExtraBuildRequires::default(),
            &settings,
            &client_builder,
            &state,
            Box::new(DefaultResolveLogger),
            Box::new(DefaultInstallLogger),
            installer_metadata,
            &concurrency,
            &cache,
            workspace_cache,
            DryRun::Disabled,
            printer,
            preview,
        )
        .await
        .map(super::super::project::EnvironmentUpdate::into_environment);
        let environment = match environment {
            Ok(environment) => environment,
            Err(ProjectError::Operation(err)) => {
                return diagnostics::OperationDiagnostic::with_system_certs(
                    client_builder.system_certs(),
                )
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
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
        let resolution = if let Some(resolution) = registry_resolution.take() {
            Ok(resolution)
        } else {
            resolve_environment(
                spec.clone(),
                &interpreter,
                python_platform.as_ref(),
                SourceTreeEditablePolicy::Tool,
                Constraints::from_requirements(build_constraints.iter().cloned()),
                &settings.resolver,
                &client_builder,
                &state,
                Box::new(DefaultResolveLogger),
                &concurrency,
                &cache,
                workspace_cache,
                printer,
                preview,
            )
            .await
            .map(Resolution::from)
        };

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
                        return diagnostics::OperationDiagnostic::with_system_certs(
                            client_builder.system_certs(),
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
                        python_platform.as_ref(),
                        SourceTreeEditablePolicy::Tool,
                        Constraints::from_requirements(build_constraints.iter().cloned()),
                        &settings.resolver,
                        &client_builder,
                        &state,
                        Box::new(DefaultResolveLogger),
                        &concurrency,
                        &cache,
                        workspace_cache,
                        printer,
                        preview,
                    )
                    .await
                    {
                        Ok(resolution) => (Resolution::from(resolution), interpreter),
                        Err(ProjectError::Operation(err)) => {
                            return diagnostics::OperationDiagnostic::with_system_certs(
                                client_builder.system_certs(),
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

        let environment = installed_tools.create_environment(package_name, interpreter)?;

        // At this point, we removed any existing environment, so we should remove any of its
        // executables.
        if let Some(existing_receipt) = existing_tool_receipt {
            remove_entrypoints(&existing_receipt);
        }

        // Sync the environment with the resolved requirements.
        match sync_environment(
            environment,
            &resolution,
            Modifications::Exact,
            Constraints::from_requirements(build_constraints.iter().cloned()),
            (&settings).into(),
            &client_builder,
            &state,
            Box::new(DefaultInstallLogger),
            installer_metadata,
            &concurrency,
            &cache,
            printer,
            preview,
        )
        .await
        .inspect_err(|_| {
            // If we failed to sync, remove the newly created environment.
            debug!("Failed to sync environment; removing `{}`", package_name);
            let _ = installed_tools.remove_environment(package_name);
        }) {
            Ok(environment) => environment,
            Err(ProjectError::Operation(err)) => {
                return diagnostics::OperationDiagnostic::with_system_certs(
                    client_builder.system_certs(),
                )
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
            }
            Err(err) => return Err(err.into()),
        }
    };

    finalize_tool_install(
        &environment,
        package_name,
        entrypoints,
        &installed_tools,
        &tool_options,
        force || invalid_tool_receipt,
        // Only persist the Python request if it was explicitly provided
        if explicit_python_request {
            python_request
        } else {
            None
        },
        requirements,
        constraints,
        overrides,
        excludes,
        build_constraints,
        printer,
    )?;

    Ok(ExitStatus::Success)
}

fn existing_environment_usable(
    environment: &PythonEnvironment,
    interpreter: &Interpreter,
    package_name: &PackageName,
    explicit_python_request: bool,
    settings: &ResolverInstallerSettings,
    existing_tool_receipt: Option<&uv_tool::Tool>,
    printer: Printer,
) -> bool {
    // If the environment matches the interpreter, it's usable
    if environment.uses(interpreter) {
        trace!(
            "Existing interpreter matches the requested interpreter for `{}`: {}",
            package_name,
            environment.interpreter().sys_executable().display()
        );
        return true;
    }

    // If there was an explicit Python request that does not match, we'll invalidate the
    // environment.
    if explicit_python_request {
        let _ = writeln!(
            printer.stderr(),
            "Ignoring existing environment for `{}`: the requested Python interpreter does not match the environment interpreter",
            package_name.cyan(),
        );
        return false;
    }

    // Otherwise, invalidate the environment when this tool is being reinstalled and its
    // previous receipt did not pin a Python request. In that case, the reinstall should
    // follow the newly selected interpreter instead of reusing the old environment.
    if let Some(tool_receipt) = existing_tool_receipt
        && settings.reinstall.contains_package(package_name)
        && tool_receipt.python().is_none()
    {
        let _ = writeln!(
            printer.stderr(),
            "Ignoring existing environment for `{from}`: the Python interpreter does not match the environment interpreter",
            from = package_name.cyan(),
        );
        return false;
    }

    true
}

use std::fmt::Write;
use std::str::FromStr;

use anyhow::{Result, bail};
use owo_colors::OwoColorize;
use tracing::{debug, trace};

use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, Constraints, DryRun, Excludes, GitLfsSetting, HashCheckingMode, Overrides,
    Reinstall, TargetTriple, Upgrade,
};
use uv_distribution::LoweredExtraBuildDependencies;
use uv_distribution_types::{
    ExtraBuildRequires, IndexCapabilities, NameRequirementSpecification, Requirement,
    RequirementSource, UnresolvedRequirementSpecification,
};
use uv_installer::{InstallationStrategy, Planner, SatisfiesResult, SitePackages};
use uv_normalize::PackageName;
use uv_pep440::{VersionSpecifier, VersionSpecifiers};
use uv_pep508::MarkerTree;
use uv_preview::{Preview, PreviewFeature};
use uv_python::{
    ConfigDiscovery, EnvironmentPreference, Interpreter, PythonDownloads, PythonEnvironment,
    PythonInstallation, PythonPreference, PythonRequest,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_settings::{PythonInstallMirrors, ResolverInstallerOptions, ToolOptions};
use uv_tool::{InstalledTools, Tool};
use uv_types::{HashStrategy, SourceTreeEditablePolicy};
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::WorkspaceCache;

use crate::commands::ExitStatus;
use crate::commands::pip::latest::LatestClient;
use crate::commands::pip::loggers::{
    DefaultInstallLogger, DefaultResolveLogger, SummaryResolveLogger,
};
use crate::commands::pip::operations::{self, Modifications};
use crate::commands::pip::{resolution_markers, resolution_tags};
use crate::commands::project::{
    EnvironmentResolution, EnvironmentSpecification, PlatformState, ProjectError,
    resolve_environment, resolve_names, sync_environment, update_environment,
};
use crate::commands::tool::common::{
    ToolLock, ToolPython, finalize_tool_install, refine_interpreter, remove_entrypoints,
    tool_environment_spec,
};
use crate::commands::tool::{Target, ToolRequest};
use crate::commands::{diagnostics, reporters::PythonDownloadReporter};
use crate::printer::Printer;
use crate::settings::{ResolverInstallerSettings, ResolverSettings};

/// Install a tool.
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
    config_discovery: ConfigDiscovery,
    cache: Cache,
    refresh: Refresh,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    let tool_locks = preview.is_enabled(PreviewFeature::ToolInstallLocks);
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

    let tool_python = ToolPython::from_request(
        python.as_deref().map(PythonRequest::parse),
        unresolved_target_requirements
            .as_ref()
            .and_then(|requirements| requirements.first())
            .map(|requirement| &requirement.requirement),
        config_discovery,
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
        install_mirrors.python_downloads_json_url.as_deref(),
    )
    .await?
    .into_interpreter();

    // If the user passed, e.g., `ruff@latest`, refresh the cache.
    let refresh = if request.is_latest() {
        refresh.combine(Refresh::All(Timestamp::now()))
    } else {
        refresh
    };
    let cache = cache.with_refresh(refresh.clone());

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
        let cache = cache.with_refresh(refresh.clone().combine(Refresh::from(reinstall.clone())));
        (
            ResolverInstallerSettings {
                reinstall,
                ..settings
            },
            cache,
        )
    };

    // Resolve the constraints.
    let receipt_constraints = spec
        .constraints
        .into_iter()
        .map(|constraint| constraint.requirement)
        .collect::<Vec<_>>();

    // Resolve the overrides.
    let receipt_overrides = resolve_names(
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
    let receipt_excludes = spec.excludes.clone();

    // Resolve the build constraints.
    let receipt_build_constraints =
        operations::read_constraints(build_constraints, &client_builder)
            .await?
            .into_iter()
            .map(|constraint| constraint.requirement)
            .collect::<Vec<_>>();

    // Convert to tool options.
    let options = ToolOptions::from(options);
    let lock_manifest = ToolLock::manifest(
        &requirements,
        &receipt_constraints,
        &receipt_overrides,
        &receipt_excludes,
        &receipt_build_constraints,
        &settings.resolver.dependency_metadata,
    );

    let installed_tools = InstalledTools::from_settings()?.init()?;
    let _lock = installed_tools.lock().await?;
    let tool_dir = installed_tools.tool_dir(package_name);

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

    let existing_environment = if force {
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

    let validation_interpreter = existing_environment
        .as_ref()
        .map_or(&interpreter, |environment| {
            environment.environment().interpreter()
        });
    let mut existing_tool_lock = if tool_locks {
        if let Some(lock) = ToolLock::read(&tool_dir) {
            match Box::pin(lock.validate(
                &requirements,
                &receipt_constraints,
                &receipt_overrides,
                &receipt_excludes,
                &receipt_build_constraints,
                &refresh,
                validation_interpreter,
                &settings.resolver,
                &client_builder,
                &state,
                &concurrency,
                &cache,
                workspace_cache,
                printer,
                preview,
            ))
            .await
            {
                Ok(lock) => Some(lock),
                Err(ProjectError::Lock(err)) if err.is_resolution() || err.is_no_build() => {
                    return Err(ProjectError::Lock(err).into());
                }
                Err(err) => {
                    warn_user!("Failed to validate existing tool lock: {err}");
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    // If the requested and receipt requirements are the same...
    if let Some(environment) = existing_environment.as_ref().filter(|_| {
        // And the user didn't request a reinstall or upgrade...
        !request.is_latest() && settings.reinstall.is_none() && settings.resolver.upgrade.is_none()
    }) {
        if let Some(tool_receipt) = existing_tool_receipt.as_ref() {
            if !tool_locks
                && requirements == tool_receipt.requirements()
                && receipt_constraints == tool_receipt.constraints()
                && receipt_overrides == tool_receipt.overrides()
                && receipt_excludes == tool_receipt.excludes()
                && receipt_build_constraints == tool_receipt.build_constraints()
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
                // This fast path only validates the explicitly requested requirements. It can miss
                // editable-mode drift for implicit workspace members.
                let already_installed = matches!(
                    site_packages.satisfies_requirements(
                        requirements.iter(),
                        receipt_constraints.iter().chain(latest.iter()),
                        &Overrides::from_requirements(receipt_overrides.clone()),
                        &Excludes::from_entries(receipt_excludes.iter().cloned()),
                        InstallationStrategy::Permissive,
                        &markers,
                        &tags,
                        config_setting,
                        config_settings_package,
                        &extra_build_requires,
                        extra_build_variables,
                    ),
                    Ok(SatisfiesResult::Fresh { .. })
                );
                if already_installed {
                    // Then we're done! Though we might need to update the receipt.
                    if *tool_receipt.options() != options {
                        installed_tools.add_tool_receipt(
                            package_name,
                            tool_receipt.clone().with_options(options),
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
        constraints: receipt_constraints
            .iter()
            .cloned()
            .chain(latest)
            .map(NameRequirementSpecification::from)
            .collect(),
        overrides: receipt_overrides
            .iter()
            .cloned()
            .map(UnresolvedRequirementSpecification::from)
            .collect(),
        excludes: receipt_excludes.clone(),
        ..spec
    };

    let resolution_scope = if tool_locks {
        EnvironmentResolution::Universal
    } else {
        EnvironmentResolution::Specific
    };

    // TODO(zanieb): Build the environment in the cache directory then copy into the tool directory.
    // This lets us confirm the environment is valid before removing an existing install. However,
    // entrypoints always contain an absolute path to the relevant Python interpreter, which would
    // be invalidated by moving the environment.
    let (environment, tool_lock) = if let Some(environment) = existing_environment {
        let environment = environment.into_environment();
        let (environment, tool_lock) = if tool_locks {
            let site_packages = SitePackages::from_environment(&environment)?;
            let satisfied_tool_lock = match existing_tool_lock.take() {
                Some(lock) if lock.is_satisfied() => Some(lock.into_lock()),
                lock => {
                    existing_tool_lock = lock;
                    None
                }
            };
            let (resolution, tool_lock) = if let Some(tool_lock) = satisfied_tool_lock {
                let resolution = tool_lock.to_resolution(
                    Some(package_name),
                    environment.interpreter(),
                    python_platform.as_ref(),
                    &settings.resolver.build_options,
                )?;
                (resolution, tool_lock)
            } else {
                let mut resolution = match resolve_environment(
                    tool_environment_spec(
                        spec.clone(),
                        existing_tool_lock
                            .as_ref()
                            .and_then(|lock| lock.preference()),
                        Some(&site_packages),
                    ),
                    resolution_scope,
                    environment.interpreter(),
                    python_platform.as_ref(),
                    SourceTreeEditablePolicy::Tool,
                    Constraints::from_requirements(receipt_build_constraints.iter().cloned()),
                    &settings.resolver,
                    &client_builder,
                    &state,
                    Box::new(SummaryResolveLogger),
                    &concurrency,
                    &cache,
                    workspace_cache,
                    printer,
                    preview,
                )
                .await
                {
                    Ok(resolution) => resolution,
                    Err(ProjectError::Operation(err)) => {
                        return diagnostics::OperationDiagnostic::default()
                            .report(err)
                            .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
                    }
                    Err(err) => return Err(err.into()),
                };
                resolution.canonicalize_proxy_artifact_urls_for_lock(
                    &settings.resolver.index_locations,
                    &[],
                )?;
                let tool_lock = ToolLock::from_resolution(
                    &tool_dir,
                    &resolution,
                    &lock_manifest,
                    &settings.resolver.index_locations,
                )?;
                let resolution = tool_lock.to_resolution(
                    Some(package_name),
                    environment.interpreter(),
                    python_platform.as_ref(),
                    &settings.resolver.build_options,
                )?;
                (resolution, tool_lock)
            };

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
            let extra_build_requires =
                LoweredExtraBuildDependencies::from_non_lowered(extra_build_dependencies.clone())
                    .into_inner();
            let tags = resolution_tags(None, python_platform.as_ref(), environment.interpreter())?;
            let hash_strategy =
                HashStrategy::from_resolution(&resolution, HashCheckingMode::Verify)?;
            let plan = Planner::new(&resolution).build(
                site_packages,
                InstallationStrategy::Permissive,
                &settings.reinstall,
                &settings.resolver.build_options,
                &hash_strategy,
                &settings.resolver.index_locations,
                config_setting,
                config_settings_package,
                &extra_build_requires,
                extra_build_variables,
                &cache,
                &environment,
                &tags,
            )?;
            if plan.is_empty()
                && !settings.compile_bytecode
                && !request.is_latest()
                && settings.reinstall.is_none()
                && settings.resolver.upgrade.is_none()
            {
                let Some(existing_tool_receipt) = existing_tool_receipt.as_ref() else {
                    bail!("Expected an existing tool receipt");
                };
                let python = if explicit_python_request {
                    python_request.clone()
                } else {
                    existing_tool_receipt.python().clone()
                };
                ToolLock::write(&tool_dir, Some(&tool_lock))?;
                installed_tools.add_tool_receipt(
                    package_name,
                    Tool::new(
                        requirements.clone(),
                        receipt_constraints.clone(),
                        receipt_overrides.clone(),
                        receipt_excludes.clone(),
                        receipt_build_constraints.clone(),
                        python,
                        existing_tool_receipt.entrypoints().iter().cloned(),
                        options.clone(),
                    ),
                )?;
                writeln!(
                    printer.stderr(),
                    "`{}` is already installed",
                    requirement.cyan()
                )?;
                return Ok(ExitStatus::Success);
            }
            let environment = if plan.is_empty() && !settings.compile_bytecode {
                environment
            } else {
                sync_environment(
                    environment,
                    &resolution,
                    hash_strategy,
                    Modifications::Exact,
                    Constraints::from_requirements(receipt_build_constraints.iter().cloned()),
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
                .await?
            };
            (environment, Some(tool_lock))
        } else {
            let update = match update_environment(
                environment,
                spec,
                Modifications::Exact,
                python_platform.as_ref(),
                SourceTreeEditablePolicy::Tool,
                Constraints::from_requirements(receipt_build_constraints.iter().cloned()),
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
            {
                Ok(update) => update,
                Err(ProjectError::Operation(err)) => {
                    return diagnostics::OperationDiagnostic::default()
                        .report(err)
                        .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
                }
                Err(err) => return Err(err.into()),
            };
            (update.environment, None)
        };

        // At this point, we updated the existing environment, so we should remove any of its
        // existing executables.
        if let Some(existing_receipt) = existing_tool_receipt.as_ref() {
            remove_entrypoints(existing_receipt);
        }

        (environment, tool_lock)
    } else {
        let satisfied_tool_lock = match existing_tool_lock.take() {
            Some(lock) if lock.is_satisfied() => Some(lock.into_lock()),
            lock => {
                existing_tool_lock = lock;
                None
            }
        };
        let (resolution, interpreter, tool_lock) = if let Some(tool_lock) = satisfied_tool_lock {
            let resolution = tool_lock.to_resolution(
                Some(package_name),
                &interpreter,
                python_platform.as_ref(),
                &settings.resolver.build_options,
            )?;
            (resolution, interpreter, Some(tool_lock))
        } else {
            let spec = if tool_locks {
                tool_environment_spec(
                    spec,
                    existing_tool_lock
                        .as_ref()
                        .and_then(|lock| lock.preference()),
                    None,
                )
            } else {
                EnvironmentSpecification::from(spec)
            };

            // If we're creating a new environment, ensure that we can resolve the requirements prior
            // to removing any existing tools.
            let resolution = resolve_environment(
                spec.clone(),
                resolution_scope,
                &interpreter,
                python_platform.as_ref(),
                SourceTreeEditablePolicy::Tool,
                Constraints::from_requirements(receipt_build_constraints.iter().cloned()),
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
            .await;

            // If the resolution failed, retry with the inferred `requires-python` constraint.
            let (mut resolution, interpreter) = match resolution {
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
                            return diagnostics::OperationDiagnostic::default()
                                .report(err)
                                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
                        };

                        debug!(
                            "Re-resolving with Python {} (`{}`)",
                            interpreter.python_version(),
                            interpreter.sys_executable().display()
                        );

                        match resolve_environment(
                            spec.clone(),
                            resolution_scope,
                            &interpreter,
                            python_platform.as_ref(),
                            SourceTreeEditablePolicy::Tool,
                            Constraints::from_requirements(
                                receipt_build_constraints.iter().cloned(),
                            ),
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
                            Ok(resolution) => (resolution, interpreter),
                            Err(ProjectError::Operation(err)) => {
                                return diagnostics::OperationDiagnostic::default()
                                    .report(err)
                                    .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
                            }
                            Err(err) => return Err(err.into()),
                        }
                    }
                    err => return Err(err.into()),
                },
            };

            if tool_locks {
                resolution.canonicalize_proxy_artifact_urls_for_lock(
                    &settings.resolver.index_locations,
                    &[],
                )?;
                let tool_lock = ToolLock::from_resolution(
                    &tool_dir,
                    &resolution,
                    &lock_manifest,
                    &settings.resolver.index_locations,
                )?;
                let resolution = tool_lock.to_resolution(
                    Some(package_name),
                    &interpreter,
                    python_platform.as_ref(),
                    &settings.resolver.build_options,
                )?;
                (resolution, interpreter, Some(tool_lock))
            } else {
                (resolution.into(), interpreter, None)
            }
        };
        let hash_strategy = if tool_lock.is_some() {
            HashStrategy::from_resolution(&resolution, HashCheckingMode::Verify)?
        } else {
            HashStrategy::default()
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
            hash_strategy,
            Modifications::Exact,
            Constraints::from_requirements(receipt_build_constraints.iter().cloned()),
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
            Ok(environment) => (environment, tool_lock),
            Err(ProjectError::Operation(err)) => {
                return diagnostics::OperationDiagnostic::default()
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
        &options,
        force || invalid_tool_receipt,
        // Only persist the Python request if it was explicitly provided
        if explicit_python_request {
            python_request
        } else {
            None
        },
        requirements,
        receipt_constraints,
        receipt_overrides,
        receipt_excludes,
        receipt_build_constraints,
        tool_lock.as_ref(),
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

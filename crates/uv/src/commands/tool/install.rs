use std::fmt::Write;
use std::str::FromStr;

use anyhow::{Result, bail};
use owo_colors::OwoColorize;
use tokio::sync::Semaphore;
use tracing::{debug, trace};

use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_client::{BaseClientBuilder, RegistryClientBuilder};
use uv_configuration::{
    Concurrency, Constraints, DryRun, GitLfsSetting, Reinstall, TargetTriple, Upgrade,
};
use uv_distribution::LoweredExtraBuildDependencies;
use uv_distribution_types::{
    ExtraBuildRequires, IndexCapabilities, InstalledDistKind, NameRequirementSpecification,
    Requirement, RequirementSource, UnresolvedRequirementSpecification,
};
use uv_installer::{InstallationStrategy, SatisfiesResult, SitePackages};
use uv_normalize::PackageName;
use uv_pep440::{VersionSpecifier, VersionSpecifiers};
use uv_pep508::MarkerTree;
use uv_preview::Preview;
use uv_pypi_types::{DirectUrl, VcsKind};
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonInstallation, PythonPreference, PythonRequest,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_settings::{PythonInstallMirrors, ResolverInstallerOptions, ToolOptions};
use uv_tool::InstalledTools;
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
    finalize_tool_install, refine_interpreter, remove_entrypoints,
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
    cache: Cache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    if settings.resolver.torch_backend.is_some() {
        warn_user_once!(
            "The `--torch-backend` option is experimental and may change without warning."
        );
    }

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
        install_mirrors.python_downloads_json_url.as_deref(),
        preview,
    )
    .await?
    .into_interpreter();

    // Initialize any shared state.
    let state = PlatformState::default();
    let workspace_cache = WorkspaceCache::default();

    // Parse the input requirement.
    let request = ToolRequest::parse(&package, from.as_deref())?;

    // If the user passed, e.g., `ruff@latest`, refresh the cache.
    let cache = if request.is_latest() {
        cache.with_refresh(Refresh::All(Timestamp::now()))
    } else {
        cache
    };

    // Git fast path: For git dependencies, check if the tool is already installed
    // with the same commit as the remote. This avoids expensive package resolution.
    // For GitHub repos: use GitHub API (faster, no git credentials needed)
    // For other repos: use git ls-remote
    if !request.is_latest() && settings.reinstall.is_none() && settings.resolver.upgrade.is_none() {
        if let ToolRequest::Package {
            target: Target::Unspecified(from_str),
            ..
        } = &request
        {
            // Check if this is a git URL
            if from_str.starts_with("git+")
                || from_str.contains("github.com")
                || from_str.contains("gitlab")
            {
                // Try to parse the package name
                if let Ok(package_name) = PackageName::from_str(&package) {
                    // Check if the tool is already installed
                    if let Ok(installed_tools) = InstalledTools::from_settings() {
                        if let Ok(Some(environment)) =
                            installed_tools.get_environment(&package_name, &cache)
                        {
                            // Get site packages to find installed git commit
                            if let Ok(site_packages) =
                                SitePackages::from_environment(environment.environment())
                            {
                                let installed = site_packages.get_packages(&package_name);
                                if let Some(dist) = installed.first() {
                                    if let InstalledDistKind::Url(url_dist) = &dist.kind {
                                        if let DirectUrl::VcsUrl {
                                            vcs_info,
                                            url: installed_url,
                                            ..
                                        } = url_dist.direct_url.as_ref()
                                        {
                                            if vcs_info.vcs == VcsKind::Git {
                                                if let Some(installed_commit) = &vcs_info.commit_id
                                                {
                                                    // Parse the git URL from the --from argument
                                                    if let Ok(parsed_url) = url::Url::parse(
                                                        from_str.trim_start_matches("git+"),
                                                    ) {
                                                        let safe_url =
                                                            uv_redacted::DisplaySafeUrl::from(
                                                                parsed_url,
                                                            );
                                                        if let Ok(git_url) =
                                                            uv_git_types::GitUrl::try_from(safe_url)
                                                        {
                                                            // Check if repository URLs match
                                                            if uv_cache_key::RepositoryUrl::parse(
                                                                installed_url,
                                                            )
                                                            .is_ok_and(|installed_repo| {
                                                                installed_repo
                                                                    == uv_cache_key::RepositoryUrl::new(
                                                                        git_url.repository(),
                                                                    )
                                                            }) {
                                                                // Try to get the remote commit
                                                                let remote_oid =
                                                                    get_remote_git_commit(
                                                                        &git_url,
                                                                        &client_builder,
                                                                    )
                                                                    .await;

                                                                if let Some(remote_oid) = remote_oid
                                                                {
                                                                    if remote_oid.as_str()
                                                                        == installed_commit
                                                                    {
                                                                        debug!(
                                                                            "Git fast path: remote commit {} matches installed, skipping resolution",
                                                                            remote_oid
                                                                        );
                                                                        writeln!(
                                                                            printer.stderr(),
                                                                            "`{}` is already installed",
                                                                            package.cyan()
                                                                        )?;
                                                                        return Ok(
                                                                            ExitStatus::Success,
                                                                        );
                                                                    }
                                                                    debug!(
                                                                        "Git fast path: remote has new commit {} (installed: {})",
                                                                        remote_oid, installed_commit
                                                                    );
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Resolve the `--from` requirement.
    let requirement = match &request {
        // Ex) `ruff`
        ToolRequest::Package {
            executable,
            target: Target::Unspecified(from),
        } => {
            let source = if editable {
                RequirementsSource::from_editable(from)?
            } else {
                RequirementsSource::from_package(from)?
            };
            let requirement = RequirementsSpecification::from_source(&source, &client_builder)
                .await?
                .requirements;

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
                requirement,
                &interpreter,
                &settings,
                &client_builder,
                &state,
                concurrency,
                &cache,
                &workspace_cache,
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
        .build();

        // Initialize the capabilities.
        let capabilities = IndexCapabilities::default();
        let download_concurrency = Semaphore::new(concurrency.downloads);

        // Initialize the client to fetch the latest version.
        let latest_client = LatestClient {
            client: &client,
            capabilities: &capabilities,
            prerelease: settings.resolver.prerelease,
            exclude_newer: &settings.resolver.exclude_newer,
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

    // If the user passed `--force`, it implies `--reinstall-package <from>`
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
                concurrency,
                &cache,
                &workspace_cache,
                printer,
                preview,
                lfs,
            )
            .await?,
        );
        requirements
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
        concurrency,
        &cache,
        &workspace_cache,
        printer,
        preview,
        lfs,
    )
    .await?;

    // Resolve the build constraints.
    let build_constraints: Vec<Requirement> =
        operations::read_constraints(build_constraints, &client_builder)
            .await?
            .into_iter()
            .map(|constraint| constraint.requirement)
            .collect();

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

    let existing_environment =
        installed_tools
            .get_environment(package_name, &cache)?
            .filter(|environment| {
                if environment.environment().uses(&interpreter) {
                    trace!(
                        "Existing interpreter matches the requested interpreter for `{}`: {}",
                        package_name,
                        environment.environment().interpreter().sys_executable().display()
                    );
                    true
                } else {
                    let _ = writeln!(
                        printer.stderr(),
                        "Ignoring existing environment for `{}`: the requested Python interpreter does not match the environment interpreter",
                        package_name.cyan(),
                    );
                    false
                }
            });

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

                // Determine the markers and tags to use for the resolution.
                let markers = resolution_markers(None, python_platform.as_ref(), &interpreter);
                let tags = resolution_tags(None, python_platform.as_ref(), &interpreter)?;

                // Check if the installed packages meet the requirements.
                let site_packages = SitePackages::from_environment(environment.environment())?;
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
        constraints: constraints
            .iter()
            .cloned()
            .chain(latest.into_iter())
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
            environment.into_environment(),
            spec,
            Modifications::Exact,
            python_platform.as_ref(),
            Constraints::from_requirements(build_constraints.iter().cloned()),
            ExtraBuildRequires::default(),
            &settings,
            &client_builder,
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
                return diagnostics::OperationDiagnostic::native_tls(
                    client_builder.is_native_tls(),
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
        let resolution = resolve_environment(
            spec.clone(),
            &interpreter,
            python_platform.as_ref(),
            Constraints::from_requirements(build_constraints.iter().cloned()),
            &settings.resolver,
            &client_builder,
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
                        preview,
                    )
                    .await
                    .ok()
                    .flatten() else {
                        return diagnostics::OperationDiagnostic::native_tls(
                            client_builder.is_native_tls(),
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
                        Constraints::from_requirements(build_constraints.iter().cloned()),
                        &settings.resolver,
                        &client_builder,
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
                                client_builder.is_native_tls(),
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

        let environment = installed_tools.create_environment(package_name, interpreter, preview)?;

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
            Constraints::from_requirements(build_constraints.iter().cloned()),
            (&settings).into(),
            &client_builder,
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
            debug!("Failed to sync environment; removing `{}`", package_name);
            let _ = installed_tools.remove_environment(package_name);
        }) {
            Ok(environment) => environment,
            Err(ProjectError::Operation(err)) => {
                return diagnostics::OperationDiagnostic::native_tls(
                    client_builder.is_native_tls(),
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
        &options,
        force || invalid_tool_receipt,
        python_request,
        requirements,
        constraints,
        overrides,
        build_constraints,
        printer,
    )?;

    Ok(ExitStatus::Success)
}

/// Get the current remote commit for a Git URL.
///
/// Priority order:
/// 1. GitHub repositories: GitHub API (fastest, no credentials needed for public repos)
/// 2. GitLab repositories: GitLab API (faster than git, works with public repos)
/// 3. All repositories: git ls-remote (fallback, may require credentials)
async fn get_remote_git_commit(
    git_url: &uv_git_types::GitUrl,
    _client_builder: &BaseClientBuilder<'_>,
) -> Option<uv_git_types::GitOid> {
    let repo_url = git_url.repository();

    // Check if this is a GitHub repository
    if let Some(github_repo) = uv_git_types::GitHubRepository::parse(repo_url) {
        // Try GitHub API first (unless disabled)
        if std::env::var_os(uv_static::EnvVars::UV_NO_GITHUB_FAST_PATH).is_none() {
            if let Some(oid) = try_github_api(&github_repo, git_url.reference()).await {
                debug!(
                    "GitHub API fast path resolved {} to {}",
                    git_url.reference(),
                    oid
                );
                return Some(oid);
            }
        }
    }

    // Check if this is a GitLab repository (gitlab.com or self-hosted)
    if let Some(gitlab_repo) = uv_git_types::GitLabRepository::parse(repo_url) {
        // Try GitLab API (unless disabled via same env var as GitHub for simplicity)
        if std::env::var_os(uv_static::EnvVars::UV_NO_GITHUB_FAST_PATH).is_none() {
            if let Some(oid) = try_gitlab_api(&gitlab_repo, git_url.reference()).await {
                debug!(
                    "GitLab API fast path resolved {} to {}",
                    git_url.reference(),
                    oid
                );
                return Some(oid);
            }
        }
    }

    // Fall back to git ls-remote (unless disabled)
    if std::env::var_os(uv_static::EnvVars::UV_NO_GIT_LS_REMOTE_FAST_PATH).is_none() {
        match uv_git::git_ls_remote(repo_url, git_url.reference(), false) {
            Ok(Some(oid)) => {
                debug!("git ls-remote resolved {} to {}", git_url.reference(), oid);
                return Some(oid);
            }
            Ok(None) => {
                debug!(
                    "git ls-remote: reference type not supported for {}",
                    git_url.reference()
                );
            }
            Err(err) => {
                debug!("git ls-remote failed: {}", err);
            }
        }
    }

    None
}

/// Try to resolve a Git reference using the GitHub API.
async fn try_github_api(
    github_repo: &uv_git_types::GitHubRepository<'_>,
    reference: &uv_git_types::GitReference,
) -> Option<uv_git_types::GitOid> {
    use std::str::FromStr;
    use uv_git_types::GitOid;

    let rev = reference.as_rev();
    let github_api_base_url = std::env::var(uv_static::EnvVars::UV_GITHUB_FAST_PATH_URL)
        .unwrap_or_else(|_| "https://api.github.com/repos".to_owned());
    let github_api_url = format!(
        "{}/{}/{}/commits/{}",
        github_api_base_url, github_repo.owner, github_repo.repo, rev
    );

    debug!("Querying GitHub API for commit at: {}", github_api_url);

    // Use a simple reqwest client for the API call
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    else {
        return None;
    };

    let response = match client
        .get(&github_api_url)
        .header("Accept", "application/vnd.github.3.sha")
        .header(
            "User-Agent",
            format!(
                "uv/{} (+https://github.com/astral-sh/uv)",
                uv_version::version()
            ),
        )
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            debug!("GitHub API request failed: {}", err);
            return None;
        }
    };

    if !response.status().is_success() {
        debug!(
            "GitHub API request failed with status: {}",
            response.status()
        );
        return None;
    }

    let Ok(text) = response.text().await else {
        return None;
    };

    GitOid::from_str(text.trim()).ok()
}

/// Try to resolve a Git reference using the GitLab API.
///
/// Works with both gitlab.com and self-hosted GitLab instances.
/// GitLab API endpoint: GET /api/v4/projects/:id/repository/commits/:sha
///
/// Authentication is supported via environment variables:
/// - `GITLAB_TOKEN` or `GL_TOKEN`: Personal access token for gitlab.com
/// - `GITLAB_TOKEN_<HOST>`: Token for specific self-hosted instance
///   (e.g., `GITLAB_TOKEN_GITLAB_EXAMPLE_COM` for gitlab.example.com)
async fn try_gitlab_api(
    gitlab_repo: &uv_git_types::GitLabRepository<'_>,
    reference: &uv_git_types::GitReference,
) -> Option<uv_git_types::GitOid> {
    use std::str::FromStr;
    use uv_git_types::GitOid;

    let rev = reference.as_rev();
    let encoded_path = gitlab_repo.encoded_project_path();

    // Construct the GitLab API URL
    let gitlab_api_url = format!(
        "https://{}/api/v4/projects/{}/repository/commits/{}",
        gitlab_repo.host, encoded_path, rev
    );

    debug!("Querying GitLab API for commit at: {}", gitlab_api_url);

    // Try to get GitLab token from environment
    let token = get_gitlab_token(gitlab_repo.host);

    // Use a simple reqwest client for the API call
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    else {
        return None;
    };

    let mut request = client.get(&gitlab_api_url).header(
        "User-Agent",
        format!(
            "uv/{} (+https://github.com/astral-sh/uv)",
            uv_version::version()
        ),
    );

    // Add authentication if token is available
    if let Some(token) = &token {
        debug!("Using GitLab token for authentication");
        request = request.header("PRIVATE-TOKEN", token.as_str());
    }

    let response = match request.send().await {
        Ok(response) => response,
        Err(err) => {
            debug!("GitLab API request failed: {}", err);
            return None;
        }
    };

    if !response.status().is_success() {
        debug!(
            "GitLab API request failed with status: {}",
            response.status()
        );
        return None;
    }

    // GitLab returns JSON with commit info, we need to extract the "id" field
    let Ok(text) = response.text().await else {
        return None;
    };

    // Parse JSON to get the commit SHA
    // Response format: {"id": "abc123...", "short_id": "abc123", ...}
    let json: serde_json::Value = match serde_json::from_str(&text) {
        Ok(json) => json,
        Err(err) => {
            debug!("Failed to parse GitLab API response: {}", err);
            return None;
        }
    };

    let commit_id = json.get("id")?.as_str()?;
    GitOid::from_str(commit_id).ok()
}

/// Get GitLab API token from environment variables.
///
/// Checks in order:
/// 1. `GITLAB_TOKEN_<HOST>` - Host-specific token (e.g., `GITLAB_TOKEN_GITLAB_EXAMPLE_COM`)
/// 2. `GITLAB_TOKEN` - General GitLab token
/// 3. `GL_TOKEN` - Alternative GitLab token (used by glab CLI)
fn get_gitlab_token(host: &str) -> Option<String> {
    // Try host-specific token first
    // Convert host to env var name: gitlab.example.com -> GITLAB_EXAMPLE_COM
    let host_env_suffix = host.to_uppercase().replace(['.', '-'], "_");
    let host_specific_var = format!("GITLAB_TOKEN_{host_env_suffix}");

    if let Ok(token) = std::env::var(&host_specific_var) {
        if !token.is_empty() {
            debug!("Using GitLab token from {}", host_specific_var);
            return Some(token);
        }
    }

    // Try general GITLAB_TOKEN
    if let Ok(token) = std::env::var("GITLAB_TOKEN") {
        if !token.is_empty() {
            debug!("Using GitLab token from GITLAB_TOKEN");
            return Some(token);
        }
    }

    // Try GL_TOKEN (glab CLI convention)
    if let Ok(token) = std::env::var("GL_TOKEN") {
        if !token.is_empty() {
            debug!("Using GitLab token from GL_TOKEN");
            return Some(token);
        }
    }

    None
}

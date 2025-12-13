use std::collections::BTreeSet;

use anyhow::Context;
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::{Level, debug, enabled, warn};

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    BuildIsolation, BuildOptions, Concurrency, Constraints, DryRun, ExtrasSpecification,
    HashCheckingMode, IndexStrategy, Reinstall, SourceStrategy, Upgrade,
};
use uv_configuration::{KeyringProviderType, TargetTriple};
use uv_dispatch::{BuildDispatch, SharedState};
use uv_distribution::LoweredExtraBuildDependencies;
use uv_distribution_types::{
    ConfigSettings, DependencyMetadata, ExtraBuildVariables, Index, IndexLocations,
    NameRequirementSpecification, Origin, PackageConfigSettings, Requirement, Resolution,
    UnresolvedRequirementSpecification,
};
use uv_fs::Simplified;
use uv_install_wheel::LinkMode;
use uv_installer::{InstallationStrategy, SatisfiesResult, SitePackages};
use uv_normalize::{DefaultExtras, DefaultGroups, PackageName};
use uv_preview::{Preview, PreviewFeatures};
use uv_pypi_types::Conflicts;
use uv_python::{
    EnvironmentPreference, Prefix, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest, PythonVersion, Target,
};
use uv_requirements::{GroupsSpecification, RequirementsSource, RequirementsSpecification};
use uv_resolver::{
    DependencyMode, ExcludeNewer, FlatIndex, OptionsBuilder, PrereleaseMode, PylockToml,
    PythonRequirement, ResolutionMode, ResolverEnvironment,
};
use uv_settings::PythonInstallMirrors;
use uv_torch::{TorchMode, TorchSource, TorchStrategy};
use uv_types::HashStrategy;
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::WorkspaceCache;
use uv_workspace::pyproject::ExtraBuildDependencies;

use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger, InstallLogger};
use crate::commands::pip::operations::Modifications;
use crate::commands::pip::operations::{report_interpreter, report_target_environment};
use crate::commands::pip::{operations, resolution_markers, resolution_tags};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{ExitStatus, diagnostics};
use crate::printer::Printer;

/// Install packages into the current environment.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn pip_install(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    excludes: &[RequirementsSource],
    build_constraints: &[RequirementsSource],
    constraints_from_workspace: Vec<Requirement>,
    overrides_from_workspace: Vec<Requirement>,
    excludes_from_workspace: Vec<uv_normalize::PackageName>,
    build_constraints_from_workspace: Vec<Requirement>,
    extras: &ExtrasSpecification,
    groups: &GroupsSpecification,
    resolution_mode: ResolutionMode,
    prerelease_mode: PrereleaseMode,
    dependency_mode: DependencyMode,
    upgrade: Upgrade,
    index_locations: IndexLocations,
    index_strategy: IndexStrategy,
    torch_backend: Option<TorchMode>,
    dependency_metadata: DependencyMetadata,
    keyring_provider: KeyringProviderType,
    client_builder: &BaseClientBuilder<'_>,
    reinstall: Reinstall,
    link_mode: LinkMode,
    compile: bool,
    hash_checking: Option<HashCheckingMode>,
    installer_metadata: bool,
    config_settings: &ConfigSettings,
    config_settings_package: &PackageConfigSettings,
    build_isolation: BuildIsolation,
    extra_build_dependencies: &ExtraBuildDependencies,
    extra_build_variables: &ExtraBuildVariables,
    build_options: BuildOptions,
    modifications: Modifications,
    python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    python_downloads: PythonDownloads,
    install_mirrors: PythonInstallMirrors,
    strict: bool,
    exclude_newer: ExcludeNewer,
    sources: SourceStrategy,
    python: Option<String>,
    system: bool,
    break_system_packages: bool,
    target: Option<Target>,
    prefix: Option<Prefix>,
    python_preference: PythonPreference,
    concurrency: Concurrency,
    cache: Cache,
    dry_run: DryRun,
    printer: Printer,
    preview: Preview,
) -> anyhow::Result<ExitStatus> {
    let start = std::time::Instant::now();

    if !preview.is_enabled(PreviewFeatures::EXTRA_BUILD_DEPENDENCIES)
        && !extra_build_dependencies.is_empty()
    {
        warn_user_once!(
            "The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeatures::EXTRA_BUILD_DEPENDENCIES
        );
    }

    let client_builder = client_builder.clone().keyring(keyring_provider);

    // Read all requirements from the provided sources.
    let RequirementsSpecification {
        project,
        requirements,
        constraints,
        overrides,
        excludes,
        pylock,
        source_trees,
        groups,
        index_url,
        extra_index_urls,
        no_index,
        find_links,
        no_binary,
        no_build,
        extras: _,
    } = operations::read_requirements(
        requirements,
        constraints,
        overrides,
        excludes,
        extras,
        Some(groups),
        &client_builder,
    )
    .await?;

    if pylock.is_some() {
        if !preview.is_enabled(PreviewFeatures::PYLOCK) {
            warn_user!(
                "The `--pylock` option is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
                PreviewFeatures::PYLOCK
            );
        }
    }

    let constraints: Vec<NameRequirementSpecification> = constraints
        .iter()
        .cloned()
        .chain(
            constraints_from_workspace
                .into_iter()
                .map(NameRequirementSpecification::from),
        )
        .collect();

    let overrides: Vec<UnresolvedRequirementSpecification> = overrides
        .iter()
        .cloned()
        .chain(
            overrides_from_workspace
                .into_iter()
                .map(UnresolvedRequirementSpecification::from),
        )
        .collect();

    let excludes: Vec<PackageName> = excludes
        .into_iter()
        .chain(excludes_from_workspace)
        .collect();

    // Read build constraints.
    let build_constraints: Vec<NameRequirementSpecification> =
        operations::read_constraints(build_constraints, &client_builder)
            .await?
            .into_iter()
            .chain(
                build_constraints_from_workspace
                    .iter()
                    .cloned()
                    .map(NameRequirementSpecification::from),
            )
            .collect();

    // Detect the current Python interpreter.
    let environment = if target.is_some() || prefix.is_some() {
        let python_request = python.as_deref().map(PythonRequest::parse);
        let reporter = PythonDownloadReporter::single(printer);

        let installation = PythonInstallation::find_or_download(
            python_request.as_ref(),
            EnvironmentPreference::from_system_flag(system, false),
            python_preference.with_system_flag(system),
            python_downloads,
            &client_builder,
            &cache,
            Some(&reporter),
            install_mirrors.python_install_mirror.as_deref(),
            install_mirrors.pypy_install_mirror.as_deref(),
            install_mirrors.python_downloads_json_url.as_deref(),
            preview,
        )
        .await?;
        report_interpreter(&installation, true, printer)?;
        PythonEnvironment::from_installation(installation)
    } else {
        let environment = PythonEnvironment::find(
            &python
                .as_deref()
                .map(PythonRequest::parse)
                .unwrap_or_default(),
            EnvironmentPreference::from_system_flag(system, true),
            PythonPreference::default().with_system_flag(system),
            &cache,
            preview,
        )?;
        report_target_environment(&environment, &cache, printer)?;
        environment
    };

    // Lower the extra build dependencies, if any.
    let extra_build_requires =
        LoweredExtraBuildDependencies::from_non_lowered(extra_build_dependencies.clone())
            .into_inner();

    // Apply any `--target` or `--prefix` directories.
    let environment = if let Some(target) = target {
        debug!(
            "Using `--target` directory at {}",
            target.root().user_display()
        );
        environment.with_target(target)?
    } else if let Some(prefix) = prefix {
        debug!(
            "Using `--prefix` directory at {}",
            prefix.root().user_display()
        );
        environment.with_prefix(prefix)?
    } else {
        environment
    };

    // If the environment is externally managed, abort.
    if let Some(externally_managed) = environment.interpreter().is_externally_managed() {
        if break_system_packages {
            debug!("Ignoring externally managed environment due to `--break-system-packages`");
        } else {
            let managed_message = match externally_managed.into_error() {
                Some(error) => format!(
                    "The interpreter at {} is externally managed, and indicates the following:\n\n{}\n",
                    environment.root().user_display().cyan(),
                    textwrap::indent(&error, "  ").green(),
                ),
                None => format!(
                    "The interpreter at {} is externally managed and cannot be modified.",
                    environment.root().user_display().cyan()
                ),
            };

            let error_message = if system {
                // Add a hint about the `--system` flag
                format!(
                    "{}\n{}{} Virtual environments were not considered due to the `--system` flag",
                    managed_message,
                    "hint".bold().cyan(),
                    ":".bold()
                )
            } else {
                // Add a hint to create a virtual environment
                format!(
                    "{}\n{}{} Consider creating a virtual environment, e.g., with `uv venv`",
                    managed_message,
                    "hint".bold().cyan(),
                    ":".bold()
                )
            };

            return Err(anyhow::Error::msg(error_message));
        }
    }

    let _lock = environment
        .lock()
        .await
        .inspect_err(|err| {
            warn!("Failed to acquire environment lock: {err}");
        })
        .ok();

    // Determine the markers and tags to use for the resolution.
    let interpreter = environment.interpreter();
    let marker_env = resolution_markers(
        python_version.as_ref(),
        python_platform.as_ref(),
        interpreter,
    );
    let tags = resolution_tags(
        python_version.as_ref(),
        python_platform.as_ref(),
        interpreter,
    )?;

    // Determine the set of installed packages.
    let site_packages = SitePackages::from_environment(&environment)?;

    // Check if the current environment satisfies the requirements.
    // Ideally, the resolver would be fast enough to let us remove this check. But right now, for large environments,
    // it's an order of magnitude faster to validate the environment than to resolve the requirements.
    if reinstall.is_none()
        && upgrade.is_none()
        && source_trees.is_empty()
        && groups.is_empty()
        && pylock.is_none()
        && matches!(modifications, Modifications::Sufficient)
    {
        match site_packages.satisfies_spec(
            &requirements,
            &constraints,
            &overrides,
            InstallationStrategy::Permissive,
            &marker_env,
            &tags,
            config_settings,
            config_settings_package,
            &extra_build_requires,
            extra_build_variables,
        )? {
            // If the requirements are already satisfied, we're done.
            SatisfiesResult::Fresh {
                recursive_requirements,
            } => {
                if enabled!(Level::DEBUG) {
                    for requirement in recursive_requirements
                        .iter()
                        .map(ToString::to_string)
                        .sorted()
                    {
                        debug!("Requirement satisfied: {requirement}");
                    }
                }
                DefaultInstallLogger.on_audit(requirements.len(), start, printer, dry_run)?;

                return Ok(ExitStatus::Success);
            }
            SatisfiesResult::Unsatisfied(requirement) => {
                debug!("At least one requirement is not satisfied: {requirement}");
            }
        }
    }

    // Determine the Python requirement, if the user requested a specific version.
    let python_requirement = if let Some(python_version) = python_version.as_ref() {
        PythonRequirement::from_python_version(interpreter, python_version)
    } else {
        PythonRequirement::from_interpreter(interpreter)
    };

    // Collect the set of required hashes.
    let hasher = if let Some(hash_checking) = hash_checking {
        HashStrategy::from_requirements(
            requirements
                .iter()
                .chain(overrides.iter())
                .map(|entry| (&entry.requirement, entry.hashes.as_slice())),
            constraints
                .iter()
                .map(|entry| (&entry.requirement, entry.hashes.as_slice())),
            Some(&marker_env),
            hash_checking,
        )?
    } else {
        HashStrategy::None
    };

    // Incorporate any index locations from the provided sources.
    let index_locations = index_locations.combine(
        extra_index_urls
            .into_iter()
            .map(Index::from_extra_index_url)
            .chain(index_url.map(Index::from_index_url))
            .map(|index| index.with_origin(Origin::RequirementsTxt))
            .collect(),
        find_links
            .into_iter()
            .map(Index::from_find_links)
            .map(|index| index.with_origin(Origin::RequirementsTxt))
            .collect(),
        no_index,
    );

    // Determine the PyTorch backend.
    let torch_backend = torch_backend
        .map(|mode| {
            let source = if uv_auth::PyxTokenStore::from_settings()
                .is_ok_and(|store| store.has_credentials())
            {
                TorchSource::Pyx
            } else {
                TorchSource::default()
            };
            TorchStrategy::from_mode(
                mode,
                source,
                python_platform
                    .map(TargetTriple::platform)
                    .as_ref()
                    .unwrap_or(interpreter.platform())
                    .os(),
            )
        })
        .transpose()?;

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(client_builder.clone(), cache.clone())
        .index_locations(index_locations.clone())
        .index_strategy(index_strategy)
        .torch_backend(torch_backend.clone())
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build();

    // Combine the `--no-binary` and `--no-build` flags from the requirements files.
    let build_options = build_options.combine(no_binary, no_build);

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(client.cached_client(), client.connectivity(), &cache);
        let entries = client
            .fetch_all(index_locations.flat_indexes().map(Index::url))
            .await?;
        FlatIndex::from_entries(entries, Some(&tags), &hasher, &build_options)
    };

    // Determine whether to enable build isolation.
    let types_build_isolation = match build_isolation {
        BuildIsolation::Isolate => uv_types::BuildIsolation::Isolated,
        BuildIsolation::Shared => uv_types::BuildIsolation::Shared(&environment),
        BuildIsolation::SharedPackage(ref packages) => {
            uv_types::BuildIsolation::SharedPackage(&environment, packages)
        }
    };

    // Enforce (but never require) the build constraints, if `--require-hashes` or `--verify-hashes`
    // is provided. _Requiring_ hashes would be too strict, and would break with pip.
    let build_hasher = if hash_checking.is_some() {
        HashStrategy::from_requirements(
            std::iter::empty(),
            build_constraints
                .iter()
                .map(|entry| (&entry.requirement, entry.hashes.as_slice())),
            Some(&marker_env),
            HashCheckingMode::Verify,
        )?
    } else {
        HashStrategy::None
    };
    let build_constraints = Constraints::from_requirements(
        build_constraints
            .iter()
            .map(|constraint| constraint.requirement.clone()),
    );

    // Initialize any shared state.
    let state = SharedState::default();

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        &cache,
        &build_constraints,
        interpreter,
        &index_locations,
        &flat_index,
        &dependency_metadata,
        state.clone(),
        index_strategy,
        config_settings,
        config_settings_package,
        types_build_isolation,
        &extra_build_requires,
        extra_build_variables,
        link_mode,
        &build_options,
        &build_hasher,
        exclude_newer.clone(),
        sources,
        WorkspaceCache::default(),
        concurrency,
        preview,
    );

    let (resolution, hasher) = if let Some(pylock) = pylock {
        // Read the `pylock.toml` from disk or URL, and deserialize it from TOML.
        let (install_path, content) =
            if pylock.starts_with("http://") || pylock.starts_with("https://") {
                // Fetch the `pylock.toml` over HTTP(S).
                let url = uv_redacted::DisplaySafeUrl::parse(&pylock.to_string_lossy())?;
                let client = client_builder.build();
                let response = client
                    .for_host(&url)
                    .get(url::Url::from(url.clone()))
                    .send()
                    .await?;
                response.error_for_status_ref()?;
                let content = response.text().await?;
                // Use the current working directory as the install path for remote lock files.
                let install_path = std::env::current_dir()?;
                (install_path, content)
            } else {
                let install_path = std::path::absolute(&pylock)?;
                let install_path = install_path.parent().unwrap().to_path_buf();
                let content = fs_err::tokio::read_to_string(&pylock).await?;
                (install_path, content)
            };
        let lock = toml::from_str::<PylockToml>(&content).with_context(|| {
            format!("Not a valid `pylock.toml` file: {}", pylock.user_display())
        })?;

        // Verify that the Python version is compatible with the lock file.
        if let Some(requires_python) = lock.requires_python.as_ref() {
            if !requires_python.contains(interpreter.python_version()) {
                return Err(anyhow::anyhow!(
                    "The requested interpreter resolved to Python {}, which is incompatible with the `pylock.toml`'s Python requirement: `{}`",
                    interpreter.python_version(),
                    requires_python,
                ));
            }
        }

        // Convert the extras and groups specifications into a concrete form.
        let extras = extras.with_defaults(DefaultExtras::default());
        let extras = extras
            .extra_names(lock.extras.iter())
            .cloned()
            .collect::<Vec<_>>();

        let groups = groups
            .get(&pylock)
            .cloned()
            .unwrap_or_default()
            .with_defaults(DefaultGroups::List(lock.default_groups.clone()));
        let groups = groups
            .group_names(lock.dependency_groups.iter())
            .cloned()
            .collect::<Vec<_>>();

        let resolution = lock.to_resolution(
            &install_path,
            marker_env.markers(),
            &extras,
            &groups,
            &tags,
            &build_options,
        )?;
        let hasher = HashStrategy::from_resolution(&resolution, HashCheckingMode::Verify)?;

        (resolution, hasher)
    } else {
        // When resolving, don't take any external preferences into account.
        let preferences = Vec::default();

        let options = OptionsBuilder::new()
            .resolution_mode(resolution_mode)
            .prerelease_mode(prerelease_mode)
            .dependency_mode(dependency_mode)
            .exclude_newer(exclude_newer.clone())
            .index_strategy(index_strategy)
            .torch_backend(torch_backend)
            .build_options(build_options.clone())
            .build();

        // Resolve the requirements.
        let resolution = match operations::resolve(
            requirements,
            constraints,
            overrides,
            excludes,
            source_trees,
            project,
            BTreeSet::default(),
            extras,
            &groups,
            preferences,
            site_packages.clone(),
            &hasher,
            &reinstall,
            &upgrade,
            Some(&tags),
            ResolverEnvironment::specific(marker_env.clone()),
            python_requirement,
            interpreter.markers(),
            Conflicts::empty(),
            &client,
            &flat_index,
            state.index(),
            &build_dispatch,
            concurrency,
            options,
            Box::new(DefaultResolveLogger),
            printer,
        )
        .await
        {
            Ok(graph) => Resolution::from(graph),
            Err(err) => {
                return diagnostics::OperationDiagnostic::native_tls(
                    client_builder.is_native_tls(),
                )
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
            }
        };

        (resolution, hasher)
    };

    // Constrain any build requirements marked as `match-runtime = true`.
    let extra_build_requires = extra_build_requires.match_runtime(&resolution)?;

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        &cache,
        &build_constraints,
        interpreter,
        &index_locations,
        &flat_index,
        &dependency_metadata,
        state.clone(),
        index_strategy,
        config_settings,
        config_settings_package,
        types_build_isolation,
        &extra_build_requires,
        extra_build_variables,
        link_mode,
        &build_options,
        &build_hasher,
        exclude_newer.clone(),
        sources,
        WorkspaceCache::default(),
        concurrency,
        preview,
    );

    // Sync the environment.
    match operations::install(
        &resolution,
        site_packages,
        InstallationStrategy::Permissive,
        modifications,
        &reinstall,
        &build_options,
        link_mode,
        compile,
        &hasher,
        &tags,
        &client,
        state.in_flight(),
        concurrency,
        &build_dispatch,
        &cache,
        &environment,
        Box::new(DefaultInstallLogger),
        installer_metadata,
        dry_run,
        printer,
        preview,
    )
    .await
    {
        Ok(..) => {}
        Err(err) => {
            return diagnostics::OperationDiagnostic::native_tls(client_builder.is_native_tls())
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
        }
    }

    // Notify the user of any resolution diagnostics.
    operations::diagnose_resolution(resolution.diagnostics(), printer)?;

    // Notify the user of any environment diagnostics.
    if strict && !dry_run.enabled() {
        operations::diagnose_environment(&resolution, &environment, &marker_env, &tags, printer)?;
    }

    Ok(ExitStatus::Success)
}

use std::collections::BTreeSet;
use std::fmt::Write;

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use tracing::{debug, warn};

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
    ConfigSettings, DependencyMetadata, ExtraBuildVariables, Index, IndexLocations, Origin,
    PackageConfigSettings, Resolution,
};
use uv_fs::Simplified;
use uv_install_wheel::LinkMode;
use uv_installer::{InstallationStrategy, SitePackages};
use uv_normalize::{DefaultExtras, DefaultGroups};
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

use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger};
use crate::commands::pip::operations::Modifications;
use crate::commands::pip::operations::{report_interpreter, report_target_environment};
use crate::commands::pip::{operations, resolution_markers, resolution_tags};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{ExitStatus, diagnostics};
use crate::printer::Printer;

/// Install a set of locked requirements into the current Python environment.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn pip_sync(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    build_constraints: &[RequirementsSource],
    extras: &ExtrasSpecification,
    groups: &GroupsSpecification,
    reinstall: Reinstall,
    link_mode: LinkMode,
    compile: bool,
    hash_checking: Option<HashCheckingMode>,
    index_locations: IndexLocations,
    index_strategy: IndexStrategy,
    torch_backend: Option<TorchMode>,
    dependency_metadata: DependencyMetadata,
    keyring_provider: KeyringProviderType,
    client_builder: &BaseClientBuilder<'_>,
    allow_empty_requirements: bool,
    installer_metadata: bool,
    config_settings: &ConfigSettings,
    config_settings_package: &PackageConfigSettings,
    build_isolation: BuildIsolation,
    extra_build_dependencies: &ExtraBuildDependencies,
    extra_build_variables: &ExtraBuildVariables,
    build_options: BuildOptions,
    python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    python_downloads: PythonDownloads,
    install_mirrors: PythonInstallMirrors,
    strict: bool,
    exclude_newer: ExcludeNewer,
    python: Option<String>,
    system: bool,
    break_system_packages: bool,
    target: Option<Target>,
    prefix: Option<Prefix>,
    sources: SourceStrategy,
    python_preference: PythonPreference,
    concurrency: Concurrency,
    cache: Cache,
    dry_run: DryRun,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    if !preview.is_enabled(PreviewFeatures::EXTRA_BUILD_DEPENDENCIES)
        && !extra_build_dependencies.is_empty()
    {
        warn_user_once!(
            "The `extra-build-dependencies` option is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeatures::EXTRA_BUILD_DEPENDENCIES
        );
    }

    let client_builder = client_builder.clone().keyring(keyring_provider);

    // Initialize a few defaults.
    let overrides = &[];
    let excludes = &[];
    let upgrade = Upgrade::default();
    let resolution_mode = ResolutionMode::default();
    let prerelease_mode = PrereleaseMode::default();
    let dependency_mode = DependencyMode::Direct;

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

    // Read build constraints.
    let build_constraints =
        operations::read_constraints(build_constraints, &client_builder).await?;

    // Validate that the requirements are non-empty.
    if !allow_empty_requirements {
        let num_requirements =
            requirements.len() + source_trees.len() + usize::from(pylock.is_some());
        if num_requirements == 0 {
            writeln!(
                printer.stderr(),
                "No requirements found (hint: use `--allow-empty-requirements` to clear the environment)"
            )?;
            return Ok(ExitStatus::Success);
        }
    }

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
            return if let Some(error) = externally_managed.into_error() {
                Err(anyhow::anyhow!(
                    "The interpreter at {} is externally managed, and indicates the following:\n\n{}\n\nConsider creating a virtual environment with `uv venv`.",
                    environment.root().user_display().cyan(),
                    textwrap::indent(&error, "  ").green(),
                ))
            } else {
                Err(anyhow::anyhow!(
                    "The interpreter at {} is externally managed. Instead, create a virtual environment with `uv venv`.",
                    environment.root().user_display().cyan()
                ))
            };
        }
    }

    let _lock = environment
        .lock()
        .await
        .inspect_err(|err| {
            warn!("Failed to acquire environment lock: {err}");
        })
        .ok();

    let interpreter = environment.interpreter();

    // Determine the Python requirement, if the user requested a specific version.
    let python_requirement = if let Some(python_version) = python_version.as_ref() {
        PythonRequirement::from_python_version(interpreter, python_version)
    } else {
        PythonRequirement::from_interpreter(interpreter)
    };

    // Determine the markers and tags to use for resolution.
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

    // Collect the set of required hashes.
    let hasher = if let Some(hash_checking) = hash_checking {
        HashStrategy::from_requirements(
            requirements
                .iter()
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

    // Lower the extra build dependencies, if any.
    let extra_build_requires =
        LoweredExtraBuildDependencies::from_non_lowered(extra_build_dependencies.clone())
            .into_inner();

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

    // Determine the set of installed packages.
    let site_packages = SitePackages::from_environment(&environment)?;

    let (resolution, hasher) = if let Some(pylock) = pylock {
        // Read the `pylock.toml` from disk, and deserialize it from TOML.
        let install_path = std::path::absolute(&pylock)?;
        let install_path = install_path.parent().unwrap();
        let content = fs_err::tokio::read_to_string(&pylock).await?;
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
            install_path,
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
            Ok(resolution) => Resolution::from(resolution),
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
        Modifications::Exact,
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
        Ok(_) => {}
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

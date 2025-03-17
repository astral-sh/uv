use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::sync::Arc;

use anyhow::Result;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    BuildOptions, Concurrency, ConfigSettings, Constraints, DryRun, ExtrasSpecification,
    HashCheckingMode, IndexStrategy, PreviewMode, Reinstall, SourceStrategy, Upgrade,
};
use uv_configuration::{KeyringProviderType, TargetTriple};
use uv_dispatch::{BuildDispatch, SharedState};
use uv_distribution_types::{DependencyMetadata, Index, IndexLocations, Origin, Resolution};
use uv_fs::Simplified;
use uv_install_wheel::LinkMode;
use uv_installer::SitePackages;
use uv_pep508::PackageName;
use uv_pypi_types::Conflicts;
use uv_python::{
    EnvironmentPreference, Prefix, PythonEnvironment, PythonInstallation, PythonPreference,
    PythonRequest, PythonVersion, Target,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_resolver::{
    DependencyMode, ExcludeNewer, FlatIndex, OptionsBuilder, PrereleaseMode, PythonRequirement,
    ResolutionMode, ResolverEnvironment,
};
use uv_types::{BuildIsolation, HashStrategy};
use uv_workspace::WorkspaceCache;

use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger};
use crate::commands::pip::operations::Modifications;
use crate::commands::pip::operations::{report_interpreter, report_target_environment};
use crate::commands::pip::{operations, resolution_markers, resolution_tags};
use crate::commands::{diagnostics, ExitStatus};
use crate::printer::Printer;
use crate::settings::NetworkSettings;

/// Install a set of locked requirements into the current Python environment.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn pip_sync(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    build_constraints: &[RequirementsSource],
    reinstall: Reinstall,
    link_mode: LinkMode,
    compile: bool,
    hash_checking: Option<HashCheckingMode>,
    index_locations: IndexLocations,
    index_strategy: IndexStrategy,
    dependency_metadata: DependencyMetadata,
    keyring_provider: KeyringProviderType,
    network_settings: &NetworkSettings,
    allow_empty_requirements: bool,
    installer_metadata: bool,
    config_settings: &ConfigSettings,
    no_build_isolation: bool,
    no_build_isolation_package: Vec<PackageName>,
    build_options: BuildOptions,
    python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    strict: bool,
    exclude_newer: Option<ExcludeNewer>,
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
    preview: PreviewMode,
) -> Result<ExitStatus> {
    let client_builder = BaseClientBuilder::new()
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .keyring(keyring_provider)
        .allow_insecure_host(network_settings.allow_insecure_host.clone());

    // Initialize a few defaults.
    let overrides = &[];
    let extras = ExtrasSpecification::default();
    let groups = BTreeMap::default();
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
        &extras,
        groups,
        &client_builder,
    )
    .await?;

    // Read build constraints.
    let build_constraints =
        operations::read_constraints(build_constraints, &client_builder).await?;

    // Validate that the requirements are non-empty.
    if !allow_empty_requirements {
        let num_requirements = requirements.len() + source_trees.len();
        if num_requirements == 0 {
            writeln!(printer.stderr(), "No requirements found (hint: use `--allow-empty-requirements` to clear the environment)")?;
            return Ok(ExitStatus::Success);
        }
    }

    // Detect the current Python interpreter.
    let environment = if target.is_some() || prefix.is_some() {
        let installation = PythonInstallation::find(
            &python
                .as_deref()
                .map(PythonRequest::parse)
                .unwrap_or_default(),
            EnvironmentPreference::from_system_flag(system, false),
            python_preference,
            &cache,
        )?;
        report_interpreter(&installation, true, printer)?;
        PythonEnvironment::from_installation(installation)
    } else {
        let environment = PythonEnvironment::find(
            &python
                .as_deref()
                .map(PythonRequest::parse)
                .unwrap_or_default(),
            EnvironmentPreference::from_system_flag(system, true),
            &cache,
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

    let _lock = environment.lock().await?;

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

    // Add all authenticated sources to the cache.
    for index in index_locations.allowed_indexes() {
        if let Some(credentials) = index.credentials() {
            let credentials = Arc::new(credentials);
            uv_auth::store_credentials(index.raw_url(), credentials.clone());
            if let Some(root_url) = index.root_url() {
                uv_auth::store_credentials(&root_url, credentials.clone());
            }
        }
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::try_from(client_builder)?
        .cache(cache.clone())
        .index_urls(index_locations.index_urls())
        .index_strategy(index_strategy)
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build();

    // Combine the `--no-binary` and `--no-build` flags from the requirements files.
    let build_options = build_options.combine(no_binary, no_build);

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, &cache);
        let entries = client
            .fetch(index_locations.flat_indexes().map(Index::url))
            .await?;
        FlatIndex::from_entries(entries, Some(&tags), &hasher, &build_options)
    };

    // Determine whether to enable build isolation.
    let build_isolation = if no_build_isolation {
        BuildIsolation::Shared(&environment)
    } else if no_build_isolation_package.is_empty() {
        BuildIsolation::Isolated
    } else {
        BuildIsolation::SharedPackage(&environment, &no_build_isolation_package)
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

    // When resolving, don't take any external preferences into account.
    let preferences = Vec::default();

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        &cache,
        build_constraints,
        interpreter,
        &index_locations,
        &flat_index,
        &dependency_metadata,
        state.clone(),
        index_strategy,
        config_settings,
        build_isolation,
        link_mode,
        &build_options,
        &build_hasher,
        exclude_newer,
        sources,
        WorkspaceCache::default(),
        concurrency,
        preview,
    );

    // Determine the set of installed packages.
    let site_packages = SitePackages::from_environment(&environment)?;

    let options = OptionsBuilder::new()
        .resolution_mode(resolution_mode)
        .prerelease_mode(prerelease_mode)
        .dependency_mode(dependency_mode)
        .exclude_newer(exclude_newer)
        .index_strategy(index_strategy)
        .build_options(build_options.clone())
        .build();

    let resolution = match operations::resolve(
        requirements,
        constraints,
        overrides,
        source_trees,
        project,
        BTreeSet::default(),
        &extras,
        &groups,
        preferences,
        site_packages.clone(),
        &hasher,
        &reinstall,
        &upgrade,
        Some(&tags),
        ResolverEnvironment::specific(marker_env.clone()),
        python_requirement,
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
            return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
        }
    };

    // Sync the environment.
    match operations::install(
        &resolution,
        site_packages,
        Modifications::Exact,
        &reinstall,
        &build_options,
        link_mode,
        compile,
        &index_locations,
        config_settings,
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
    )
    .await
    {
        Ok(_) => {}
        Err(err) => {
            return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
        }
    }

    // Notify the user of any resolution diagnostics.
    operations::diagnose_resolution(resolution.diagnostics(), printer)?;

    // Notify the user of any environment diagnostics.
    if strict && !dry_run.enabled() {
        operations::diagnose_environment(&resolution, &environment, &marker_env, printer)?;
    }

    Ok(ExitStatus::Success)
}

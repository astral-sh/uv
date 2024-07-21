use std::fmt::Write;

use anstream::eprint;
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::{debug, enabled, Level};

use distribution_types::{IndexLocations, Resolution, UnresolvedRequirementSpecification};
use install_wheel_rs::linker::LinkMode;
use pypi_types::Requirement;
use uv_auth::store_credentials_from_url;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    BuildOptions, Concurrency, ConfigSettings, ExtrasSpecification, HashCheckingMode,
    IndexStrategy, PreviewMode, Reinstall, SetupPyStrategy, Upgrade,
};
use uv_configuration::{KeyringProviderType, TargetTriple};
use uv_dispatch::BuildDispatch;
use uv_fs::Simplified;
use uv_installer::{SatisfiesResult, SitePackages};
use uv_python::{
    EnvironmentPreference, Prefix, PythonEnvironment, PythonRequest, PythonVersion, Target,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_resolver::{
    DependencyMode, ExcludeNewer, FlatIndex, OptionsBuilder, PreReleaseMode, PythonRequirement,
    ResolutionMode, ResolverMarkers,
};
use uv_types::{BuildIsolation, HashStrategy};

use crate::commands::pip::operations::Modifications;
use crate::commands::pip::{operations, resolution_environment};
use crate::commands::{elapsed, ExitStatus, SharedState};
use crate::printer::Printer;

/// Install packages into the current environment.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn pip_install(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    constraints_from_workspace: Vec<Requirement>,
    overrides_from_workspace: Vec<Requirement>,
    extras: &ExtrasSpecification,
    resolution_mode: ResolutionMode,
    prerelease_mode: PreReleaseMode,
    dependency_mode: DependencyMode,
    upgrade: Upgrade,
    index_locations: IndexLocations,
    index_strategy: IndexStrategy,
    keyring_provider: KeyringProviderType,
    reinstall: Reinstall,
    link_mode: LinkMode,
    compile: bool,
    hash_checking: Option<HashCheckingMode>,
    setup_py: SetupPyStrategy,
    connectivity: Connectivity,
    config_settings: &ConfigSettings,
    no_build_isolation: bool,
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
    concurrency: Concurrency,
    native_tls: bool,
    preview: PreviewMode,
    cache: Cache,
    dry_run: bool,
    printer: Printer,
) -> anyhow::Result<ExitStatus> {
    let start = std::time::Instant::now();

    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls)
        .keyring(keyring_provider);

    // Read all requirements from the provided sources.
    let RequirementsSpecification {
        project,
        requirements,
        constraints,
        overrides,
        source_trees,
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
        extras,
        &client_builder,
    )
    .await?;

    let constraints: Vec<Requirement> = constraints
        .iter()
        .cloned()
        .chain(constraints_from_workspace.into_iter())
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

    // Detect the current Python interpreter.
    let environment = PythonEnvironment::find(
        &python
            .as_deref()
            .map(PythonRequest::parse)
            .unwrap_or_default(),
        EnvironmentPreference::from_system_flag(system, true),
        &cache,
    )?;

    debug!(
        "Using Python {} environment at {}",
        environment.interpreter().python_version(),
        environment.python_executable().user_display().cyan()
    );

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

    let _lock = environment.lock()?;

    // Determine the set of installed packages.
    let site_packages = SitePackages::from_environment(&environment)?;

    // Check if the current environment satisfies the requirements.
    // Ideally, the resolver would be fast enough to let us remove this check. But right now, for large environments,
    // it's an order of magnitude faster to validate the environment than to resolve the requirements.
    if reinstall.is_none() && upgrade.is_none() && source_trees.is_empty() && overrides.is_empty() {
        match site_packages.satisfies(&requirements, &constraints)? {
            // If the requirements are already satisfied, we're done.
            SatisfiesResult::Fresh {
                recursive_requirements,
            } => {
                if enabled!(Level::DEBUG) {
                    for requirement in recursive_requirements
                        .iter()
                        .map(|entry| entry.requirement.to_string())
                        .sorted()
                    {
                        debug!("Requirement satisfied: {requirement}");
                    }
                }
                let num_requirements = requirements.len();
                let s = if num_requirements == 1 { "" } else { "s" };
                writeln!(
                    printer.stderr(),
                    "{}",
                    format!(
                        "Audited {} {}",
                        format!("{num_requirements} package{s}").bold(),
                        format!("in {}", elapsed(start.elapsed())).dimmed()
                    )
                    .dimmed()
                )?;
                if dry_run {
                    writeln!(printer.stderr(), "Would make no changes")?;
                }
                return Ok(ExitStatus::Success);
            }
            SatisfiesResult::Unsatisfied(requirement) => {
                debug!("At least one requirement is not satisfied: {requirement}");
            }
        }
    }

    let interpreter = environment.interpreter();

    // Determine the Python requirement, if the user requested a specific version.
    let python_requirement = if let Some(python_version) = python_version.as_ref() {
        PythonRequirement::from_python_version(interpreter, python_version)
    } else {
        PythonRequirement::from_interpreter(interpreter)
    };

    // Determine the environment for the resolution.
    let (tags, markers) = resolution_environment(python_version, python_platform, interpreter)?;

    // Collect the set of required hashes.
    let hasher = if let Some(hash_checking) = hash_checking {
        HashStrategy::from_requirements(
            requirements
                .iter()
                .chain(overrides.iter())
                .map(|entry| (&entry.requirement, entry.hashes.as_slice())),
            Some(&markers),
            hash_checking,
        )?
    } else {
        HashStrategy::None
    };

    // When resolving, don't take any external preferences into account.
    let preferences = Vec::default();

    // Ignore development dependencies.
    let dev = Vec::default();

    // Incorporate any index locations from the provided sources.
    let index_locations =
        index_locations.combine(index_url, extra_index_urls, find_links, no_index);

    // Add all authenticated sources to the cache.
    for url in index_locations.urls() {
        store_credentials_from_url(url);
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::from(client_builder)
        .cache(cache.clone())
        .index_urls(index_locations.index_urls())
        .index_strategy(index_strategy)
        .markers(&markers)
        .platform(interpreter.platform())
        .build();

    // Combine the `--no-binary` and `--no-build` flags from the requirements files.
    let build_options = build_options.combine(no_binary, no_build);

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, &cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, Some(&tags), &hasher, &build_options)
    };

    // Determine whether to enable build isolation.
    let build_isolation = if no_build_isolation {
        BuildIsolation::Shared(&environment)
    } else {
        BuildIsolation::Isolated
    };

    // Initialize any shared state.
    let state = SharedState::default();

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        &cache,
        interpreter,
        &index_locations,
        &flat_index,
        &state.index,
        &state.git,
        &state.in_flight,
        index_strategy,
        setup_py,
        config_settings,
        build_isolation,
        link_mode,
        &build_options,
        exclude_newer,
        concurrency,
        preview,
    );

    let options = OptionsBuilder::new()
        .resolution_mode(resolution_mode)
        .prerelease_mode(prerelease_mode)
        .dependency_mode(dependency_mode)
        .exclude_newer(exclude_newer)
        .index_strategy(index_strategy)
        .build();

    // Resolve the requirements.
    let resolution = match operations::resolve(
        requirements,
        constraints,
        overrides,
        dev,
        source_trees,
        project,
        extras,
        preferences,
        site_packages.clone(),
        &hasher,
        &reinstall,
        &upgrade,
        Some(&tags),
        ResolverMarkers::SpecificEnvironment((*markers).clone()),
        python_requirement,
        &client,
        &flat_index,
        &state.index,
        &build_dispatch,
        concurrency,
        options,
        printer,
        preview,
        false,
    )
    .await
    {
        Ok(resolution) => Resolution::from(resolution),
        Err(operations::Error::Resolve(uv_resolver::ResolveError::NoSolution(err))) => {
            let report = miette::Report::msg(format!("{err}")).context(err.header());
            eprint!("{report:?}");
            return Ok(ExitStatus::Failure);
        }
        Err(err) => return Err(err.into()),
    };

    // Sync the environment.
    operations::install(
        &resolution,
        site_packages,
        Modifications::Sufficient,
        &reinstall,
        &build_options,
        link_mode,
        compile,
        &index_locations,
        &hasher,
        &tags,
        &client,
        &state.in_flight,
        concurrency,
        &build_dispatch,
        &cache,
        &environment,
        dry_run,
        printer,
        preview,
    )
    .await?;

    // Notify the user of any resolution diagnostics.
    operations::diagnose_resolution(resolution.diagnostics(), printer)?;

    // Notify the user of any environment diagnostics.
    if strict && !dry_run {
        operations::diagnose_environment(&resolution, &environment, printer)?;
    }

    Ok(ExitStatus::Success)
}

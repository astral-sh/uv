use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;

use uv_auth::UrlAuthPolicies;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    BuildOptions, Concurrency, ConfigSettings, Constraints, DryRun, ExtrasSpecification,
    HashCheckingMode, IndexStrategy, PreviewMode, Reinstall, SourceStrategy, Upgrade,
};
use uv_configuration::{KeyringProviderType, TargetTriple};
use uv_dispatch::{BuildDispatch, SharedState};
use uv_distribution::DistributionDatabase;
use uv_distribution_types::{
    DependencyMetadata, Index, IndexLocations, NameRequirementSpecification, Origin, Requirement,
    Resolution, UnresolvedRequirementSpecification,
};
use uv_install_wheel::LinkMode;
use uv_installer::{Plan, Planner, Preparer, SitePackages};
use uv_normalize::GroupName;
use uv_pep508::PackageName;
use uv_pypi_types::Conflicts;
use uv_python::{
    EnvironmentPreference, PythonEnvironment, PythonInstallation, PythonPreference,
    PythonRequest, PythonVersion, Target,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_resolver::{
    DependencyMode, ExcludeNewer, FlatIndex, OptionsBuilder, PrereleaseMode, PythonRequirement,
    ResolutionMode, ResolverEnvironment,
};
use uv_torch::{TorchMode, TorchStrategy};
use uv_types::{BuildIsolation, HashStrategy};
use uv_warnings::warn_user;
use uv_workspace::WorkspaceCache;

use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::pip::operations::report_interpreter;
use crate::commands::pip::{operations, resolution_markers, resolution_tags};
use crate::commands::reporters::PrepareReporter;
use crate::commands::{diagnostics, ExitStatus};
use crate::printer::Printer;
use crate::settings::NetworkSettings;
use walkdir::WalkDir;
use zip::{CompressionMethod, ZipWriter};

/// Puts wheels into the current environment.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn pip_wheel(
    target: Target,
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    build_constraints: &[RequirementsSource],
    constraints_from_workspace: Vec<Requirement>,
    overrides_from_workspace: Vec<Requirement>,
    build_constraints_from_workspace: Vec<Requirement>,
    extras: &ExtrasSpecification,
    groups: BTreeMap<PathBuf, Vec<GroupName>>,
    resolution_mode: ResolutionMode,
    prerelease_mode: PrereleaseMode,
    dependency_mode: DependencyMode,
    upgrade: Upgrade,
    index_locations: IndexLocations,
    index_strategy: IndexStrategy,
    torch_backend: Option<TorchMode>,
    dependency_metadata: DependencyMetadata,
    keyring_provider: KeyringProviderType,
    network_settings: &NetworkSettings,
    reinstall: Reinstall,
    link_mode: LinkMode,
    hash_checking: Option<HashCheckingMode>,
    installer_metadata: bool,
    config_settings: &ConfigSettings,
    no_build_isolation: bool,
    no_build_isolation_package: Vec<PackageName>,
    build_options: BuildOptions,
    python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    exclude_newer: Option<ExcludeNewer>,
    sources: SourceStrategy,
    python: Option<String>,
    system: bool,
    python_preference: PythonPreference,
    concurrency: Concurrency,
    cache: Cache,
    dry_run: DryRun,
    printer: Printer,
    preview: PreviewMode,
) -> anyhow::Result<ExitStatus> {
    let start = std::time::Instant::now();

    let client_builder = BaseClientBuilder::new()
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .keyring(keyring_provider)
        .allow_insecure_host(network_settings.allow_insecure_host.clone());

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
        extras,
        groups,
        &client_builder,
    )
    .await?;

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

    // Read build constraints.
    let build_constraints: Vec<NameRequirementSpecification> =
        operations::read_constraints(build_constraints, &client_builder)
            .await?
            .iter()
            .cloned()
            .chain(
                build_constraints_from_workspace
                    .iter()
                    .cloned()
                    .map(NameRequirementSpecification::from),
            )
            .collect();

    let environment = {
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
    };

    let _lock = environment.lock().await?;

    // Determine the markers to use for the resolution.
    let interpreter = environment.interpreter();
    let marker_env = resolution_markers(
        python_version.as_ref(),
        python_platform.as_ref(),
        interpreter,
    );

    // Determine the set of installed packages.
    let site_packages = SitePackages::from_environment(&environment)?;

    // Determine the Python requirement, if the user requested a specific version.
    let python_requirement = if let Some(python_version) = python_version.as_ref() {
        PythonRequirement::from_python_version(interpreter, python_version)
    } else {
        PythonRequirement::from_interpreter(interpreter)
    };

    // Determine the tags to use for the resolution.
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

    // When resolving, don't take any external preferences into account.
    let preferences = Vec::default();

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

    // Determine the PyTorch backend.
    let torch_backend = torch_backend.map(|mode| {
        if preview.is_disabled() {
            warn_user!("The `--torch-backend` setting is experimental and may change without warning. Pass `--preview` to disable this warning.");
        }

        TorchStrategy::from_mode(
            mode,
            python_platform
                .map(TargetTriple::platform)
                .as_ref()
                .unwrap_or(interpreter.platform())
                .os(),
        )
    }).transpose()?;

    // Initialize the registry client.
    let client = RegistryClientBuilder::try_from(client_builder)?
        .cache(cache.clone())
        .index_urls(index_locations.index_urls())
        .index_strategy(index_strategy)
        .url_auth_policies(UrlAuthPolicies::from(&index_locations))
        .torch_backend(torch_backend)
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

    let options = OptionsBuilder::new()
        .resolution_mode(resolution_mode)
        .prerelease_mode(prerelease_mode)
        .dependency_mode(dependency_mode)
        .exclude_newer(exclude_newer)
        .index_strategy(index_strategy)
        .build_options(build_options.clone())
        .build();

    // Resolve the requirements.
    let resolution = match operations::resolve(
        requirements,
        constraints,
        overrides,
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
            return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
        }
    };

    let plan = Planner::new(&resolution)
        .build(
            site_packages,
            &reinstall,
            &build_options,
            &hasher,
            &index_locations,
            config_settings,
            &cache,
            &environment,
            &tags,
        )
        .context("Failed to determine installation plan")?;

    let Plan {
        cached,
        remote,
        reinstalls,
        extraneous,
    } = plan;

    let wheels = if remote.is_empty() {
        vec![]
    } else {
        let start = std::time::Instant::now();

        let preparer = Preparer::new(
            &cache,
            &tags,
            &hasher,
            &build_options,
            DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads),
        )
        .with_reporter(Arc::new(
            PrepareReporter::from(printer).with_length(remote.len() as u64),
        ));

        preparer
            .prepare(remote.clone(), state.in_flight(), &resolution)
            .await?
    };

    let total_wheels = wheels.into_iter().chain(cached).collect::<Vec<_>>();
    if !total_wheels.is_empty() {
        let output_path = target.root();
        for wheel in &total_wheels {
            let wheel_output = output_path.join(wheel.filename().to_string());
            let wheel_dir = wheel.path();
            let file = File::create(wheel_output)?;
            let mut zip = ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default()
                .unix_permissions(664)
                .compression_method(CompressionMethod::Deflated);

            let source_dir = wheel_dir;

            for entry in WalkDir::new(source_dir).min_depth(1) {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    continue;
                }

                let name = path.strip_prefix(source_dir).unwrap().to_string_lossy();

                zip.start_file(name.to_string(), options)?;
                let mut file = File::open(path)?;
                io::copy(&mut file, &mut zip)?;
            }

            zip.finish()?;
        }
    }

    Ok(ExitStatus::Success)
}

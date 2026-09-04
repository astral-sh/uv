use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::io::Write;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Result, anyhow};
use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::FxHashSet;
use tracing::debug;

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, FlatIndexClient, RegistryClientBuilder, SimpleMetadataCache};
use uv_configuration::{
    BuildIsolation, BuildOptions, Concurrency, Constraints, ExcludeDependency, ExtrasSpecification,
    IndexStrategy, NoBinary, NoBuild, NoSources, Override, PipCompileFormat, Reinstall, Upgrade,
};
use uv_configuration::{KeyringProviderType, TargetTriple};
use uv_dispatch::{BuildDispatch, SharedState};
use uv_distribution::LoweredExtraBuildDependencies;
use uv_distribution_types::{
    ConfigSettings, DependencyMetadata, ExtraBuildVariables, HashGeneration, Index, IndexLocations,
    NameRequirementSpecification, Origin, PackageConfigSettings, Requirement, RequiresPython,
    Verbatim,
};
use uv_fs::{CWD, Simplified};
use uv_git::ResolvedRepositoryReference;
use uv_install_wheel::LinkMode;
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifier};
use uv_pep508::{
    MarkerEnvironment, MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString,
    MarkerValueVersion,
};
use uv_preview::{Preview, PreviewFeature};
use uv_pypi_types::{Conflicts, SupportedEnvironments};
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest, PythonVersion, VersionRequest,
};
use uv_requirements::{
    GroupsSpecification, LockedRequirements, RequirementsSource, RequirementsSpecification,
    is_pylock_toml, read_pylock_toml_requirements, read_requirements_txt,
};
use uv_resolver::{
    AnnotationStyle, DependencyMode, DisplayResolutionGraph, DisplayResolutionMatrix,
    ExactTargetOutput, ExcludeNewer, FlatIndex, ForkStrategy, InMemoryIndex, OptionsBuilder,
    Prerelease, PylockToml, PythonRequirement, ResolutionMode, ResolverEnvironment, ResolverOutput,
};
use uv_settings::PythonInstallMirrors;
use uv_static::EnvVars;
use uv_torch::{AmdGpuArchitecture, TorchMode, TorchStrategy};
use uv_types::{EmptyInstalledPackages, HashStrategy, SourceTreeEditablePolicy};
use uv_warnings::warn_user;
use uv_workspace::WorkspaceCache;
use uv_workspace::pyproject::ExtraBuildDependencies;

use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::pip::{operations, resolution_markers, resolution_tags};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{ExitStatus, OutputWriter, diagnostics};
use crate::printer::Printer;

/// Inputs parsed once and reused by exact-target compiles in the same invocation.
pub(crate) struct ParsedCompileInputs {
    requirements: RequirementsSpecification,
    build_constraints: Vec<NameRequirementSpecification>,
}

impl ParsedCompileInputs {
    pub(crate) async fn from_sources(
        requirements: &[RequirementsSource],
        constraints: &[RequirementsSource],
        overrides: &[RequirementsSource],
        excludes: &[RequirementsSource],
        build_constraints: &[RequirementsSource],
        groups: &GroupsSpecification,
        client_builder: &BaseClientBuilder<'_>,
    ) -> Result<Self> {
        Ok(Self {
            requirements: RequirementsSpecification::from_sources(
                requirements,
                constraints,
                overrides,
                excludes,
                Some(groups),
                client_builder,
            )
            .await?,
            build_constraints: operations::read_constraints(build_constraints, client_builder)
                .await?,
        })
    }
}

/// The previous output, read once and used as a preference by every exact target.
#[derive(Default)]
pub(crate) struct PriorLockSnapshot {
    locked: Option<LockedRequirements>,
}

impl PriorLockSnapshot {
    /// Parse the existing output at most once, including any recursive requirements files.
    async fn read(
        &mut self,
        output_file: Option<&Path>,
        format: PipCompileFormat,
        upgrade: &Upgrade,
    ) -> Result<LockedRequirements> {
        if let Some(locked) = &self.locked {
            return Ok(locked.clone());
        }
        let locked = if let Some(output_file) = output_file.filter(|path| path.exists()) {
            read_existing_lock(output_file, format, upgrade).await?
        } else {
            LockedRequirements::default()
        };
        self.locked = Some(locked.clone());
        Ok(locked)
    }
}

/// One exact resolution to include in a combined `requirements.txt` output.
pub(crate) struct ExactTargetResolution {
    resolution: ResolverOutput,
    resolver_env: ResolverEnvironment,
    selector: MarkerTree,
    relevant_markers: Option<MarkerTree>,
    index_locations: IndexLocations,
    build_options: BuildOptions,
}

/// A marker for the interpreter and platform used by one exact-target resolution.
///
/// Wheel compatibility tags are not PEP 508 environment markers. The caller rejects
/// overlapping selectors (for example, two manylinux versions of the same architecture).
fn exact_target_selector(
    marker_env: &MarkerEnvironment,
    python_version: Option<&PythonVersion>,
) -> MarkerTree {
    let string_marker = |key, value: &str| {
        MarkerTree::expression(MarkerExpression::String {
            key,
            operator: MarkerOperator::Equal,
            value: value.into(),
        })
    };
    let version_marker = |key, version| {
        MarkerTree::expression(MarkerExpression::Version {
            key,
            specifier: VersionSpecifier::equals_version(version),
        })
    };

    let machine = string_marker(
        MarkerValueString::PlatformMachine,
        marker_env.platform_machine(),
    );
    // Native Windows reports AMD64, while cross-platform target metadata uses x86_64.
    let machine =
        if marker_env.sys_platform() == "win32" && marker_env.platform_machine() == "x86_64" {
            machine.or(string_marker(MarkerValueString::PlatformMachine, "AMD64"))
        } else {
            machine
        };
    let version = if python_version.is_some_and(|version| version.patch().is_some()) {
        version_marker(
            MarkerValueVersion::PythonFullVersion,
            marker_env.python_full_version().version.clone(),
        )
    } else {
        version_marker(
            MarkerValueVersion::PythonVersion,
            marker_env.python_version().version.clone(),
        )
    };

    string_marker(MarkerValueString::SysPlatform, marker_env.sys_platform())
        .and(machine)
        .and(string_marker(
            MarkerValueString::ImplementationName,
            marker_env.implementation_name(),
        ))
        .and(version)
}

/// Load the preferred pins and Git revisions from one existing output file.
async fn read_existing_lock(
    output_file: &Path,
    format: PipCompileFormat,
    upgrade: &Upgrade,
) -> Result<LockedRequirements> {
    match format {
        PipCompileFormat::RequirementsTxt => Ok(LockedRequirements::from_preferences(
            read_requirements_txt(output_file, upgrade).await?,
        )),
        PipCompileFormat::PylockToml => {
            Ok(read_pylock_toml_requirements(output_file, upgrade).await?)
        }
    }
}

/// Resolve a set of requirements into a set of pinned versions.
#[expect(clippy::fn_params_excessive_bools)]
pub(crate) async fn pip_compile(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    excludes: &[RequirementsSource],
    build_constraints: &[RequirementsSource],
    constraints_from_workspace: Vec<Requirement>,
    overrides_from_workspace: Vec<Override<Requirement>>,
    excludes_from_workspace: Vec<ExcludeDependency>,
    build_constraints_from_workspace: Vec<Requirement>,
    environments: SupportedEnvironments,
    required_environments: SupportedEnvironments,
    extras: ExtrasSpecification,
    groups: GroupsSpecification,
    output_file: Option<&Path>,
    format: Option<PipCompileFormat>,
    resolution_mode: ResolutionMode,
    prerelease: Prerelease,
    fork_strategy: ForkStrategy,
    dependency_mode: DependencyMode,
    upgrade: Upgrade,
    generate_hashes: bool,
    no_emit_packages: Vec<PackageName>,
    include_extras: bool,
    include_markers: bool,
    include_annotations: bool,
    include_header: bool,
    custom_compile_command: Option<String>,
    include_index_url: bool,
    include_find_links: bool,
    include_build_options: bool,
    include_marker_expression: bool,
    include_index_annotation: bool,
    index_locations: IndexLocations,
    index_strategy: IndexStrategy,
    torch_backend: Option<TorchMode>,
    cuda_driver_version: Option<Version>,
    amd_gpu_architecture: Option<AmdGpuArchitecture>,
    dependency_metadata: DependencyMetadata,
    keyring_provider: KeyringProviderType,
    client_builder: &BaseClientBuilder<'_>,
    parsed_inputs: Option<&ParsedCompileInputs>,
    prior_lock_snapshot: Option<&mut PriorLockSnapshot>,
    simple_metadata_cache: Option<SimpleMetadataCache>,
    exact_targets: Option<&mut Vec<ExactTargetResolution>>,
    config_settings: ConfigSettings,
    config_settings_package: PackageConfigSettings,
    build_isolation: BuildIsolation,
    extra_build_dependencies: &ExtraBuildDependencies,
    extra_build_variables: &ExtraBuildVariables,
    build_options: BuildOptions,
    install_mirrors: PythonInstallMirrors,
    mut python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    python_downloads: PythonDownloads,
    universal: bool,
    exclude_newer: ExcludeNewer,
    sources: NoSources,
    annotation_style: AnnotationStyle,
    link_mode: LinkMode,
    mut python: Option<String>,
    system: bool,
    python_preference: PythonPreference,
    concurrency: Concurrency,
    quiet: bool,
    cache: Cache,
    workspace_cache: WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    // If the user provides a `pyproject.toml` or other TOML file as the output file, raise an
    // error.
    if output_file
        .and_then(Path::file_name)
        .is_some_and(|name| name.eq_ignore_ascii_case("pyproject.toml"))
    {
        return Err(anyhow!(
            "`pyproject.toml` is not a supported output format for `{}` (only `requirements.txt`-style output is supported)",
            "uv pip compile".green()
        ));
    }

    // Determine the output format.
    let format = format.unwrap_or_else(|| {
        let extension = output_file.and_then(Path::extension);
        if extension.is_some_and(|ext| ext.eq_ignore_ascii_case("txt")) {
            PipCompileFormat::RequirementsTxt
        } else if extension.is_some_and(|ext| ext.eq_ignore_ascii_case("toml")) {
            PipCompileFormat::PylockToml
        } else {
            PipCompileFormat::RequirementsTxt
        }
    });

    if exact_targets.is_some() && matches!(format, PipCompileFormat::PylockToml) {
        return Err(anyhow!(
            "Multiple exact Python targets cannot be written to a single `pylock.toml` file"
        ));
    }

    // If the user is exporting to PEP 751, ensure the filename matches the specification.
    if matches!(format, PipCompileFormat::PylockToml) {
        if let Some(file_name) = output_file
            .and_then(Path::file_name)
            .and_then(OsStr::to_str)
        {
            if !is_pylock_toml(file_name) {
                return Err(anyhow!(
                    "Expected the output filename to be `pylock.toml` or `pylock.<name>.toml`, where `<name>` is non-empty and contains no dots; found `{file_name}`",
                ));
            }
        }
    }

    // Respect `UV_PYTHON`
    if python.is_none() && python_version.is_none() {
        if let Ok(request) = std::env::var(EnvVars::UV_PYTHON) {
            if !request.is_empty() {
                python = Some(request);
            }
        }
    }

    // If `--python` / `-p` is a simple Python version request, we treat it as `--python-version`
    // for backwards compatibility. `-p` was previously aliased to `--python-version` but changed to
    // `--python` for consistency with the rest of the CLI in v0.6.0. Since we assume metadata is
    // consistent across wheels, it's okay for us to build wheels (to determine metadata) with an
    // alternative Python interpreter as long as we solve with the proper Python version tags.
    if python_version.is_none() {
        if let Some(request) = python.as_ref() {
            if let Ok(version) = PythonVersion::from_str(request) {
                python_version = Some(version);
                python = None;
            }
        }
    }

    // If the user requests `extras` but does not provide a valid source (e.g., a `pyproject.toml`),
    // return an error.
    if !extras.is_empty() && !requirements.iter().any(RequirementsSource::allows_extras) {
        return Err(anyhow!(
            "Requesting extras requires a `pyproject.toml`, `setup.cfg`, or `setup.py` file."
        ));
    }

    let client_builder = client_builder.clone().keyring(keyring_provider);

    // Read all requirements from the provided sources.
    let RequirementsSpecification {
        project,
        requirements,
        constraints,
        overrides,
        mut override_dependencies,
        excludes,
        pylock,
        source_trees,
        groups,
        extras: used_extras,
        index_url,
        extra_index_urls,
        no_index,
        require_hashes: _,
        find_links,
        no_binary,
        no_build,
    } = if let Some(parsed_inputs) = parsed_inputs {
        parsed_inputs.requirements.clone()
    } else {
        RequirementsSpecification::from_sources(
            requirements,
            constraints,
            overrides,
            excludes,
            Some(&groups),
            &client_builder,
        )
        .await?
    };

    override_dependencies.extend(overrides_from_workspace);

    // Reject `pylock.toml` files, which are valid outputs but not inputs.
    if pylock.is_some() {
        return Err(anyhow!(
            "`pylock.toml` is not a supported input format for `uv pip compile`"
        ));
    }

    let constraints = constraints
        .iter()
        .cloned()
        .chain(
            constraints_from_workspace
                .into_iter()
                .map(NameRequirementSpecification::from),
        )
        .collect();

    let excludes: Vec<ExcludeDependency> = excludes
        .into_iter()
        .chain(excludes_from_workspace)
        .collect();

    // Read build constraints.
    let build_constraints = if let Some(parsed_inputs) = parsed_inputs {
        parsed_inputs.build_constraints.clone()
    } else {
        operations::read_constraints(build_constraints, &client_builder).await?
    };
    let build_constraints: Vec<NameRequirementSpecification> = build_constraints
        .into_iter()
        .chain(
            build_constraints_from_workspace
                .into_iter()
                .map(NameRequirementSpecification::from),
        )
        .collect();

    // If all the metadata could be statically resolved, validate that every extra was used. If we
    // need to resolve metadata via PEP 517, we don't know which extras are used until much later.
    if source_trees.is_empty() {
        let mut unused_extras = extras
            .explicit_names()
            .filter(|extra| !used_extras.contains(extra))
            .collect::<Vec<_>>();
        if !unused_extras.is_empty() {
            unused_extras.sort_unstable();
            unused_extras.dedup();
            let s = if unused_extras.len() == 1 { "" } else { "s" };
            return Err(anyhow!(
                "Requested extra{s} not found: {}",
                unused_extras.iter().join(", ")
            ));
        }
    }

    // Find an interpreter to use for building distributions
    let environment_preference = EnvironmentPreference::from_system_flag(system, false);
    let python_preference = python_preference.with_system_flag(system);
    let reporter = PythonDownloadReporter::single(printer);
    let interpreter = if let Some(python) = python.as_ref() {
        let request = PythonRequest::parse(python);
        PythonInstallation::find_or_download(
            Some(&request),
            environment_preference,
            python_preference,
            python_downloads,
            &client_builder,
            &cache,
            Some(&reporter),
            install_mirrors.python_install_mirror.as_deref(),
            install_mirrors.pypy_install_mirror.as_deref(),
            install_mirrors.python_downloads_json_url.as_deref(),
        )
        .await
    } else {
        // TODO(zanieb): The split here hints at a problem with the request abstraction; we should
        // be able to use `PythonInstallation::find(...)` here.
        let request = if let Some(version) = python_version.as_ref() {
            // TODO(zanieb): We should consolidate `VersionRequest` and `PythonVersion`
            PythonRequest::Version(VersionRequest::from(version))
        } else {
            PythonRequest::default()
        };
        PythonInstallation::find_best(
            &request,
            environment_preference,
            python_preference,
            python_downloads,
            &client_builder,
            &cache,
            Some(&reporter),
            install_mirrors.python_install_mirror.as_deref(),
            install_mirrors.pypy_install_mirror.as_deref(),
            install_mirrors.python_downloads_json_url.as_deref(),
        )
        .await
    }?
    .into_interpreter();

    debug!(
        "Using Python {} interpreter at {} for builds",
        interpreter.python_version(),
        interpreter.sys_executable().user_display().cyan()
    );

    if let Some(python_version) = python_version.as_ref() {
        // If the requested version does not match the version we're using warn the user
        // _unless_ they have not specified a patch version and that is the only difference
        // _or_ if builds are disabled
        let matches_without_patch = {
            python_version.major() == interpreter.python_major()
                && python_version.minor() == interpreter.python_minor()
        };
        if no_build.is_none()
            && python.is_none()
            && python_version.version() != interpreter.python_version()
            && (python_version.patch().is_some() || !matches_without_patch)
        {
            warn_user!(
                "The requested Python version {} is not available; {} will be used to build dependencies instead.",
                python_version.version(),
                interpreter.python_version(),
            );
        }
    }

    // Create the shared state.
    let state = SharedState::default();

    // If we're resolving against a different Python version, use a separate index. Source
    // distributions will be built against the installed version, and so the index may contain
    // different package priorities than in the top-level resolution.
    let top_level_index = if python_version.is_some() {
        InMemoryIndex::default()
    } else {
        state.index().clone()
    };

    // Determine the Python requirement, if the user requested a specific version.
    let python_requirement = if universal {
        let requires_python = if let Some(python_version) = python_version.as_ref() {
            RequiresPython::greater_than_equal_version(&python_version.version)
        } else {
            let version = interpreter.python_minor_version();
            RequiresPython::greater_than_equal_version(&version)
        };
        PythonRequirement::from_requires_python(&interpreter, requires_python)
    } else if let Some(python_version) = python_version.as_ref() {
        PythonRequirement::from_python_version(&interpreter, python_version)
    } else {
        PythonRequirement::from_interpreter(&interpreter)
    };

    let artifact_environments = if universal {
        SupportedEnvironments::from_markers(
            environments
                .iter()
                .chain(required_environments.iter())
                .copied()
                .collect(),
        )
    } else {
        SupportedEnvironments::default()
    };

    // Determine the environment for the resolution.
    let (tags, resolver_env) = if universal {
        (
            None,
            ResolverEnvironment::universal(environments.into_markers()),
        )
    } else {
        let tags = resolution_tags(
            python_version.as_ref(),
            python_platform.as_ref(),
            &interpreter,
        )?;
        let marker_env = resolution_markers(
            python_version.as_ref(),
            python_platform.as_ref(),
            &interpreter,
        );
        (Some(tags), ResolverEnvironment::specific(marker_env))
    };

    let exact_selector = if let Some(exact_targets) = exact_targets.as_deref() {
        let marker_env = resolver_env
            .marker_environment()
            .ok_or_else(|| anyhow!("Multiple Python targets require exact-platform resolution"))?;
        // A 32-bit interpreter running on 64-bit Windows reports the host's machine (AMD64),
        // not the interpreter's wheel architecture, so no PEP 508 marker can select it reliably.
        if marker_env.sys_platform() == "win32" && marker_env.platform_machine() == "x86" {
            return Err(anyhow!(
                "A 32-bit Windows Python target cannot be distinguished by environment markers in a single requirements file"
            ));
        }
        let selector = exact_target_selector(marker_env, python_version.as_ref());
        if exact_targets
            .iter()
            .any(|target| !selector.is_disjoint(target.selector))
        {
            return Err(anyhow!(
                "Exact Python targets with different wheel tags cannot be distinguished by environment markers in a single requirements file; use separate invocations and output files"
            ));
        }
        Some(selector)
    } else {
        None
    };

    // Generate, but don't enforce hashes for the requirements. PEP 751 _requires_ a hash to be
    // present, but otherwise, we omit them by default.
    let hasher = if generate_hashes || matches!(format, PipCompileFormat::PylockToml) {
        HashStrategy::generate(HashGeneration::All)
    } else {
        HashStrategy::default()
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
            TorchStrategy::from_mode(
                mode,
                python_platform
                    .map(TargetTriple::platform)
                    .as_ref()
                    .unwrap_or(interpreter.platform())
                    .os(),
                cuda_driver_version,
                amd_gpu_architecture,
            )
        })
        .transpose()?;

    // Initialize the registry client.
    let registry_client = RegistryClientBuilder::new(client_builder.clone(), cache.clone())
        .index_locations(index_locations.clone())
        .index_strategy(index_strategy)
        .torch_backend(torch_backend.clone())
        .markers(interpreter.markers())
        .platform(interpreter.platform());
    let registry_client = if let Some(simple_metadata_cache) = simple_metadata_cache {
        registry_client.simple_metadata_cache(simple_metadata_cache)
    } else {
        registry_client
    };
    let client = registry_client.build()?;

    // Read the lockfile, if present.
    let LockedRequirements { preferences, git } =
        if let Some(prior_lock_snapshot) = prior_lock_snapshot {
            prior_lock_snapshot
                .read(output_file, format, &upgrade)
                .await?
        } else if let Some(output_file) = output_file.filter(|output_file| output_file.exists()) {
            read_existing_lock(output_file, format, &upgrade).await?
        } else {
            LockedRequirements::default()
        };

    // Populate the Git resolver.
    for ResolvedRepositoryReference { reference, sha } in git {
        debug!("Inserting Git reference into resolver: `{reference:?}` at `{sha}`");
        state.git().insert(reference, sha);
    }

    // Combine the `--no-binary` and `--no-build` flags from the requirements files.
    let build_options = build_options.combine(no_binary, no_build);

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(client.cached_client(), client.connectivity(), &cache);
        let entries = client
            .fetch_all(index_locations.flat_indexes().map(Index::url))
            .await?;
        FlatIndex::from_entries(entries, tags.as_deref(), &hasher, &build_options)
    };

    // Determine whether to enable build isolation.
    let environment;
    let types_build_isolation = match build_isolation {
        BuildIsolation::Isolate => uv_types::BuildIsolation::Isolated,
        BuildIsolation::Shared => {
            environment = PythonEnvironment::from_interpreter(interpreter.clone());
            uv_types::BuildIsolation::Shared(&environment)
        }
        BuildIsolation::SharedPackage(ref packages) => {
            environment = PythonEnvironment::from_interpreter(interpreter.clone());
            uv_types::BuildIsolation::SharedPackage(&environment, packages)
        }
    };

    // Don't enforce hashes in `pip compile`.
    let build_hashes = HashStrategy::default();
    let build_constraints = Constraints::from_requirements(
        build_constraints
            .iter()
            .map(|constraint| constraint.requirement.clone()),
    );

    // Lower the extra build dependencies, if any.
    let extra_build_requires =
        LoweredExtraBuildDependencies::from_non_lowered(extra_build_dependencies.clone())
            .into_inner();

    // Create a build dispatch.
    let build_dispatch = BuildDispatch::new(
        &client,
        &cache,
        &build_constraints,
        &interpreter,
        &index_locations,
        &flat_index,
        &dependency_metadata,
        state,
        index_strategy,
        &config_settings,
        &config_settings_package,
        types_build_isolation,
        &extra_build_requires,
        extra_build_variables,
        link_mode,
        &build_options,
        &build_hashes,
        exclude_newer.clone(),
        sources,
        SourceTreeEditablePolicy::Project,
        workspace_cache,
        concurrency.clone(),
        preview,
    );

    let options = OptionsBuilder::new()
        .resolution_mode(resolution_mode)
        .prerelease(prerelease)
        .fork_strategy(fork_strategy)
        .dependency_mode(dependency_mode)
        .exclude_newer(exclude_newer.clone())
        .index_strategy(index_strategy)
        .torch_backend(torch_backend)
        .build_options(build_options.clone())
        .artifact_environments(artifact_environments)
        .build();

    // Resolve the requirements.
    let mut resolution = match operations::resolve(
        requirements,
        constraints,
        overrides,
        override_dependencies,
        excludes,
        source_trees,
        project,
        BTreeSet::default(),
        &extras,
        &groups,
        preferences,
        EmptyInstalledPackages,
        &hasher,
        &Reinstall::None,
        &upgrade,
        tags.as_deref(),
        resolver_env.clone(),
        python_requirement,
        interpreter.markers(),
        Conflicts::empty(),
        &client,
        &flat_index,
        &top_level_index,
        &build_dispatch,
        &concurrency,
        options,
        Box::new(DefaultResolveLogger),
        printer,
    )
    .await
    {
        Ok((resolution, _)) => resolution,
        Err(err) => {
            return diagnostics::OperationDiagnostic::default()
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
        }
    };

    if generate_hashes && preview.is_enabled(PreviewFeature::ArtifactHashFiltering) {
        resolution.retain_allowed_distribution_hashes(&build_options);
    }

    // Keep the resolved graphs until all exact targets have succeeded. The same existing output
    // remains available as the prior-lock preference for every target, and we write only once.
    if let Some(exact_targets) = exact_targets {
        let selector = exact_selector.ok_or_else(|| anyhow!("Expected an exact Python target"))?;
        let relevant_markers = if include_marker_expression {
            resolver_env
                .marker_environment()
                .map(|marker_env| {
                    resolution
                        .marker_tree(&top_level_index, marker_env)
                        .map(|marker| marker.and(selector))
                })
                .transpose()?
        } else {
            None
        };
        operations::diagnose_resolution(resolution.diagnostics(), printer)?;
        exact_targets.push(ExactTargetResolution {
            resolution,
            resolver_env,
            selector,
            relevant_markers,
            index_locations,
            build_options,
        });
        return Ok(ExitStatus::Success);
    }

    // Write the resolved dependencies to the output channel.
    let mut writer = OutputWriter::new(!quiet || output_file.is_none(), output_file);

    write_compile_header(
        &mut writer,
        include_header,
        include_index_url,
        include_find_links,
        custom_compile_command,
    )?;

    match format {
        PipCompileFormat::RequirementsTxt => {
            let relevant_markers = if include_marker_expression {
                resolver_env
                    .marker_environment()
                    .map(|marker_env| resolution.marker_tree(&top_level_index, marker_env))
                    .transpose()?
            } else {
                None
            };
            write_requirements_preamble(
                &mut writer,
                relevant_markers,
                &index_locations,
                &build_options,
                include_index_url,
                include_find_links,
                include_build_options,
            )?;

            write!(
                writer,
                "{}",
                DisplayResolutionGraph::new(
                    &resolution,
                    &resolver_env,
                    &no_emit_packages,
                    generate_hashes,
                    include_extras,
                    include_markers || universal,
                    include_annotations,
                    include_index_annotation,
                    annotation_style,
                )
            )?;
        }
        PipCompileFormat::PylockToml => {
            if include_marker_expression {
                warn_user!(
                    "The `--emit-marker-expression` option is not supported for `pylock.toml` output"
                );
            }
            if include_index_url {
                warn_user!(
                    "The `--emit-index-url` option is not supported for `pylock.toml` output"
                );
            }
            if include_find_links {
                warn_user!(
                    "The `--emit-find-links` option is not supported for `pylock.toml` output"
                );
            }
            if include_build_options {
                warn_user!(
                    "The `--emit-build-options` option is not supported for `pylock.toml` output"
                );
            }
            if include_index_annotation {
                warn_user!(
                    "The `--emit-index-annotation` option is not supported for `pylock.toml` output"
                );
            }

            // Determine the directory relative to which the output file should be written.
            let output_file = output_file.map(std::path::absolute).transpose()?;
            let install_path = if let Some(output_file) = output_file.as_deref() {
                output_file.parent().unwrap()
            } else {
                &*CWD
            };

            // Convert the resolution to a `pylock.toml` file.
            let export = PylockToml::from_resolution(
                &resolution,
                &no_emit_packages,
                install_path,
                tags.as_deref(),
                &build_options,
            )?;
            write!(writer, "{}", export.to_toml()?)?;
        }
    }

    // If any "unsafe" packages were excluded, notify the user.
    let excluded = no_emit_packages
        .into_iter()
        .filter(|name| resolution.contains(name))
        .collect::<Vec<_>>();
    if include_annotations && !excluded.is_empty() {
        writeln!(writer)?;
        writeln!(
            writer,
            "{}",
            "# The following packages were excluded from the output:".green()
        )?;
        for package in excluded {
            writeln!(writer, "# {package}")?;
        }
    }

    // Commit the output to disk.
    writer.commit().await?;

    // Notify the user of any resolution diagnostics.
    operations::diagnose_resolution(resolution.diagnostics(), printer)?;

    Ok(ExitStatus::Success)
}

/// Write all exact-platform solutions as one marker-qualified requirements file.
#[expect(clippy::fn_params_excessive_bools)]
pub(crate) async fn write_pip_compile_matrix(
    targets: &[ExactTargetResolution],
    output_file: Option<&Path>,
    no_emit_packages: &[PackageName],
    generate_hashes: bool,
    include_extras: bool,
    include_annotations: bool,
    include_header: bool,
    custom_compile_command: Option<String>,
    include_index_url: bool,
    include_find_links: bool,
    include_build_options: bool,
    include_marker_expression: bool,
    include_index_annotation: bool,
    annotation_style: AnnotationStyle,
    quiet: bool,
) -> Result<()> {
    let first = targets
        .first()
        .ok_or_else(|| anyhow!("Expected at least one exact Python target"))?;
    if targets.iter().skip(1).any(|target| {
        target.index_locations != first.index_locations
            || target.build_options != first.build_options
    }) {
        return Err(anyhow!(
            "Target-specific indexes or build options cannot be represented in a single requirements file"
        ));
    }
    let outputs = targets
        .iter()
        .map(|target| ExactTargetOutput {
            resolution: &target.resolution,
            environment: &target.resolver_env,
            selector: target.selector,
        })
        .collect::<Vec<_>>();
    // Validate the merged requirements before writing anything, including stdout.
    let display = DisplayResolutionMatrix::new(
        &outputs,
        no_emit_packages,
        generate_hashes,
        include_extras,
        include_annotations,
        include_index_annotation,
        annotation_style,
    )?;
    let mut writer = OutputWriter::new(!quiet || output_file.is_none(), output_file);
    write_compile_header(
        &mut writer,
        include_header,
        include_index_url,
        include_find_links,
        custom_compile_command,
    )?;

    let relevant_markers = include_marker_expression.then(|| {
        targets
            .iter()
            .filter_map(|target| target.relevant_markers)
            .fold(MarkerTree::FALSE, MarkerTree::or)
    });
    write_requirements_preamble(
        &mut writer,
        relevant_markers,
        &first.index_locations,
        &first.build_options,
        include_index_url,
        include_find_links,
        include_build_options,
    )?;

    write!(writer, "{display}")?;

    if include_annotations {
        let excluded = no_emit_packages
            .iter()
            .filter(|name| {
                targets
                    .iter()
                    .any(|target| target.resolution.contains(name))
            })
            .collect::<Vec<_>>();
        if !excluded.is_empty() {
            writeln!(writer)?;
            writeln!(
                writer,
                "{}",
                "# The following packages were excluded from the output:".green()
            )?;
            for package in excluded {
                writeln!(writer, "# {package}")?;
            }
        }
    }

    writer.commit().await?;
    Ok(())
}

/// Write the command which generated a requirements file or `pylock.toml`.
fn write_compile_header(
    writer: &mut impl Write,
    include_header: bool,
    include_index_url: bool,
    include_find_links: bool,
    custom_compile_command: Option<String>,
) -> std::io::Result<()> {
    if include_header {
        writeln!(
            writer,
            "{}",
            "# This file was autogenerated by uv via the following command:".green()
        )?;
        writeln!(
            writer,
            "{}",
            format!(
                "#    {}",
                cmd(
                    include_index_url,
                    include_find_links,
                    custom_compile_command
                )
            )
            .green()
        )?;
    }
    Ok(())
}

/// Write the shared marker note and index/build options before pinned requirements.
fn write_requirements_preamble(
    writer: &mut impl Write,
    relevant_markers: Option<MarkerTree>,
    index_locations: &IndexLocations,
    build_options: &BuildOptions,
    include_index_url: bool,
    include_find_links: bool,
    include_build_options: bool,
) -> std::io::Result<()> {
    if let Some(relevant_markers) = relevant_markers.and_then(MarkerTree::contents) {
        writeln!(
            writer,
            "{}",
            "# Pinned dependencies known to be valid for:".green()
        )?;
        writeln!(writer, "{}", format!("#    {relevant_markers}").green())?;
    }

    let mut wrote_preamble = false;
    if include_index_url {
        if let Some(index) = index_locations.default_index() {
            writeln!(writer, "--index-url {}", index.url().verbatim())?;
            wrote_preamble = true;
        }
        let mut seen = FxHashSet::default();
        for extra_index in index_locations.implicit_indexes() {
            if seen.insert(extra_index.url()) {
                writeln!(writer, "--extra-index-url {}", extra_index.url().verbatim())?;
                wrote_preamble = true;
            }
        }
    }
    if include_find_links {
        for flat_index in index_locations.flat_indexes() {
            writeln!(writer, "--find-links {}", flat_index.url().verbatim())?;
            wrote_preamble = true;
        }
    }
    if include_build_options {
        match build_options.no_binary() {
            NoBinary::None => {}
            NoBinary::All => {
                writeln!(writer, "--no-binary :all:")?;
                wrote_preamble = true;
            }
            NoBinary::Packages(packages) => {
                for package in packages {
                    writeln!(writer, "--no-binary {package}")?;
                    wrote_preamble = true;
                }
            }
        }
        match build_options.no_build() {
            NoBuild::None => {}
            NoBuild::All => {
                writeln!(writer, "--only-binary :all:")?;
                wrote_preamble = true;
            }
            NoBuild::Packages(packages) => {
                for package in packages {
                    writeln!(writer, "--only-binary {package}")?;
                    wrote_preamble = true;
                }
            }
        }
    }
    if wrote_preamble {
        writeln!(writer)?;
    }
    Ok(())
}

/// Format the uv command used to generate the output file.
fn cmd(
    include_index_url: bool,
    include_find_links: bool,
    custom_compile_command: Option<String>,
) -> String {
    if let Some(cmd_str) = custom_compile_command {
        return cmd_str;
    }
    let args = env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().to_string())
        .scan(None, move |skip_next, arg| {
            if matches!(skip_next, Some(true)) {
                // Reset state; skip this iteration.
                *skip_next = None;
                return Some(None);
            }

            // Skip any index URLs, unless requested.
            if !include_index_url {
                if arg.starts_with("--extra-index-url=")
                    || arg.starts_with("--index-url=")
                    || arg.starts_with("-i=")
                    || arg.starts_with("--index=")
                    || arg.starts_with("--default-index=")
                {
                    // Reset state; skip this iteration.
                    *skip_next = None;
                    return Some(None);
                }

                // Mark the next item as (to be) skipped.
                if arg == "--index-url"
                    || arg == "--extra-index-url"
                    || arg == "-i"
                    || arg == "--index"
                    || arg == "--default-index"
                {
                    *skip_next = Some(true);
                    return Some(None);
                }
            }

            // Skip any `--find-links` URLs, unless requested.
            if !include_find_links {
                // Always skip the `--find-links` and mark the next item to be skipped
                if arg == "--find-links" || arg == "-f" {
                    *skip_next = Some(true);
                    return Some(None);
                }

                // Skip only this argument if option and value are together
                if arg.starts_with("--find-links=") || arg.starts_with("-f") {
                    // Reset state; skip this iteration.
                    *skip_next = None;
                    return Some(None);
                }
            }

            // Always skip the `--upgrade` flag.
            if arg == "--upgrade" || arg == "-U" {
                *skip_next = None;
                return Some(None);
            }

            // Always skip the `--upgrade-package` and mark the next item to be skipped
            if arg == "--upgrade-package" || arg == "-P" {
                *skip_next = Some(true);
                return Some(None);
            }

            // Skip only this argument if option and value are together
            if arg.starts_with("--upgrade-package=") || arg.starts_with("-P") {
                // Reset state; skip this iteration.
                *skip_next = None;
                return Some(None);
            }

            // Always skip the `--quiet` flag.
            if arg == "--quiet" || arg == "-q" {
                *skip_next = None;
                return Some(None);
            }

            // Always skip the `--verbose` flag.
            if arg == "--verbose" || arg == "-v" {
                *skip_next = None;
                return Some(None);
            }

            // Always skip the `--no-progress` flag.
            if arg == "--no-progress" {
                *skip_next = None;
                return Some(None);
            }

            // Always skip the `--native-tls` flag.
            if arg == "--native-tls" {
                *skip_next = None;
                return Some(None);
            }

            // Return the argument.
            Some(Some(arg))
        })
        .flatten()
        .join(" ");
    format!("uv {args}")
}

use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Result, anyhow};
use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::FxHashSet;
use tracing::debug;

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    BuildOptions, Concurrency, ConfigSettings, Constraints, ExportFormat, ExtrasSpecification,
    IndexStrategy, NoBinary, NoBuild, PackageConfigSettings, Preview, PreviewFeatures, Reinstall,
    SourceStrategy, Upgrade,
};
use uv_configuration::{KeyringProviderType, TargetTriple};
use uv_dispatch::{BuildDispatch, SharedState};
use uv_distribution::LoweredExtraBuildDependencies;
use uv_distribution_types::{
    DependencyMetadata, HashGeneration, Index, IndexLocations, NameRequirementSpecification,
    Origin, Requirement, RequiresPython, UnresolvedRequirementSpecification, Verbatim,
};
use uv_fs::{CWD, Simplified};
use uv_git::ResolvedRepositoryReference;
use uv_install_wheel::LinkMode;
use uv_normalize::PackageName;
use uv_pypi_types::{Conflicts, SupportedEnvironments};
use uv_python::{
    EnvironmentPreference, PythonEnvironment, PythonInstallation, PythonPreference, PythonRequest,
    PythonVersion, VersionRequest,
};
use uv_requirements::upgrade::{LockedRequirements, read_pylock_toml_requirements};
use uv_requirements::{
    GroupsSpecification, RequirementsSource, RequirementsSpecification, is_pylock_toml,
    upgrade::read_requirements_txt,
};
use uv_resolver::{
    AnnotationStyle, DependencyMode, DisplayResolutionGraph, ExcludeNewer, FlatIndex, ForkStrategy,
    InMemoryIndex, OptionsBuilder, PrereleaseMode, PylockToml, PythonRequirement, ResolutionMode,
    ResolverEnvironment,
};
use uv_torch::{TorchMode, TorchStrategy};
use uv_types::{BuildIsolation, EmptyInstalledPackages, HashStrategy};
use uv_warnings::{warn_user, warn_user_once};
use uv_workspace::WorkspaceCache;
use uv_workspace::pyproject::ExtraBuildDependencies;

use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::pip::{operations, resolution_environment};
use crate::commands::{ExitStatus, OutputWriter, diagnostics};
use crate::printer::Printer;
use crate::settings::NetworkSettings;

/// Resolve a set of requirements into a set of pinned versions.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn pip_compile(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    build_constraints: &[RequirementsSource],
    constraints_from_workspace: Vec<Requirement>,
    overrides_from_workspace: Vec<Requirement>,
    build_constraints_from_workspace: Vec<Requirement>,
    environments: SupportedEnvironments,
    extras: ExtrasSpecification,
    groups: GroupsSpecification,
    output_file: Option<&Path>,
    format: Option<ExportFormat>,
    resolution_mode: ResolutionMode,
    prerelease_mode: PrereleaseMode,
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
    dependency_metadata: DependencyMetadata,
    keyring_provider: KeyringProviderType,
    network_settings: &NetworkSettings,
    config_settings: ConfigSettings,
    config_settings_package: PackageConfigSettings,
    no_build_isolation: bool,
    no_build_isolation_package: Vec<PackageName>,
    extra_build_dependencies: &ExtraBuildDependencies,
    build_options: BuildOptions,
    mut python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    universal: bool,
    exclude_newer: ExcludeNewer,
    sources: SourceStrategy,
    annotation_style: AnnotationStyle,
    link_mode: LinkMode,
    mut python: Option<String>,
    system: bool,
    python_preference: PythonPreference,
    concurrency: Concurrency,
    quiet: bool,
    cache: Cache,
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
            ExportFormat::RequirementsTxt
        } else if extension.is_some_and(|ext| ext.eq_ignore_ascii_case("toml")) {
            ExportFormat::PylockToml
        } else {
            ExportFormat::RequirementsTxt
        }
    });

    // If the user is exporting to PEP 751, ensure the filename matches the specification.
    if matches!(format, ExportFormat::PylockToml) {
        if let Some(file_name) = output_file
            .and_then(Path::file_name)
            .and_then(OsStr::to_str)
        {
            if !is_pylock_toml(file_name) {
                return Err(anyhow!(
                    "Expected the output filename to start with `pylock.` and end with `.toml` (e.g., `pylock.toml`, `pylock.dev.toml`); `{file_name}` won't be recognized as a `pylock.toml` file in subsequent commands",
                ));
            }
        }
    }

    // Respect `UV_PYTHON`
    if python.is_none() && python_version.is_none() {
        if let Ok(request) = std::env::var("UV_PYTHON") {
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

    let client_builder = BaseClientBuilder::new()
        .retries_from_env()?
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
        pylock,
        source_trees,
        groups,
        extras: used_extras,
        index_url,
        extra_index_urls,
        no_index,
        find_links,
        no_binary,
        no_build,
    } = RequirementsSpecification::from_sources(
        requirements,
        constraints,
        overrides,
        Some(&groups),
        &client_builder,
    )
    .await?;

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
    let interpreter = if let Some(python) = python.as_ref() {
        let request = PythonRequest::parse(python);
        PythonInstallation::find(
            &request,
            environment_preference,
            python_preference,
            &cache,
            preview,
        )
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
            &cache,
            preview,
        )
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

    // Determine the environment for the resolution.
    let (tags, resolver_env) = if universal {
        (
            None,
            ResolverEnvironment::universal(environments.into_markers()),
        )
    } else {
        let (tags, marker_env) =
            resolution_environment(python_version, python_platform, &interpreter)?;
        (Some(tags), ResolverEnvironment::specific(marker_env))
    };

    // Generate, but don't enforce hashes for the requirements. PEP 751 _requires_ a hash to be
    // present, but otherwise, we omit them by default.
    let hasher = if generate_hashes || matches!(format, ExportFormat::PylockToml) {
        HashStrategy::Generate(HashGeneration::All)
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

    index_locations.cache_index_credentials();

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
            )
        })
        .transpose()?;

    // Initialize the registry client.
    let client = RegistryClientBuilder::try_from(client_builder)?
        .cache(cache.clone())
        .index_locations(&index_locations)
        .index_strategy(index_strategy)
        .torch_backend(torch_backend.clone())
        .markers(interpreter.markers())
        .platform(interpreter.platform())
        .build();

    // Read the lockfile, if present.
    let LockedRequirements { preferences, git } =
        if let Some(output_file) = output_file.filter(|output_file| output_file.exists()) {
            match format {
                ExportFormat::RequirementsTxt => LockedRequirements::from_preferences(
                    read_requirements_txt(output_file, &upgrade).await?,
                ),
                ExportFormat::PylockToml => {
                    read_pylock_toml_requirements(output_file, &upgrade).await?
                }
            }
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
    let build_isolation = if no_build_isolation {
        environment = PythonEnvironment::from_interpreter(interpreter.clone());
        BuildIsolation::Shared(&environment)
    } else if no_build_isolation_package.is_empty() {
        BuildIsolation::Isolated
    } else {
        environment = PythonEnvironment::from_interpreter(interpreter.clone());
        BuildIsolation::SharedPackage(&environment, &no_build_isolation_package)
    };

    // Don't enforce hashes in `pip compile`.
    let build_hashes = HashStrategy::None;
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
        build_isolation,
        &extra_build_requires,
        link_mode,
        &build_options,
        &build_hashes,
        exclude_newer.clone(),
        sources,
        WorkspaceCache::default(),
        concurrency,
        preview,
    );

    let options = OptionsBuilder::new()
        .resolution_mode(resolution_mode)
        .prerelease_mode(prerelease_mode)
        .fork_strategy(fork_strategy)
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
        concurrency,
        options,
        Box::new(DefaultResolveLogger),
        printer,
    )
    .await
    {
        Ok(resolution) => resolution,
        Err(err) => {
            return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
        }
    };

    // Write the resolved dependencies to the output channel.
    let mut writer = OutputWriter::new(!quiet || output_file.is_none(), output_file);

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

    match format {
        ExportFormat::RequirementsTxt => {
            if include_marker_expression {
                if let Some(marker_env) = resolver_env.marker_environment() {
                    let relevant_markers = resolution.marker_tree(&top_level_index, marker_env)?;
                    if let Some(relevant_markers) = relevant_markers.contents() {
                        writeln!(
                            writer,
                            "{}",
                            "# Pinned dependencies known to be valid for:".green()
                        )?;
                        writeln!(writer, "{}", format!("#    {relevant_markers}").green())?;
                    }
                }
            }

            let mut wrote_preamble = false;

            // If necessary, include the `--index-url` and `--extra-index-url` locations.
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

            // If necessary, include the `--find-links` locations.
            if include_find_links {
                for flat_index in index_locations.flat_indexes() {
                    writeln!(writer, "--find-links {}", flat_index.url().verbatim())?;
                    wrote_preamble = true;
                }
            }

            // If necessary, include the `--no-binary` and `--only-binary` options.
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

            // If we wrote an index, add a newline to separate it from the requirements
            if wrote_preamble {
                writeln!(writer)?;
            }

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
        ExportFormat::PylockToml => {
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
            let export = PylockToml::from_resolution(&resolution, &no_emit_packages, install_path)?;
            write!(writer, "{}", export.to_toml()?)?;
        }
    }

    // If any "unsafe" packages were excluded, notify the user.
    let excluded = no_emit_packages
        .into_iter()
        .filter(|name| resolution.contains(name))
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

    // Commit the output to disk.
    writer.commit().await?;

    // Notify the user of any resolution diagnostics.
    operations::diagnose_resolution(resolution.diagnostics(), printer)?;

    Ok(ExitStatus::Success)
}

/// Format the uv command used to generate the output file.
#[allow(clippy::fn_params_excessive_bools)]
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

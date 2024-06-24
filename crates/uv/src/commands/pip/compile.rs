use std::env;
use std::fmt::Write;
use std::io::stdout;
use std::ops::Deref;
use std::path::Path;

use anstream::{eprint, AutoStream, StripStream};
use anyhow::{anyhow, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use distribution_types::{
    IndexLocations, SourceAnnotation, SourceAnnotations, UnresolvedRequirementSpecification,
    Verbatim,
};
use install_wheel_rs::linker::LinkMode;
use pypi_types::Requirement;
use uv_auth::store_credentials_from_url;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity, FlatIndexClient, RegistryClientBuilder};
use uv_configuration::{
    BuildOptions, Concurrency, ConfigSettings, Constraints, ExtrasSpecification, IndexStrategy,
    NoBinary, NoBuild, Overrides, PreviewMode, SetupPyStrategy, Upgrade,
};
use uv_configuration::{KeyringProviderType, TargetTriple};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_fs::Simplified;
use uv_git::GitResolver;
use uv_normalize::PackageName;
use uv_requirements::{
    upgrade::read_requirements_txt, LookaheadResolver, NamedRequirementsResolver,
    RequirementsSource, RequirementsSpecification, SourceTreeResolver,
};
use uv_resolver::{
    AnnotationStyle, DependencyMode, DisplayResolutionGraph, ExcludeNewer, Exclusions, FlatIndex,
    InMemoryIndex, Manifest, OptionsBuilder, PreReleaseMode, PythonRequirement, ResolutionMode,
    Resolver,
};
use uv_toolchain::{
    EnvironmentPreference, PythonEnvironment, PythonVersion, Toolchain, ToolchainPreference,
    ToolchainRequest, VersionRequest,
};
use uv_types::{BuildIsolation, EmptyInstalledPackages, HashStrategy, InFlight};
use uv_warnings::warn_user;

use crate::commands::pip::{operations, resolution_environment};
use crate::commands::reporters::ResolverReporter;
use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;

/// Resolve a set of requirements into a set of pinned versions.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub(crate) async fn pip_compile(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    overrides_from_workspace: Vec<Requirement>,
    extras: ExtrasSpecification,
    output_file: Option<&Path>,
    resolution_mode: ResolutionMode,
    prerelease_mode: PreReleaseMode,
    dependency_mode: DependencyMode,
    upgrade: Upgrade,
    generate_hashes: bool,
    no_emit_packages: Vec<PackageName>,
    include_extras: bool,
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
    keyring_provider: KeyringProviderType,
    setup_py: SetupPyStrategy,
    config_settings: ConfigSettings,
    connectivity: Connectivity,
    no_build_isolation: bool,
    build_options: BuildOptions,
    python_version: Option<PythonVersion>,
    python_platform: Option<TargetTriple>,
    exclude_newer: Option<ExcludeNewer>,
    annotation_style: AnnotationStyle,
    link_mode: LinkMode,
    python: Option<String>,
    system: bool,
    toolchain_preference: ToolchainPreference,
    concurrency: Concurrency,
    native_tls: bool,
    quiet: bool,
    preview: PreviewMode,
    cache: Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    // If the user requests `extras` but does not provide a valid source (e.g., a `pyproject.toml`),
    // return an error.
    if !extras.is_empty() && !requirements.iter().any(RequirementsSource::allows_extras) {
        return Err(anyhow!(
            "Requesting extras requires a `pyproject.toml`, `setup.cfg`, or `setup.py` file."
        ));
    }

    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls)
        .keyring(keyring_provider);

    // Read all requirements from the provided sources.
    let RequirementsSpecification {
        mut project,
        requirements,
        constraints,
        overrides,
        source_trees,
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
        &client_builder,
    )
    .await?;

    // If all the metadata could be statically resolved, validate that every extra was used. If we
    // need to resolve metadata via PEP 517, we don't know which extras are used until much later.
    if source_trees.is_empty() {
        if let ExtrasSpecification::Some(extras) = &extras {
            let mut unused_extras = extras
                .iter()
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
    }

    // Find an interpreter to use for building distributions
    let environments = EnvironmentPreference::from_system_flag(system, false);
    let interpreter = if let Some(python) = python.as_ref() {
        let request = ToolchainRequest::parse(python);
        Toolchain::find(&request, environments, toolchain_preference, &cache)
    } else {
        // TODO(zanieb): The split here hints at a problem with the abstraction; we should be able to use
        // `Toolchain::find(...)` here.
        let request = if let Some(version) = python_version.as_ref() {
            // TODO(zanieb): We should consolidate `VersionRequest` and `PythonVersion`
            ToolchainRequest::Version(VersionRequest::from(version))
        } else {
            ToolchainRequest::default()
        };
        Toolchain::find_best(&request, environments, toolchain_preference, &cache)
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

    // Create a shared in-memory index.
    let source_index = InMemoryIndex::default();

    // If we're resolving against a different Python version, use a separate index. Source
    // distributions will be built against the installed version, and so the index may contain
    // different package priorities than in the top-level resolution.
    let top_level_index = if python_version.is_some() {
        InMemoryIndexRef::Owned(InMemoryIndex::default())
    } else {
        InMemoryIndexRef::Borrowed(&source_index)
    };

    // Determine the Python requirement, based on the interpreter and the requested version.
    let python_requirement = if let Some(python_version) = python_version.as_ref() {
        PythonRequirement::from_python_version(&interpreter, python_version)
    } else {
        PythonRequirement::from_interpreter(&interpreter)
    };

    // Determine the environment for the resolution.
    let (tags, markers) = resolution_environment(python_version, python_platform, &interpreter)?;

    // Generate, but don't enforce hashes for the requirements.
    let hasher = if generate_hashes {
        HashStrategy::Generate
    } else {
        HashStrategy::None
    };

    // Incorporate any index locations from the provided sources.
    let index_locations =
        index_locations.combine(index_url, extra_index_urls, find_links, no_index);

    // Add all authenticated sources to the cache.
    for url in index_locations.urls() {
        store_credentials_from_url(url);
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .index_strategy(index_strategy)
        .keyring(keyring_provider)
        .markers(&markers)
        .platform(interpreter.platform())
        .build();

    // Read the lockfile, if present.
    let preferences = read_requirements_txt(output_file, &upgrade).await?;
    let git = GitResolver::default();

    // Combine the `--no-binary` and `--no-build` flags from the requirements files.
    let build_options = build_options.combine(no_binary, no_build);

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, &cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, Some(&tags), &hasher, &build_options)
    };

    // Track in-flight downloads, builds, etc., across resolutions.
    let in_flight = InFlight::default();

    // Determine whether to enable build isolation.
    let environment;
    let build_isolation = if no_build_isolation {
        environment = PythonEnvironment::from_interpreter(interpreter.clone());
        BuildIsolation::Shared(&environment)
    } else {
        BuildIsolation::Isolated
    };

    let build_dispatch = BuildDispatch::new(
        &client,
        &cache,
        &interpreter,
        &index_locations,
        &flat_index,
        &source_index,
        &git,
        &in_flight,
        index_strategy,
        setup_py,
        &config_settings,
        build_isolation,
        link_mode,
        &build_options,
        exclude_newer,
        concurrency,
        preview,
    );

    // Resolve the requirements from the provided sources.
    let requirements = {
        // Convert from unnamed to named requirements.
        let mut requirements = NamedRequirementsResolver::new(
            requirements,
            &hasher,
            &top_level_index,
            DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads, preview),
        )
        .with_reporter(ResolverReporter::from(printer))
        .resolve()
        .await?;

        // Resolve any source trees into requirements.
        if !source_trees.is_empty() {
            let resolutions = SourceTreeResolver::new(
                source_trees,
                &extras,
                &hasher,
                &top_level_index,
                DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads, preview),
            )
            .with_reporter(ResolverReporter::from(printer))
            .resolve()
            .await?;

            // If we resolved a single project, use it for the project name.
            project = project.or_else(|| {
                if let [resolution] = &resolutions[..] {
                    Some(resolution.project.clone())
                } else {
                    None
                }
            });

            // If any of the extras were unused, surface a warning.
            if let ExtrasSpecification::Some(extras) = extras {
                let mut unused_extras = extras
                    .iter()
                    .filter(|extra| {
                        !resolutions
                            .iter()
                            .any(|resolution| resolution.extras.contains(extra))
                    })
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

            // Extend the requirements with the resolved source trees.
            requirements.extend(
                resolutions
                    .into_iter()
                    .flat_map(|resolution| resolution.requirements),
            );
        }

        requirements
    };

    // Merge workspace overrides.
    let overrides: Vec<UnresolvedRequirementSpecification> = overrides
        .iter()
        .cloned()
        .chain(
            overrides_from_workspace
                .into_iter()
                .map(UnresolvedRequirementSpecification::from),
        )
        .collect();

    // Resolve the overrides from the provided sources.
    let overrides = NamedRequirementsResolver::new(
        overrides,
        &hasher,
        &top_level_index,
        DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads, preview),
    )
    .with_reporter(ResolverReporter::from(printer))
    .resolve()
    .await?;

    // Generate a map from requirement to originating source file.
    let mut sources = SourceAnnotations::default();

    for requirement in requirements
        .iter()
        .filter(|requirement| requirement.evaluate_markers(Some(&markers), &[]))
    {
        if let Some(origin) = &requirement.origin {
            sources.add(
                &requirement.name,
                SourceAnnotation::Requirement(origin.clone()),
            );
        }
    }

    for requirement in constraints
        .iter()
        .filter(|requirement| requirement.evaluate_markers(Some(&markers), &[]))
    {
        if let Some(origin) = &requirement.origin {
            sources.add(
                &requirement.name,
                SourceAnnotation::Constraint(origin.clone()),
            );
        }
    }

    for requirement in overrides
        .iter()
        .filter(|requirement| requirement.evaluate_markers(Some(&markers), &[]))
    {
        if let Some(origin) = &requirement.origin {
            sources.add(
                &requirement.name,
                SourceAnnotation::Override(origin.clone()),
            );
        }
    }

    // Collect constraints and overrides.
    let constraints = Constraints::from_requirements(constraints);
    let overrides = Overrides::from_requirements(overrides);

    // Ignore development dependencies.
    let dev = Vec::default();

    // Determine any lookahead requirements.
    let lookaheads = match dependency_mode {
        DependencyMode::Transitive => {
            LookaheadResolver::new(
                &requirements,
                &constraints,
                &overrides,
                &dev,
                &hasher,
                &top_level_index,
                DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads, preview),
            )
            .with_reporter(ResolverReporter::from(printer))
            .resolve(Some(&markers))
            .await?
        }
        DependencyMode::Direct => Vec::new(),
    };

    // Create a manifest of the requirements.
    let manifest = Manifest::new(
        requirements,
        constraints,
        overrides,
        dev,
        preferences,
        project,
        // Do not consider any installed packages during resolution.
        Exclusions::All,
        lookaheads,
    );

    let options = OptionsBuilder::new()
        .resolution_mode(resolution_mode)
        .prerelease_mode(prerelease_mode)
        .dependency_mode(dependency_mode)
        .exclude_newer(exclude_newer)
        .index_strategy(index_strategy)
        .build();

    // Resolve the dependencies.
    let resolver = Resolver::new(
        manifest.clone(),
        options,
        &python_requirement,
        Some(&markers),
        Some(&tags),
        &flat_index,
        &top_level_index,
        &hasher,
        &build_dispatch,
        EmptyInstalledPackages,
        DistributionDatabase::new(&client, &build_dispatch, concurrency.downloads, preview),
    )?
    .with_reporter(ResolverReporter::from(printer));

    let resolution = match resolver.resolve().await {
        Err(uv_resolver::ResolveError::NoSolution(err)) => {
            let report = miette::Report::msg(format!("{err}"))
                .context("No solution found when resolving dependencies:");
            eprint!("{report:?}");
            return Ok(ExitStatus::Failure);
        }
        result => result,
    }?;

    let s = if resolution.len() == 1 { "" } else { "s" };
    writeln!(
        printer.stderr(),
        "{}",
        format!(
            "Resolved {} in {}",
            format!("{} package{}", resolution.len(), s).bold(),
            elapsed(start.elapsed())
        )
        .dimmed()
    )?;

    // Write the resolved dependencies to the output channel.
    let mut writer = OutputWriter::new(!quiet || output_file.is_none(), output_file)?;

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

    if include_marker_expression {
        let relevant_markers = resolution.marker_tree(&manifest, &top_level_index, &markers)?;
        writeln!(
            writer,
            "{}",
            "# Pinned dependencies known to be valid for:".green()
        )?;
        writeln!(writer, "{}", format!("#    {relevant_markers}").green())?;
    }

    let mut wrote_preamble = false;

    // If necessary, include the `--index-url` and `--extra-index-url` locations.
    if include_index_url {
        if let Some(index) = index_locations.index() {
            writeln!(writer, "--index-url {}", index.verbatim())?;
            wrote_preamble = true;
        }
        for extra_index in index_locations.extra_index() {
            writeln!(writer, "--extra-index-url {}", extra_index.verbatim())?;
            wrote_preamble = true;
        }
    }

    // If necessary, include the `--find-links` locations.
    if include_find_links {
        for flat_index in index_locations.flat_index() {
            writeln!(writer, "--find-links {flat_index}")?;
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
            &no_emit_packages,
            generate_hashes,
            include_extras,
            include_annotations,
            include_index_annotation,
            annotation_style,
            sources,
        )
    )?;

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

    // Notify the user of any resolution diagnostics.
    operations::diagnose_resolution(resolution.diagnostics(), printer)?;

    Ok(ExitStatus::Success)
}

/// Format the `uv` command used to generate the output file.
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
                if arg.starts_with("--extra-index-url=") || arg.starts_with("--index-url=") {
                    // Reset state; skip this iteration.
                    *skip_next = None;
                    return Some(None);
                }

                // Mark the next item as (to be) skipped.
                if arg == "--index-url" || arg == "--extra-index-url" {
                    *skip_next = Some(true);
                    return Some(None);
                }
            }

            // Skip any `--find-links` URLs, unless requested.
            if !include_find_links {
                if arg.starts_with("--find-links=") || arg.starts_with("-f=") {
                    // Reset state; skip this iteration.
                    *skip_next = None;
                    return Some(None);
                }

                // Mark the next item as (to be) skipped.
                if arg == "--find-links" || arg == "-f" {
                    *skip_next = Some(true);
                    return Some(None);
                }
            }

            // Always skip the `--upgrade` flag.
            if arg == "--upgrade" || arg == "-U" {
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

            // Return the argument.
            Some(Some(arg))
        })
        .flatten()
        .join(" ");
    format!("uv {args}")
}

/// A multi-casting writer that writes to both the standard output and an output file, if present.
#[allow(clippy::disallowed_types)]
struct OutputWriter {
    stdout: Option<AutoStream<std::io::Stdout>>,
    output_file: Option<StripStream<std::fs::File>>,
}

#[allow(clippy::disallowed_types)]
impl OutputWriter {
    /// Create a new output writer.
    fn new(include_stdout: bool, output_file: Option<&Path>) -> Result<Self> {
        let stdout = include_stdout.then(|| AutoStream::<std::io::Stdout>::auto(stdout()));
        let output_file = output_file
            .map(|output_file| -> Result<_, std::io::Error> {
                let output_file = fs_err::File::create(output_file)?;
                Ok(StripStream::new(output_file.into()))
            })
            .transpose()?;
        Ok(Self {
            stdout,
            output_file,
        })
    }

    /// Write the given arguments to both the standard output and the output file, if present.
    fn write_fmt(&mut self, args: std::fmt::Arguments<'_>) -> std::io::Result<()> {
        use std::io::Write;

        if let Some(output_file) = &mut self.output_file {
            write!(output_file, "{args}")?;
        }

        if let Some(stdout) = &mut self.stdout {
            write!(stdout, "{args}")?;
        }

        Ok(())
    }
}

/// An owned or unowned [`InMemoryIndex`].
enum InMemoryIndexRef<'a> {
    Owned(InMemoryIndex),
    Borrowed(&'a InMemoryIndex),
}

impl Deref for InMemoryIndexRef<'_> {
    type Target = InMemoryIndex;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(index) => index,
            Self::Borrowed(index) => index,
        }
    }
}

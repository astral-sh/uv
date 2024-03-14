use std::borrow::Cow;
use std::env;
use std::fmt::Write;
use std::io::stdout;
use std::ops::Deref;
use std::path::Path;
use std::str::FromStr;

use anstream::{eprint, AutoStream, StripStream};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::FxHashSet;
use tempfile::tempdir_in;
use tracing::debug;

use distribution_types::{IndexLocations, LocalEditable, Verbatim};
use platform_tags::Tags;
use requirements_txt::EditableRequirement;
use uv_auth::{KeyringProvider, GLOBAL_AUTH_STORE};
use uv_cache::Cache;
use uv_client::{Connectivity, FlatIndex, FlatIndexClient, RegistryClientBuilder};
use uv_dispatch::BuildDispatch;
use uv_fs::Simplified;
use uv_installer::{Downloader, NoBinary};
use uv_interpreter::{find_best_python, PythonEnvironment, PythonVersion};
use uv_normalize::{ExtraName, PackageName};
use uv_resolver::{
    AnnotationStyle, DependencyMode, DisplayResolutionGraph, InMemoryIndex, Manifest,
    OptionsBuilder, PreReleaseMode, PythonRequirement, ResolutionMode, Resolver,
};
use uv_traits::{BuildIsolation, ConfigSettings, InFlight, NoBuild, SetupPyStrategy};
use uv_warnings::warn_user;

use crate::commands::reporters::{DownloadReporter, ResolverReporter};
use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;
use crate::requirements::{
    read_lockfile, ExtrasSpecification, RequirementsSource, RequirementsSpecification,
};

/// Resolve a set of requirements into a set of pinned versions.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub(crate) async fn pip_compile(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    extras: ExtrasSpecification<'_>,
    output_file: Option<&Path>,
    resolution_mode: ResolutionMode,
    prerelease_mode: PreReleaseMode,
    dependency_mode: DependencyMode,
    upgrade: Upgrade,
    generate_hashes: bool,
    no_emit_packages: Vec<PackageName>,
    include_annotations: bool,
    include_header: bool,
    include_index_url: bool,
    include_find_links: bool,
    index_locations: IndexLocations,
    keyring_provider: KeyringProvider,
    setup_py: SetupPyStrategy,
    config_settings: ConfigSettings,
    connectivity: Connectivity,
    no_build_isolation: bool,
    no_build: &NoBuild,
    python_version: Option<PythonVersion>,
    exclude_newer: Option<DateTime<Utc>>,
    annotation_style: AnnotationStyle,
    native_tls: bool,
    quiet: bool,
    cache: Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    // If the user requests `extras` but does not provide a pyproject toml source
    if !matches!(extras, ExtrasSpecification::None)
        && !requirements
            .iter()
            .any(|source| matches!(source, RequirementsSource::PyprojectToml(_)))
    {
        return Err(anyhow!(
            "Requesting extras requires a pyproject.toml input file."
        ));
    }

    // Read all requirements from the provided sources.
    let RequirementsSpecification {
        project,
        requirements,
        constraints,
        overrides,
        editables,
        index_url,
        extra_index_urls,
        no_index,
        find_links,
        extras: used_extras,
    } = RequirementsSpecification::from_sources(
        requirements,
        constraints,
        overrides,
        &extras,
        connectivity,
    )
    .await?;

    // Check that all provided extras are used.
    if let ExtrasSpecification::Some(extras) = extras {
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

    // Read the lockfile, if present.
    let preferences = read_lockfile(output_file, upgrade).await?;

    // Find an interpreter to use for building distributions
    let interpreter = find_best_python(python_version.as_ref(), &cache)?;
    debug!(
        "Using Python {} interpreter at {} for builds",
        interpreter.python_version(),
        interpreter.sys_executable().simplified_display().cyan()
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

    // Determine the tags, markers, and interpreter to use for resolution.
    let tags = if let Some(python_version) = python_version.as_ref() {
        Cow::Owned(Tags::from_env(
            interpreter.platform(),
            (python_version.major(), python_version.minor()),
            interpreter.implementation_name(),
            interpreter.implementation_tuple(),
        )?)
    } else {
        Cow::Borrowed(interpreter.tags()?)
    };
    let markers = python_version.map_or_else(
        || Cow::Borrowed(interpreter.markers()),
        |python_version| Cow::Owned(python_version.markers(interpreter.markers())),
    );

    // Incorporate any index locations from the provided sources.
    let index_locations =
        index_locations.combine(index_url, extra_index_urls, find_links, no_index);

    // Add all authenticated sources to the store.
    for url in index_locations.urls() {
        GLOBAL_AUTH_STORE.save_from_url(url);
    }

    // Initialize the registry client.
    let client = RegistryClientBuilder::new(cache.clone())
        .native_tls(native_tls)
        .connectivity(connectivity)
        .index_urls(index_locations.index_urls())
        .keyring_provider(keyring_provider)
        .build();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, &cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, &tags)
    };

    // Track in-flight downloads, builds, etc., across resolutions.
    let in_flight = InFlight::default();

    // Determine whether to enable build isolation.
    let venv;
    let build_isolation = if no_build_isolation {
        venv = PythonEnvironment::from_interpreter(interpreter.clone());
        BuildIsolation::Shared(&venv)
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
        &in_flight,
        setup_py,
        &config_settings,
        build_isolation,
        no_build,
        &NoBinary::None,
    )
    .with_options(OptionsBuilder::new().exclude_newer(exclude_newer).build());

    // Build the editables and add their requirements
    let editable_metadata = if editables.is_empty() {
        Vec::new()
    } else {
        let start = std::time::Instant::now();

        let editables: Vec<LocalEditable> = editables
            .into_iter()
            .map(|editable| {
                let EditableRequirement { url, extras, path } = editable;
                Ok(LocalEditable { url, path, extras })
            })
            .collect::<Result<_>>()?;

        let downloader = Downloader::new(&cache, &tags, &client, &build_dispatch)
            .with_reporter(DownloadReporter::from(printer).with_length(editables.len() as u64));

        // Build all editables.
        let editable_wheel_dir = tempdir_in(cache.root())?;
        let editables: Vec<_> = downloader
            .build_editables(editables, editable_wheel_dir.path())
            .await
            .context("Failed to build editables")?
            .into_iter()
            .map(|built_editable| (built_editable.editable, built_editable.metadata))
            .collect();

        // Validate that the editables are compatible with the target Python version.
        let requirement = PythonRequirement::new(&interpreter, &markers);
        for (.., metadata) in &editables {
            if let Some(python_requires) = metadata.requires_python.as_ref() {
                if !python_requires.contains(requirement.target()) {
                    return Err(anyhow!(
                        "Editable `{}` requires Python {}, but resolution targets Python {}",
                        metadata.name,
                        python_requires,
                        requirement.target()
                    ));
                }
            }
        }

        let s = if editables.len() == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "{}",
            format!(
                "Built {} in {}",
                format!("{} editable{}", editables.len(), s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;
        editables
    };

    // Create a manifest of the requirements.
    let manifest = Manifest::new(
        requirements,
        constraints,
        overrides,
        preferences,
        project,
        editable_metadata,
    );

    let options = OptionsBuilder::new()
        .resolution_mode(resolution_mode)
        .prerelease_mode(prerelease_mode)
        .dependency_mode(dependency_mode)
        .exclude_newer(exclude_newer)
        .build();

    // Resolve the dependencies.
    let resolver = Resolver::new(
        manifest,
        options,
        &markers,
        &interpreter,
        &tags,
        &client,
        &flat_index,
        &top_level_index,
        &build_dispatch,
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

    // Notify the user of any diagnostics.
    for diagnostic in resolution.diagnostics() {
        writeln!(
            printer.stderr(),
            "{}{} {}",
            "warning".yellow().bold(),
            ":".bold(),
            diagnostic.message().bold()
        )?;
    }

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
            format!("#    {}", cmd(include_index_url, include_find_links,)).green()
        )?;
    }

    // Write the index locations to the output channel.
    let mut wrote_index = false;

    // If necessary, include the `--index-url` and `--extra-index-url` locations.
    if include_index_url {
        if let Some(index) = index_locations.index() {
            writeln!(writer, "--index-url {}", index.verbatim())?;
            wrote_index = true;
        }
        for extra_index in index_locations.extra_index() {
            writeln!(writer, "--extra-index-url {}", extra_index.verbatim())?;
            wrote_index = true;
        }
    }

    // If necessary, include the `--find-links` locations.
    if include_find_links {
        for flat_index in index_locations.flat_index() {
            writeln!(writer, "--find-links {flat_index}")?;
            wrote_index = true;
        }
    }

    // If we wrote an index, add a newline to separate it from the requirements
    if wrote_index {
        writeln!(writer)?;
    }

    write!(
        writer,
        "{}",
        DisplayResolutionGraph::new(
            &resolution,
            &no_emit_packages,
            generate_hashes,
            include_annotations,
            annotation_style,
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

    Ok(ExitStatus::Success)
}

/// Format the `uv` command used to generate the output file.
#[allow(clippy::fn_params_excessive_bools)]
fn cmd(include_index_url: bool, include_find_links: bool) -> String {
    let args = env::args_os()
        .skip(1)
        .map(|arg| arg.simplified_display().to_string())
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

/// Whether to allow package upgrades.
#[derive(Debug)]
pub(crate) enum Upgrade {
    /// Prefer pinned versions from the existing lockfile, if possible.
    None,

    /// Allow package upgrades for all packages, ignoring the existing lockfile.
    All,

    /// Allow package upgrades, but only for the specified packages.
    Packages(FxHashSet<PackageName>),
}

impl Upgrade {
    /// Determine the upgrade strategy from the command-line arguments.
    pub(crate) fn from_args(upgrade: bool, upgrade_package: Vec<PackageName>) -> Self {
        if upgrade {
            Self::All
        } else if !upgrade_package.is_empty() {
            Self::Packages(upgrade_package.into_iter().collect())
        } else {
            Self::None
        }
    }

    /// Returns `true` if no packages should be upgraded.
    pub(crate) fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Returns `true` if all packages should be upgraded.
    pub(crate) fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }
}

pub(crate) fn extra_name_with_clap_error(arg: &str) -> Result<ExtraName> {
    ExtraName::from_str(arg).map_err(|_err| {
        anyhow!(
            "Extra names must start and end with a letter or digit and may only \
            contain -, _, ., and alphanumeric characters"
        )
    })
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

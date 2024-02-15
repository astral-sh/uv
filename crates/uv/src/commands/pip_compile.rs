use std::borrow::Cow;
use std::env;
use std::fmt::Write;
use std::io::stdout;
use std::ops::Deref;
use std::path::Path;
use std::str::FromStr;

use anstream::{eprint, AutoStream};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::FxHashSet;
use tempfile::tempdir_in;
use tracing::debug;

use distribution_types::{IndexLocations, LocalEditable};
use pep508_rs::Requirement;
use platform_host::Platform;
use platform_tags::Tags;
use requirements_txt::EditableRequirement;
use uv_cache::Cache;
use uv_client::{Connectivity, FlatIndex, FlatIndexClient, RegistryClientBuilder};
use uv_dispatch::BuildDispatch;
use uv_fs::Normalized;
use uv_installer::{Downloader, NoBinary};
use uv_interpreter::{Interpreter, PythonVersion};
use uv_normalize::{ExtraName, PackageName};
use uv_resolver::{
    DependencyMode, DisplayResolutionGraph, InMemoryIndex, Manifest, OptionsBuilder,
    PreReleaseMode, ResolutionMode, Resolver,
};
use uv_traits::{InFlight, NoBuild, SetupPyStrategy};
use uv_warnings::warn_user;

use crate::commands::reporters::{DownloadReporter, ResolverReporter};
use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;
use crate::requirements::{ExtrasSpecification, RequirementsSource, RequirementsSpecification};

const VERSION: &str = env!("CARGO_PKG_VERSION");

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
    include_annotations: bool,
    include_header: bool,
    include_index_url: bool,
    include_find_links: bool,
    index_locations: IndexLocations,
    setup_py: SetupPyStrategy,
    connectivity: Connectivity,
    no_build: &NoBuild,
    python_version: Option<PythonVersion>,
    exclude_newer: Option<DateTime<Utc>>,
    cache: Cache,
    mut printer: Printer,
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
    } = RequirementsSpecification::from_sources(requirements, constraints, overrides, &extras)?;

    // Incorporate any index locations from the provided sources.
    let index_locations =
        index_locations.combine(index_url, extra_index_urls, find_links, no_index);

    // Check that all provided extras are used
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

    let preferences: Vec<Requirement> = output_file
        // As an optimization, skip reading the lockfile is we're upgrading all packages anyway.
        .filter(|_| !upgrade.is_all())
        .filter(|output_file| output_file.exists())
        .map(Path::to_path_buf)
        .map(RequirementsSource::from_path)
        .as_ref()
        .map(|source| RequirementsSpecification::from_source(source, &extras))
        .transpose()?
        .map(|spec| spec.requirements)
        .map(|requirements| match upgrade {
            // Respect all pinned versions from the existing lockfile.
            Upgrade::None => requirements,
            // Ignore all pinned versions from the existing lockfile.
            Upgrade::All => vec![],
            // Ignore pinned versions for the specified packages.
            Upgrade::Packages(packages) => requirements
                .into_iter()
                .filter(|requirement| !packages.contains(&requirement.name))
                .collect(),
        })
        .unwrap_or_default();

    // Find an interpreter to use for building distributions
    let platform = Platform::current()?;
    let interpreter = Interpreter::find_best(python_version.as_ref(), &platform, &cache)?;
    debug!(
        "Using Python {} interpreter at {} for builds",
        interpreter.python_version(),
        interpreter.sys_executable().normalized_display().cyan()
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

    // Instantiate a client.
    let client = RegistryClientBuilder::new(cache.clone())
        .index_urls(index_locations.index_urls())
        .connectivity(connectivity)
        .build();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, &cache);
        let entries = client.fetch(index_locations.flat_index()).await?;
        FlatIndex::from_entries(entries, &tags)
    };

    // Track in-flight downloads, builds, etc., across resolutions.
    let in_flight = InFlight::default();

    let options = OptionsBuilder::new()
        .resolution_mode(resolution_mode)
        .prerelease_mode(prerelease_mode)
        .dependency_mode(dependency_mode)
        .exclude_newer(exclude_newer)
        .build();

    let build_dispatch = BuildDispatch::new(
        &client,
        &cache,
        &interpreter,
        &index_locations,
        &flat_index,
        &source_index,
        &in_flight,
        interpreter.sys_executable().to_path_buf(),
        setup_py,
        no_build,
        &NoBinary::None,
    )
    .with_options(options);

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

        let editable_wheel_dir = tempdir_in(cache.root())?;
        let editable_metadata: Vec<_> = downloader
            .build_editables(editables, editable_wheel_dir.path())
            .await
            .context("Failed to build editables")?
            .into_iter()
            .map(|built_editable| (built_editable.editable, built_editable.metadata))
            .collect();

        let s = if editable_metadata.len() == 1 {
            ""
        } else {
            "s"
        };
        writeln!(
            printer,
            "{}",
            format!(
                "Built {} in {}",
                format!("{} editable{}", editable_metadata.len(), s).bold(),
                elapsed(start.elapsed())
            )
            .dimmed()
        )?;
        editable_metadata
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
    )
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
        printer,
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
            printer,
            "{}{} {}",
            "warning".yellow().bold(),
            ":".bold(),
            diagnostic.message().bold()
        )?;
    }

    // Write the resolved dependencies to the output channel.
    let mut writer: Box<dyn std::io::Write> = if let Some(output_file) = output_file {
        Box::new(AutoStream::<std::fs::File>::auto(
            fs_err::File::create(output_file)?.into(),
        ))
    } else {
        Box::new(AutoStream::auto(stdout()))
    };

    if include_header {
        writeln!(
            writer,
            "{}",
            format!("# This file was autogenerated by uv v{VERSION} via the following command:")
                .green()
        )?;
        writeln!(
            writer,
            "{}",
            format!(
                "#    uv {}",
                env::args_os()
                    .skip(1)
                    .map(|arg| arg.normalized_display().to_string())
                    .join(" ")
            )
            .green()
        )?;
    }

    // Write the index locations to the output channel.
    let mut wrote_index = false;

    // If necessary, include the `--index-url` and `--extra-index-url` locations.
    if include_index_url {
        if let Some(index) = index_locations.index() {
            writeln!(writer, "--index-url {index}")?;
            wrote_index = true;
        }
        for extra_index in index_locations.extra_index() {
            writeln!(writer, "--extra-index-url {extra_index}")?;
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
        DisplayResolutionGraph::new(&resolution, generate_hashes, include_annotations)
    )?;

    Ok(ExitStatus::Success)
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

use std::borrow::Cow;
use std::env;
use std::fmt::Write;
use std::io::stdout;
use std::ops::Deref;
use std::path::Path;
use std::str::FromStr;

use anstream::AutoStream;
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
use puffin_cache::Cache;
use puffin_client::{FlatIndex, FlatIndexClient, RegistryClientBuilder};
use puffin_dispatch::BuildDispatch;
use puffin_installer::{Downloader, NoBinary};
use puffin_interpreter::{Interpreter, PythonVersion};
use puffin_normalize::{ExtraName, PackageName};
use puffin_resolver::{
    DisplayResolutionGraph, InMemoryIndex, Manifest, PreReleaseMode, ResolutionMode,
    ResolutionOptions, Resolver,
};
use puffin_traits::{InFlight, SetupPyStrategy};
use requirements_txt::EditableRequirement;

use crate::commands::reporters::{DownloadReporter, ResolverReporter};
use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;
use crate::requirements::{ExtrasSpecification, RequirementsSource, RequirementsSpecification};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Resolve a set of requirements into a set of pinned versions.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn pip_compile(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    overrides: &[RequirementsSource],
    extras: ExtrasSpecification<'_>,
    output_file: Option<&Path>,
    resolution_mode: ResolutionMode,
    prerelease_mode: PreReleaseMode,
    upgrade: Upgrade,
    generate_hashes: bool,
    index_locations: IndexLocations,
    setup_py: SetupPyStrategy,
    no_build: bool,
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
        extras: used_extras,
    } = RequirementsSpecification::from_sources(requirements, constraints, overrides, &extras)?;

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
        .map(RequirementsSource::from)
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

    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let interpreter = Interpreter::find(python_version.as_ref(), platform, &cache)?;

    debug!(
        "Using Python {} at {}",
        interpreter.markers().python_version,
        interpreter.sys_executable().display()
    );

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
            python_version.simple_version(),
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
        .build();

    // Resolve the flat indexes from `--find-links`.
    let flat_index = {
        let client = FlatIndexClient::new(&client, &cache);
        let entries = client.fetch(index_locations.flat_indexes()).await?;
        FlatIndex::from_entries(entries, &tags)
    };

    // Track in-flight downloads, builds, etc., across resolutions.
    let in_flight = InFlight::default();

    let options = ResolutionOptions::new(resolution_mode, prerelease_mode, exclude_newer);
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
                let EditableRequirement { path, url } = editable;
                Ok(LocalEditable { url, path })
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
        Err(puffin_resolver::ResolveError::NoSolution(err)) => {
            #[allow(clippy::print_stderr)]
            {
                let report = miette::Report::msg(format!("{err}"))
                    .context("No solution found when resolving dependencies:");
                eprint!("{report:?}");
            }
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

    writeln!(
        writer,
        "{}",
        format!("# This file was autogenerated by Puffin v{VERSION} via the following command:")
            .green()
    )?;
    writeln!(
        writer,
        "{}",
        format!("#    puffin {}", env::args().skip(1).join(" ")).green()
    )?;
    write!(
        writer,
        "{}",
        DisplayResolutionGraph::new(&resolution, generate_hashes)
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

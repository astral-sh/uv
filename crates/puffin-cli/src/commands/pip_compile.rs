use std::borrow::Cow;
use std::env;
use std::fmt::Write;
use std::io::stdout;
use std::path::Path;
use std::str::FromStr;

use anstream::AutoStream;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use colored::Colorize;
use fs_err as fs;
use itertools::Itertools;
use tracing::debug;

use pep508_rs::Requirement;
use platform_host::Platform;
use platform_tags::Tags;
use puffin_cache::Cache;
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_interpreter::Virtualenv;
use puffin_normalize::ExtraName;
use puffin_resolver::{Manifest, PreReleaseMode, ResolutionMode, ResolutionOptions, Resolver};
use pypi_types::IndexUrls;

use crate::commands::reporters::ResolverReporter;
use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;
use crate::python_version::PythonVersion;
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
    upgrade_mode: UpgradeMode,
    index_urls: IndexUrls,
    no_build: bool,
    python_version: Option<PythonVersion>,
    exclude_newer: Option<DateTime<Utc>>,
    cache: Cache,
    mut printer: Printer,
) -> Result<ExitStatus> {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .break_words(false)
                .word_separator(textwrap::WordSeparator::AsciiSpace)
                .word_splitter(textwrap::WordSplitter::NoHyphenation)
                .build(),
        )
    }))?;

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
        .filter(|_| upgrade_mode.is_prefer_pinned())
        .filter(|output_file| output_file.exists())
        .map(Path::to_path_buf)
        .map(RequirementsSource::from)
        .as_ref()
        .map(|source| RequirementsSpecification::from_source(source, &extras))
        .transpose()?
        .map(|spec| spec.requirements)
        .unwrap_or_default();

    // Create a manifest of the requirements.
    let manifest = Manifest::new(requirements, constraints, overrides, preferences, project);
    let options = ResolutionOptions::new(resolution_mode, prerelease_mode, exclude_newer);

    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, &cache)?;

    debug!(
        "Using Python {} at {}",
        venv.interpreter().markers().python_version,
        venv.python_executable().display()
    );

    // Determine the compatible platform tags.
    let tags = Tags::from_interpreter(venv.interpreter())?;

    // Determine the interpreter to use for resolution.
    let interpreter = venv.interpreter().clone();

    // Determine the markers to use for resolution.
    let markers = python_version.map_or_else(
        || Cow::Borrowed(venv.interpreter().markers()),
        |python_version| Cow::Owned(python_version.markers(venv.interpreter().markers())),
    );

    // Instantiate a client.
    let client = RegistryClientBuilder::new(cache.clone())
        .index_urls(index_urls.clone())
        .build();

    let build_dispatch = BuildDispatch::new(
        client.clone(),
        cache.clone(),
        interpreter,
        fs::canonicalize(venv.python_executable())?,
        no_build,
        index_urls,
    )
    .with_options(options);

    // Resolve the dependencies.
    let resolver = Resolver::new(manifest, options, &markers, &tags, &client, &build_dispatch)
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
            fs::File::create(output_file)?.into(),
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
    write!(writer, "{resolution}")?;

    Ok(ExitStatus::Success)
}

/// Whether to allow package upgrades.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpgradeMode {
    /// Allow package upgrades, ignoring the existing lockfile.
    AllowUpgrades,
    /// Prefer pinned versions from the existing lockfile, if possible.
    PreferPinned,
}

impl UpgradeMode {
    fn is_prefer_pinned(self) -> bool {
        self == Self::PreferPinned
    }
}

impl From<bool> for UpgradeMode {
    fn from(value: bool) -> Self {
        if value {
            Self::AllowUpgrades
        } else {
            Self::PreferPinned
        }
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

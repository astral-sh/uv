use std::collections::HashSet;
use std::fmt::Write;
use std::io::{stdout, BufWriter};
use std::path::Path;
use std::{env, fs};

use anyhow::{anyhow, Result};
use colored::Colorize;
use fs_err::File;
use itertools::Itertools;
use pubgrub::report::Reporter;
use puffin_package::extra_name::ExtraName;
use tracing::debug;

use pep508_rs::Requirement;
use platform_host::Platform;
use platform_tags::Tags;
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::BuildDispatch;
use puffin_interpreter::Virtualenv;
use puffin_resolver::{Manifest, PreReleaseMode, ResolutionMode};

use crate::commands::reporters::ResolverReporter;
use crate::commands::{elapsed, ExitStatus};
use crate::index_urls::IndexUrls;
use crate::printer::Printer;
use crate::requirements::{RequirementsSource, RequirementsSpecification};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Resolve a set of requirements into a set of pinned versions.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn pip_compile(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    extras: Vec<ExtraName>,
    output_file: Option<&Path>,
    resolution_mode: ResolutionMode,
    prerelease_mode: PreReleaseMode,
    upgrade_mode: UpgradeMode,
    index_urls: Option<IndexUrls>,
    cache: Option<&Path>,
    mut printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    // Read all requirements from the provided sources.
    let RequirementsSpecification {
        requirements,
        constraints,
        extras: used_extras,
    } = RequirementsSpecification::try_from_sources(requirements, constraints, &extras)?;

    // Check that all provided extras are used
    let given_extras = HashSet::from_iter(extras.clone());
    let unused_extras = given_extras.difference(&used_extras).collect_vec();
    if !unused_extras.is_empty() {
        let s = if unused_extras.len() == 1 { "" } else { "s" };
        return Err(anyhow!(
            "requested extra{s} not found: {}",
            unused_extras.iter().sorted().join(", ")
        ));
    }

    let preferences: Vec<Requirement> = output_file
        .filter(|_| upgrade_mode.is_prefer_pinned())
        .filter(|output_file| output_file.exists())
        .map(Path::to_path_buf)
        .map(RequirementsSource::from)
        .as_ref()
        .map(|source| RequirementsSpecification::try_from_source(source, &extras))
        .transpose()?
        .map(|spec| spec.requirements)
        .unwrap_or_default();

    // Create a manifest of the requirements.
    let manifest = Manifest::new(
        requirements,
        constraints,
        preferences,
        resolution_mode,
        prerelease_mode,
    );

    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, cache)?;

    debug!(
        "Using Python {} at {}",
        venv.interpreter_info().markers().python_version,
        venv.python_executable().display()
    );

    // Determine the compatible platform tags.
    let tags = Tags::from_env(
        venv.interpreter_info().platform(),
        venv.interpreter_info().simple_version(),
    )?;

    // Instantiate a client.
    let client = {
        let mut builder = RegistryClientBuilder::default();
        builder = builder.cache(cache);
        if let Some(IndexUrls { index, extra_index }) = index_urls {
            if let Some(index) = index {
                builder = builder.index(index);
            }
            builder = builder.extra_index(extra_index);
        } else {
            builder = builder.no_index();
        }
        builder.build()
    };

    let build_dispatch = BuildDispatch::new(
        RegistryClientBuilder::default().build(),
        cache.map(Path::to_path_buf),
        venv.interpreter_info().clone(),
        fs::canonicalize(venv.python_executable())?,
    );

    // Resolve the dependencies.
    let resolver = puffin_resolver::Resolver::new(
        manifest,
        venv.interpreter_info().markers(),
        &tags,
        &client,
        &build_dispatch,
    )
    .with_reporter(ResolverReporter::from(printer));
    let resolution = match resolver.resolve().await {
        Err(puffin_resolver::ResolveError::PubGrub(pubgrub::error::PubGrubError::NoSolution(
            mut derivation_tree,
        ))) => {
            derivation_tree.collapse_no_versions();
            #[allow(clippy::print_stderr)]
            {
                let report = miette::Report::msg(pubgrub::report::DefaultStringReporter::report(
                    &derivation_tree,
                ))
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

    if output_file.is_some() {
        colored::control::set_override(false);
    }

    let mut writer: Box<dyn std::io::Write> = if let Some(output_file) = output_file {
        Box::new(BufWriter::new(File::create(output_file)?))
    } else {
        Box::new(stdout())
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
        format!("#    {}", env::args().join(" ")).green()
    )?;
    write!(writer, "{resolution}")?;

    if output_file.is_some() {
        colored::control::unset_override();
    }

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

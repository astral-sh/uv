use std::fmt::Write;
use std::io::{stdout, BufWriter};
use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use fs_err::File;
use itertools::Itertools;
use pubgrub::report::Reporter;
use tracing::debug;

use pep508_rs::Requirement;
use platform_host::Platform;
use platform_tags::Tags;
use puffin_client::PypiClientBuilder;
use puffin_interpreter::PythonExecutable;

use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;
use crate::requirements::RequirementsSource;

/// Resolve a set of requirements into a set of pinned versions.
pub(crate) async fn pip_compile(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    output_file: Option<&Path>,
    cache: Option<&Path>,
    mut printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    // Read all requirements from the provided sources.
    let requirements = requirements
        .iter()
        .map(RequirementsSource::requirements)
        .flatten_ok()
        .collect::<Result<Vec<Requirement>>>()?;
    let constraints = constraints
        .iter()
        .map(RequirementsSource::requirements)
        .flatten_ok()
        .collect::<Result<Vec<Requirement>>>()?;

    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let python = PythonExecutable::from_env(platform, cache)?;
    debug!(
        "Using Python interpreter: {}",
        python.executable().display()
    );

    // Determine the current environment markers.
    let markers = python.markers();

    // Determine the compatible platform tags.
    let tags = Tags::from_env(python.platform(), python.simple_version())?;

    // Instantiate a client.
    let client = PypiClientBuilder::default().cache(cache).build();

    // Resolve the dependencies.
    let resolver =
        puffin_resolver::Resolver::new(requirements, constraints, markers, &tags, &client);
    let resolution = match resolver.resolve().await {
        Err(puffin_resolver::ResolveError::PubGrub(pubgrub::error::PubGrubError::NoSolution(
            mut derivation_tree,
        ))) => {
            derivation_tree.collapse_no_versions();
            #[allow(clippy::print_stderr)]
            {
                eprintln!("{}: {}", "error".red().bold(), "no solution found".bold());
                eprintln!(
                    "{}",
                    pubgrub::report::DefaultStringReporter::report(&derivation_tree)
                );
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

    if let Some(output_file) = output_file {
        resolution.write_requirement_format(&mut BufWriter::new(File::create(output_file)?))?;
    } else {
        resolution.write_requirement_format(&mut stdout().lock())?;
    };

    Ok(ExitStatus::Success)
}

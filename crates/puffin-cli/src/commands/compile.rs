use fs_err::File;
use std::fmt::Write;
use std::io::{stdout, BufWriter};
use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use pubgrub::report::Reporter;
use tracing::debug;

use platform_host::Platform;
use platform_tags::Tags;
use puffin_client::PypiClientBuilder;
use puffin_interpreter::PythonExecutable;
use puffin_package::requirements_txt::RequirementsTxt;

use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;

/// Resolve a set of requirements into a set of pinned versions.
pub(crate) async fn compile(
    src: &Path,
    output_file: Option<&Path>,
    cache: Option<&Path>,
    mut printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    // Read the `requirements.txt` from disk.
    let requirements_txt = RequirementsTxt::parse(src, std::env::current_dir()?)?;
    let requirements = requirements_txt
        .requirements
        .into_iter()
        .map(|entry| entry.requirement)
        .collect::<Vec<_>>();
    if requirements.is_empty() {
        writeln!(printer, "No requirements found")?;
        return Ok(ExitStatus::Success);
    }

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
    let client = {
        let mut pypi_client = PypiClientBuilder::default();
        if let Some(cache) = cache {
            pypi_client = pypi_client.cache(cache);
        }
        pypi_client.build()
    };

    // Resolve the dependencies.
    let resolver = puffin_resolver::Resolver::new(requirements, markers, &tags, &client);
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

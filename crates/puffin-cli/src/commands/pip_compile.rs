use std::env;
use std::fmt::Write;
use std::io::{stdout, BufWriter};
use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use fs_err::File;
use itertools::Itertools;
use pep508_rs::Requirement;
use platform_host::Platform;
use platform_tags::Tags;
use pubgrub::report::Reporter;
use puffin_client::RegistryClientBuilder;
use puffin_dispatch::PuffinDispatch;
use puffin_interpreter::PythonExecutable;
use puffin_resolver::ResolutionMode;
use tracing::debug;

use crate::commands::{elapsed, ExitStatus};
use crate::index_urls::IndexUrls;
use crate::printer::Printer;
use crate::requirements::RequirementsSource;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Resolve a set of requirements into a set of pinned versions.
pub(crate) async fn pip_compile(
    requirements: &[RequirementsSource],
    constraints: &[RequirementsSource],
    output_file: Option<&Path>,
    mode: ResolutionMode,
    index_urls: Option<IndexUrls>,
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
        "Using Python {} at {}",
        python.markers().python_version,
        python.executable().display()
    );

    // Determine the compatible platform tags.
    let tags = Tags::from_env(python.platform(), python.simple_version())?;

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

    let puffin_dispatch = PuffinDispatch::new(
        RegistryClientBuilder::default().build(),
        python.clone(),
        cache,
    );

    // Resolve the dependencies.
    let resolver = puffin_resolver::Resolver::new(
        requirements,
        constraints,
        mode,
        &python.markers(),
        &tags,
        &client,
        &puffin_dispatch,
    );
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

    Ok(ExitStatus::Success)
}

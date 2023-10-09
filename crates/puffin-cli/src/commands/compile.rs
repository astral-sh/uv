use std::fmt::Write;
use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use tracing::debug;

use platform_host::Platform;
use platform_tags::Tags;
use puffin_client::PypiClientBuilder;
use puffin_interpreter::PythonExecutable;
use puffin_package::requirements::Requirements;

use crate::commands::{elapsed, ExitStatus};
use crate::printer::Printer;

/// Resolve a set of requirements into a set of pinned versions.
pub(crate) async fn compile(
    src: &Path,
    cache: Option<&Path>,
    mut printer: Printer,
) -> Result<ExitStatus> {
    let start = std::time::Instant::now();

    // Read the `requirements.txt` from disk.
    let requirements_txt = std::fs::read_to_string(src)?;

    // Parse the `requirements.txt` into a list of requirements.
    let requirements = Requirements::from_str(&requirements_txt)?;

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
    let resolver = puffin_resolver::Resolver::new(markers, &tags, &client)
        .with_reporter(ProgressReporter::from(printer));
    let resolution = resolver
        .resolve(
            requirements.iter(),
            puffin_resolver::ResolveFlags::default(),
        )
        .await?;

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

    for (name, package) in resolution.iter() {
        #[allow(clippy::print_stdout)]
        {
            println!("{}=={}", name, package.version());
        }
    }

    Ok(ExitStatus::Success)
}

#[derive(Debug)]
struct ProgressReporter {
    progress: ProgressBar,
}

impl From<Printer> for ProgressReporter {
    fn from(printer: Printer) -> Self {
        let progress = ProgressBar::with_draw_target(None, printer.target());
        progress.set_message("Resolving dependencies...");
        progress.set_style(
            ProgressStyle::with_template("{bar:20} [{pos}/{len}] {wide_msg:.dim}").unwrap(),
        );
        Self { progress }
    }
}

impl puffin_resolver::Reporter for ProgressReporter {
    fn on_dependency_added(&self) {
        self.progress.inc_length(1);
    }

    fn on_resolve_progress(&self, package: &puffin_resolver::PinnedPackage) {
        self.progress.set_message(format!(
            "{}=={}",
            package.metadata.name, package.metadata.version
        ));
        self.progress.inc(1);
    }

    fn on_resolve_complete(&self) {
        self.progress.finish_and_clear();
    }
}

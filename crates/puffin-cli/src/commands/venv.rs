use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Result;
use fs_err as fs;
use miette::{Diagnostic, IntoDiagnostic};
use owo_colors::OwoColorize;
use thiserror::Error;

use distribution_types::{DistributionMetadata, IndexLocations, Name};
use pep508_rs::Requirement;
use platform_host::Platform;
use puffin_cache::Cache;
use puffin_client::{FlatIndex, FlatIndexClient, RegistryClientBuilder};
use puffin_dispatch::BuildDispatch;
use puffin_interpreter::Interpreter;
use puffin_traits::{BuildContext, InFlight, SetupPyStrategy};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Create a virtual environment.
#[allow(clippy::unnecessary_wraps)]
pub(crate) async fn venv(
    path: &Path,
    base_python: Option<&Path>,
    index_locations: &IndexLocations,
    seed: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    match venv_impl(path, base_python, index_locations, seed, cache, printer).await {
        Ok(status) => Ok(status),
        Err(err) => {
            #[allow(clippy::print_stderr)]
            {
                eprint!("{err:?}");
            }
            Ok(ExitStatus::Failure)
        }
    }
}

#[derive(Error, Debug, Diagnostic)]
enum VenvError {
    #[error("Unable to find a Python interpreter")]
    #[diagnostic(code(puffin::venv::python_not_found))]
    PythonNotFound,

    #[error("Unable to find a Python interpreter {0}")]
    #[diagnostic(code(puffin::venv::python_not_found))]
    UserPythonNotFound(PathBuf),

    #[error("Failed to extract Python interpreter info")]
    #[diagnostic(code(puffin::venv::interpreter))]
    InterpreterError(#[source] puffin_interpreter::Error),

    #[error("Failed to create virtual environment")]
    #[diagnostic(code(puffin::venv::creation))]
    CreationError(#[source] gourgeist::Error),

    #[error("Failed to install seed packages")]
    #[diagnostic(code(puffin::venv::seed))]
    SeedError(#[source] anyhow::Error),

    #[error("Failed to extract interpreter tags")]
    #[diagnostic(code(puffin::venv::tags))]
    TagsError(#[source] platform_host::PlatformError),

    #[error("Failed to resolve `--find-links` entry")]
    #[diagnostic(code(puffin::venv::flat_index))]
    FlatIndexError(#[source] puffin_client::FlatIndexError),
}

/// Create a virtual environment.
async fn venv_impl(
    path: &Path,
    base_python: Option<&Path>,
    index_locations: &IndexLocations,
    seed: bool,
    cache: &Cache,
    mut printer: Printer,
) -> miette::Result<ExitStatus> {
    // Locate the Python interpreter.
    let base_python = if let Some(base_python) = base_python {
        fs::canonicalize(
            which::which_global(base_python)
                .map_err(|_| VenvError::UserPythonNotFound(base_python.to_path_buf()))?,
        )
        .into_diagnostic()?
    } else {
        fs::canonicalize(
            which::which_global("python3")
                .or_else(|_| which::which_global("python"))
                .map_err(|_| VenvError::PythonNotFound)?,
        )
        .into_diagnostic()?
    };

    let platform = Platform::current().into_diagnostic()?;
    let interpreter =
        Interpreter::query(&base_python, platform, cache).map_err(VenvError::InterpreterError)?;

    writeln!(
        printer,
        "Using Python {} at {}",
        interpreter.version(),
        interpreter.sys_executable().display().cyan()
    )
    .into_diagnostic()?;

    writeln!(
        printer,
        "Creating virtual environment at: {}",
        path.display().cyan()
    )
    .into_diagnostic()?;

    // Create the virtual environment.
    let venv = gourgeist::create_venv(path, interpreter).map_err(VenvError::CreationError)?;

    // Install seed packages.
    if seed {
        // Extract the interpreter.
        let interpreter = venv.interpreter();

        // Instantiate a client.
        let client = RegistryClientBuilder::new(cache.clone()).build();

        // Resolve the flat indexes from `--find-links`.
        let flat_index = {
            let tags = interpreter.tags().map_err(VenvError::TagsError)?;
            let client = FlatIndexClient::new(&client, cache);
            let entries = client
                .fetch(index_locations.flat_indexes())
                .await
                .map_err(VenvError::FlatIndexError)?;
            FlatIndex::from_entries(entries, tags)
        };

        // Track in-flight downloads, builds, etc., across resolutions.
        let in_flight = InFlight::default();

        // Prep the build context.
        let build_dispatch = BuildDispatch::new(
            &client,
            cache,
            interpreter,
            index_locations,
            &flat_index,
            &in_flight,
            venv.python_executable(),
            SetupPyStrategy::default(),
            true,
        );

        // Resolve the seed packages.
        let resolution = build_dispatch
            .resolve(&[
                Requirement::from_str("wheel").unwrap(),
                Requirement::from_str("pip").unwrap(),
                Requirement::from_str("setuptools").unwrap(),
            ])
            .await
            .map_err(VenvError::SeedError)?;

        // Install into the environment.
        build_dispatch
            .install(&resolution, &venv)
            .await
            .map_err(VenvError::SeedError)?;

        for distribution in resolution.distributions() {
            writeln!(
                printer,
                " {} {}{}",
                "+".green(),
                distribution.name().as_ref().white().bold(),
                distribution.version_or_url().dimmed()
            )
            .into_diagnostic()?;
        }
    }

    Ok(ExitStatus::Success)
}

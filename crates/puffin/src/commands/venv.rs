use std::fmt::Write;
use std::path::Path;
use std::str::FromStr;

use anstream::eprint;
use anyhow::Result;
use chrono::{DateTime, Utc};
use miette::{Diagnostic, IntoDiagnostic};
use owo_colors::OwoColorize;
use thiserror::Error;

use distribution_types::{DistributionMetadata, IndexLocations, Name};
use pep508_rs::Requirement;
use platform_host::Platform;
use puffin_cache::Cache;
use puffin_client::{FlatIndex, FlatIndexClient, RegistryClientBuilder};
use puffin_dispatch::BuildDispatch;
use puffin_fs::Normalized;
use puffin_installer::NoBinary;
use puffin_interpreter::{find_default_python, find_requested_python, Error};
use puffin_resolver::{InMemoryIndex, OptionsBuilder};
use puffin_traits::{BuildContext, InFlight, NoBuild, SetupPyStrategy};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Create a virtual environment.
#[allow(clippy::unnecessary_wraps)]
pub(crate) async fn venv(
    path: &Path,
    python_request: Option<&str>,
    index_locations: &IndexLocations,
    seed: bool,
    exclude_newer: Option<DateTime<Utc>>,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    match venv_impl(
        path,
        python_request,
        index_locations,
        seed,
        exclude_newer,
        cache,
        printer,
    )
    .await
    {
        Ok(status) => Ok(status),
        Err(err) => {
            eprint!("{err:?}");
            Ok(ExitStatus::Failure)
        }
    }
}

#[derive(Error, Debug, Diagnostic)]
enum VenvError {
    #[error("Failed to create virtualenv")]
    #[diagnostic(code(puffin::venv::creation))]
    Creation(#[source] gourgeist::Error),

    #[error("Failed to install seed packages")]
    #[diagnostic(code(puffin::venv::seed))]
    Seed(#[source] anyhow::Error),

    #[error("Failed to extract interpreter tags")]
    #[diagnostic(code(puffin::venv::tags))]
    Tags(#[source] platform_tags::TagsError),

    #[error("Failed to resolve `--find-links` entry")]
    #[diagnostic(code(puffin::venv::flat_index))]
    FlatIndex(#[source] puffin_client::FlatIndexError),
}

/// Create a virtual environment.
async fn venv_impl(
    path: &Path,
    python_request: Option<&str>,
    index_locations: &IndexLocations,
    seed: bool,
    exclude_newer: Option<DateTime<Utc>>,
    cache: &Cache,
    mut printer: Printer,
) -> miette::Result<ExitStatus> {
    // Locate the Python interpreter.
    let platform = Platform::current().into_diagnostic()?;
    let interpreter = if let Some(python_request) = python_request {
        find_requested_python(python_request, &platform, cache)
            .into_diagnostic()?
            .ok_or(Error::NoSuchPython(python_request.to_string()))
            .into_diagnostic()?
    } else {
        find_default_python(&platform, cache).into_diagnostic()?
    };

    writeln!(
        printer,
        "Using Python {} interpreter at {}",
        interpreter.python_version(),
        interpreter.sys_executable().normalized_display().cyan()
    )
    .into_diagnostic()?;

    writeln!(
        printer,
        "Creating virtualenv at: {}",
        path.normalized_display().cyan()
    )
    .into_diagnostic()?;

    // Create the virtual environment.
    let venv = gourgeist::create_venv(path, interpreter).map_err(VenvError::Creation)?;

    // Install seed packages.
    if seed {
        // Extract the interpreter.
        let interpreter = venv.interpreter();

        // Instantiate a client.
        let client = RegistryClientBuilder::new(cache.clone()).build();

        // Resolve the flat indexes from `--find-links`.
        let flat_index = {
            let tags = interpreter.tags().map_err(VenvError::Tags)?;
            let client = FlatIndexClient::new(&client, cache);
            let entries = client
                .fetch(index_locations.flat_index())
                .await
                .map_err(VenvError::FlatIndex)?;
            FlatIndex::from_entries(entries, tags)
        };

        // Create a shared in-memory index.
        let index = InMemoryIndex::default();

        // Track in-flight downloads, builds, etc., across resolutions.
        let in_flight = InFlight::default();

        // Prep the build context.
        let options = OptionsBuilder::new().exclude_newer(exclude_newer).build();
        let build_dispatch = BuildDispatch::new(
            &client,
            cache,
            interpreter,
            index_locations,
            &flat_index,
            &index,
            &in_flight,
            venv.python_executable(),
            SetupPyStrategy::default(),
            &NoBuild::All,
            &NoBinary::None,
        )
        .with_options(options);

        // Resolve the seed packages.
        let resolution = build_dispatch
            .resolve(&[
                Requirement::from_str("wheel").unwrap(),
                Requirement::from_str("pip").unwrap(),
                Requirement::from_str("setuptools").unwrap(),
            ])
            .await
            .map_err(VenvError::Seed)?;

        // Install into the environment.
        build_dispatch
            .install(&resolution, &venv)
            .await
            .map_err(VenvError::Seed)?;

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

use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::debug;

use uv_cache::Cache;
use uv_cli::ExternalCommand;
use uv_client::BaseClientBuilder;
use uv_configuration::{Concurrency, Constraints, Preview};
use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::PackageName;
use uv_pep440::VersionSpecifiers;
use uv_pep508::MarkerTree;
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest,
};
use uv_requirements::RequirementsSpecification;
use uv_settings::PythonInstallMirrors;

use crate::commands::project::environment::CachedEnvironment;
use crate::commands::project::{EnvironmentSpecification, PlatformState};
use crate::commands::ExitStatus;
use crate::commands::pip::loggers::{SummaryInstallLogger, SummaryResolveLogger};
use crate::commands::reporters::PythonDownloadReporter;
use crate::printer::Printer;
use crate::settings::{NetworkSettings, ResolverInstallerSettings};

/// Format Python source files using Ruff.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn format(
    check: bool,
    diff: bool,
    files: Vec<PathBuf>,
    args: Option<ExternalCommand>,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverInstallerSettings,
    network_settings: NetworkSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    cache: Cache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    // Create a Ruff requirement.
    let ruff_requirement = Requirement {
        name: PackageName::from_str("ruff")?,
        extras: Box::new([]),
        groups: Box::new([]),
        marker: MarkerTree::default(),
        source: RequirementSource::Registry {
            specifier: VersionSpecifiers::empty(),
            index: None,
            conflict: None,
        },
        origin: None,
    };

    // Get or create the Python environment.
    let client_builder = BaseClientBuilder::new()
        .retries_from_env()?
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .allow_insecure_host(network_settings.allow_insecure_host.clone());

    let reporter = PythonDownloadReporter::single(printer);

    let python_request = python.as_deref().map(PythonRequest::parse);

    // Discover an interpreter.
    let interpreter = PythonInstallation::find_or_download(
        python_request.as_ref(),
        EnvironmentPreference::Any,
        python_preference,
        python_downloads,
        &client_builder,
        &cache,
        Some(&reporter),
        install_mirrors.python_install_mirror.as_deref(),
        install_mirrors.pypy_install_mirror.as_deref(),
        install_mirrors.python_downloads_json_url.as_deref(),
        preview,
    )
    .await?
    .into_interpreter();

    // Initialize shared state.
    let state = PlatformState::default();

    // Create a requirements specification with Ruff.
    let spec = EnvironmentSpecification::from(RequirementsSpecification {
        requirements: vec![ruff_requirement.into()],
        constraints: vec![],
        overrides: vec![],
        ..Default::default()
    });

    // Create or reuse a cached environment.
    let environment = CachedEnvironment::from_spec(
        spec,
        Constraints::default(),
        &interpreter,
        &settings,
        &network_settings,
        &state,
        Box::new(SummaryResolveLogger),
        Box::new(SummaryInstallLogger),
        installer_metadata,
        concurrency,
        &cache,
        printer,
        preview,
    )
    .await?;

    let environment: PythonEnvironment = environment.into();

    // Construct the ruff format command.
    let mut command = Command::new(environment.scripts().join("ruff"));
    command.arg("format");

    // Add check flag if requested.
    if check {
        command.arg("--check");
    }

    // Add diff flag if requested.
    if diff {
        command.arg("--diff");
    }

    // Add files or directories to format.
    if files.is_empty() {
        // If no files specified, format the current directory.
        command.arg(".");
    } else {
        for file in &files {
            command.arg(file);
        }
    }

    // Add any additional arguments passed after --.
    if let Some(args) = args {
        for arg in args.iter() {
            command.arg(arg);
        }
    }

    debug!("Running ruff format command: {:?}", command);

    // Run the ruff format command.
    let output = command.output().await.context("Failed to run ruff format")?;

    // Stream stdout and stderr.
    if !output.stdout.is_empty() {
        std::io::stdout().write_all(&output.stdout)?;
    }
    if !output.stderr.is_empty() {
        std::io::stderr().write_all(&output.stderr)?;
    }

    // Return the exit status.
    if output.status.success() {
        Ok(ExitStatus::Success)
    } else {
        Ok(ExitStatus::Failure)
    }
}
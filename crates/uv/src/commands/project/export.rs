use std::env;

use anyhow::{Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use std::path::{Path, PathBuf};

use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{
    Concurrency, DevMode, DevSpecification, EditableMode, ExportFormat, ExtrasSpecification,
    InstallOptions, LowerBound, TrustedHost,
};
use uv_normalize::{PackageName, DEV_DEPENDENCIES};
use uv_python::{PythonDownloads, PythonPreference, PythonRequest};
use uv_resolver::RequirementsTxtExport;
use uv_workspace::{DiscoveryOptions, MemberDiscovery, VirtualProject, Workspace};

use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::project::lock::do_safe_lock;
use crate::commands::project::{ProjectError, ProjectInterpreter};
use crate::commands::{diagnostics, pip, ExitStatus, OutputWriter, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverSettings;

/// Export the project's `uv.lock` in an alternate format.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn export(
    project_dir: &Path,
    format: ExportFormat,
    package: Option<PackageName>,
    hashes: bool,
    install_options: InstallOptions,
    output_file: Option<PathBuf>,
    extras: ExtrasSpecification,
    dev: DevMode,
    editable: EditableMode,
    locked: bool,
    frozen: bool,
    include_header: bool,
    python: Option<String>,
    settings: ResolverSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    quiet: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Identify the project.
    let project = if let Some(package) = package {
        VirtualProject::Project(
            Workspace::discover(project_dir, &DiscoveryOptions::default())
                .await?
                .with_current_project(package.clone())
                .with_context(|| format!("Package `{package}` not found in workspace"))?,
        )
    } else if frozen {
        VirtualProject::discover(
            project_dir,
            &DiscoveryOptions {
                members: MemberDiscovery::None,
                ..DiscoveryOptions::default()
            },
        )
        .await?
    } else {
        VirtualProject::discover(project_dir, &DiscoveryOptions::default()).await?
    };

    let VirtualProject::Project(project) = project else {
        return Err(anyhow::anyhow!("Legacy non-project roots are not supported in `uv export`; add a `[project]` table to your `pyproject.toml` to enable exports"));
    };

    // Find an interpreter for the project
    let interpreter = ProjectInterpreter::discover(
        project.workspace(),
        python.as_deref().map(PythonRequest::parse),
        python_preference,
        python_downloads,
        connectivity,
        native_tls,
        allow_insecure_host,
        cache,
        printer,
    )
    .await?
    .into_interpreter();

    // Initialize any shared state.
    let state = SharedState::default();

    // Lock the project.
    let lock = match do_safe_lock(
        locked,
        frozen,
        project.workspace(),
        &interpreter,
        settings.as_ref(),
        LowerBound::Warn,
        &state,
        Box::new(DefaultResolveLogger),
        connectivity,
        concurrency,
        native_tls,
        allow_insecure_host,
        cache,
        printer,
    )
    .await
    {
        Ok(result) => result.into_lock(),
        Err(ProjectError::Operation(pip::operations::Error::Resolve(
            uv_resolver::ResolveError::NoSolution(err),
        ))) => {
            diagnostics::no_solution(&err);
            return Ok(ExitStatus::Failure);
        }
        Err(ProjectError::Operation(pip::operations::Error::Resolve(
            uv_resolver::ResolveError::FetchAndBuild(dist, err),
        ))) => {
            diagnostics::fetch_and_build(dist, err);
            return Ok(ExitStatus::Failure);
        }
        Err(ProjectError::Operation(pip::operations::Error::Resolve(
            uv_resolver::ResolveError::Build(dist, err),
        ))) => {
            diagnostics::build(dist, err);
            return Ok(ExitStatus::Failure);
        }
        Err(err) => return Err(err.into()),
    };

    // Include development dependencies, if requested.
    let dev = match dev {
        DevMode::Include => DevSpecification::Include(std::slice::from_ref(&DEV_DEPENDENCIES)),
        DevMode::Exclude => DevSpecification::Exclude,
        DevMode::Only => DevSpecification::Only(std::slice::from_ref(&DEV_DEPENDENCIES)),
    };

    // Write the resolved dependencies to the output channel.
    let mut writer = OutputWriter::new(!quiet || output_file.is_none(), output_file.as_deref());

    // Generate the export.
    match format {
        ExportFormat::RequirementsTxt => {
            let export = RequirementsTxtExport::from_lock(
                &lock,
                project.project_name(),
                &extras,
                dev,
                editable,
                hashes,
                &install_options,
            )?;

            if include_header {
                writeln!(
                    writer,
                    "{}",
                    "# This file was autogenerated by uv via the following command:".green()
                )?;
                writeln!(writer, "{}", format!("#    {}", cmd()).green())?;
            }
            write!(writer, "{export}")?;
        }
    }

    writer.commit().await?;

    Ok(ExitStatus::Success)
}

/// Format the uv command used to generate the output file.
fn cmd() -> String {
    let args = env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().to_string())
        .scan(None, move |skip_next, arg| {
            if matches!(skip_next, Some(true)) {
                // Reset state; skip this iteration.
                *skip_next = None;
                return Some(None);
            }

            // Always skip the `--upgrade` flag.
            if arg == "--upgrade" || arg == "-U" {
                *skip_next = None;
                return Some(None);
            }

            // Always skip the `--upgrade-package` and mark the next item to be skipped
            if arg == "--upgrade-package" || arg == "-P" {
                *skip_next = Some(true);
                return Some(None);
            }

            // Skip only this argument if option and value are together
            if arg.starts_with("--upgrade-package=") || arg.starts_with("-P") {
                // Reset state; skip this iteration.
                *skip_next = None;
                return Some(None);
            }

            // Always skip the `--quiet` flag.
            if arg == "--quiet" || arg == "-q" {
                *skip_next = None;
                return Some(None);
            }

            // Always skip the `--verbose` flag.
            if arg == "--verbose" || arg == "-v" {
                *skip_next = None;
                return Some(None);
            }

            // Return the argument.
            Some(Some(arg))
        })
        .flatten()
        .join(" ");
    format!("uv {args}")
}

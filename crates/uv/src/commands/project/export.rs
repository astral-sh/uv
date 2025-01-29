use std::env;

use anyhow::{Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use std::path::{Path, PathBuf};
use uv_settings::PythonInstallMirrors;

use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{
    Concurrency, DevGroupsSpecification, EditableMode, ExportFormat, ExtrasSpecification,
    InstallOptions, LowerBound, PreviewMode, TrustedHost,
};
use uv_normalize::PackageName;
use uv_python::{PythonDownloads, PythonPreference, PythonRequest};
use uv_resolver::RequirementsTxtExport;
use uv_scripts::{Pep723ItemRef, Pep723Script};
use uv_workspace::{DiscoveryOptions, MemberDiscovery, VirtualProject, Workspace};

use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::lock::{do_safe_lock, LockMode};
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{
    default_dependency_groups, detect_conflicts, DependencyGroupsTarget, ProjectError,
    ProjectInterpreter, ScriptInterpreter, UniversalState,
};
use crate::commands::{diagnostics, ExitStatus, OutputWriter};
use crate::printer::Printer;
use crate::settings::ResolverSettings;

#[derive(Debug, Clone)]
enum ExportTarget {
    /// A PEP 723 script, with inline metadata.
    Script(Pep723Script),

    /// A project with a `pyproject.toml`.
    Project(VirtualProject),
}

impl<'lock> From<&'lock ExportTarget> for LockTarget<'lock> {
    fn from(value: &'lock ExportTarget) -> Self {
        match value {
            ExportTarget::Script(script) => Self::Script(script),
            ExportTarget::Project(project) => Self::Workspace(project.workspace()),
        }
    }
}

/// Export the project's `uv.lock` in an alternate format.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn export(
    project_dir: &Path,
    format: ExportFormat,
    all_packages: bool,
    package: Option<PackageName>,
    prune: Vec<PackageName>,
    hashes: bool,
    install_options: InstallOptions,
    output_file: Option<PathBuf>,
    extras: ExtrasSpecification,
    dev: DevGroupsSpecification,
    editable: EditableMode,
    locked: bool,
    frozen: bool,
    include_header: bool,
    script: Option<Pep723Script>,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    allow_insecure_host: &[TrustedHost],
    no_config: bool,
    quiet: bool,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    // Identify the target.
    let target = if let Some(script) = script {
        ExportTarget::Script(script)
    } else {
        let project = if frozen {
            VirtualProject::discover(
                project_dir,
                &DiscoveryOptions {
                    members: MemberDiscovery::None,
                    ..DiscoveryOptions::default()
                },
            )
            .await?
        } else if let Some(package) = package.as_ref() {
            VirtualProject::Project(
                Workspace::discover(project_dir, &DiscoveryOptions::default())
                    .await?
                    .with_current_project(package.clone())
                    .with_context(|| format!("Package `{package}` not found in workspace"))?,
            )
        } else {
            VirtualProject::discover(project_dir, &DiscoveryOptions::default()).await?
        };
        ExportTarget::Project(project)
    };

    // Validate that any referenced dependency groups are defined in the workspace.
    if !frozen {
        let target = match &target {
            ExportTarget::Project(VirtualProject::Project(project)) => {
                if all_packages {
                    DependencyGroupsTarget::Workspace(project.workspace())
                } else {
                    DependencyGroupsTarget::Project(project)
                }
            }
            ExportTarget::Project(VirtualProject::NonProject(workspace)) => {
                DependencyGroupsTarget::Workspace(workspace)
            }
            ExportTarget::Script(_) => DependencyGroupsTarget::Script,
        };
        target.validate(&dev)?;
    }

    // Determine the default groups to include.
    let defaults = match &target {
        ExportTarget::Project(project) => default_dependency_groups(project.pyproject_toml())?,
        ExportTarget::Script(_) => vec![],
    };
    let dev = dev.with_defaults(defaults);

    // Find an interpreter for the project, unless `--frozen` is set.
    let interpreter = if frozen {
        None
    } else {
        Some(match &target {
            ExportTarget::Script(script) => ScriptInterpreter::discover(
                Pep723ItemRef::Script(script),
                python.as_deref().map(PythonRequest::parse),
                python_preference,
                python_downloads,
                connectivity,
                native_tls,
                allow_insecure_host,
                &install_mirrors,
                no_config,
                cache,
                printer,
            )
            .await?
            .into_interpreter(),
            ExportTarget::Project(project) => ProjectInterpreter::discover(
                project.workspace(),
                project_dir,
                python.as_deref().map(PythonRequest::parse),
                python_preference,
                python_downloads,
                connectivity,
                native_tls,
                allow_insecure_host,
                &install_mirrors,
                no_config,
                cache,
                printer,
            )
            .await?
            .into_interpreter(),
        })
    };

    // Determine the lock mode.
    let mode = if frozen {
        LockMode::Frozen
    } else if locked {
        LockMode::Locked(interpreter.as_ref().unwrap())
    } else if matches!(target, ExportTarget::Script(_))
        && !LockTarget::from(&target).lock_path().is_file()
    {
        // If we're locking a script, avoid creating a lockfile if it doesn't already exist.
        LockMode::DryRun(interpreter.as_ref().unwrap())
    } else {
        LockMode::Write(interpreter.as_ref().unwrap())
    };

    // Initialize any shared state.
    let state = UniversalState::default();

    // Lock the project.
    let lock = match do_safe_lock(
        mode,
        (&target).into(),
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
        preview,
    )
    .await
    {
        Ok(result) => result.into_lock(),
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::native_tls(native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()))
        }
        Err(err) => return Err(err.into()),
    };

    // Validate that the set of requested extras and development groups are compatible.
    detect_conflicts(&lock, &extras, &dev)?;

    // Identify the installation target.
    let target = match &target {
        ExportTarget::Project(VirtualProject::Project(project)) => {
            if all_packages {
                InstallTarget::Workspace {
                    workspace: project.workspace(),
                    lock: &lock,
                }
            } else if let Some(package) = package.as_ref() {
                InstallTarget::Project {
                    workspace: project.workspace(),
                    name: package,
                    lock: &lock,
                }
            } else {
                // By default, install the root package.
                InstallTarget::Project {
                    workspace: project.workspace(),
                    name: project.project_name(),
                    lock: &lock,
                }
            }
        }
        ExportTarget::Project(VirtualProject::NonProject(workspace)) => {
            if all_packages {
                InstallTarget::NonProjectWorkspace {
                    workspace,
                    lock: &lock,
                }
            } else if let Some(package) = package.as_ref() {
                InstallTarget::Project {
                    workspace,
                    name: package,
                    lock: &lock,
                }
            } else {
                // By default, install the entire workspace.
                InstallTarget::NonProjectWorkspace {
                    workspace,
                    lock: &lock,
                }
            }
        }
        ExportTarget::Script(script) => InstallTarget::Script {
            script,
            lock: &lock,
        },
    };

    // Write the resolved dependencies to the output channel.
    let mut writer = OutputWriter::new(!quiet || output_file.is_none(), output_file.as_deref());

    // Generate the export.
    match format {
        ExportFormat::RequirementsTxt => {
            let export = RequirementsTxtExport::from_lock(
                &target,
                &prune,
                &extras,
                &dev,
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

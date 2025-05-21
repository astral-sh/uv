use std::fmt::Write;
use std::str::FromStr;
use std::{cmp::Ordering, path::Path};

use anyhow::{Context, Result, anyhow};
use owo_colors::OwoColorize;

use tracing::debug;
use uv_cache::Cache;
use uv_cli::version::VersionInfo;
use uv_cli::{VersionBump, VersionFormat};
use uv_configuration::{
    Concurrency, DependencyGroups, DryRun, EditableMode, ExtrasSpecification, InstallOptions,
    PreviewMode,
};
use uv_fs::Simplified;
use uv_normalize::DefaultExtras;
use uv_pep440::Version;
use uv_pep508::PackageName;
use uv_python::{PythonDownloads, PythonPreference, PythonRequest};
use uv_settings::PythonInstallMirrors;
use uv_warnings::warn_user;
use uv_workspace::pyproject_mut::Error;
use uv_workspace::{
    DiscoveryOptions, WorkspaceCache,
    pyproject_mut::{DependencyTarget, PyProjectTomlMut},
};
use uv_workspace::{VirtualProject, Workspace};

use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger};
use crate::commands::pip::operations::Modifications;
use crate::commands::project::add::{AddTarget, PythonTarget};
use crate::commands::project::install_target::InstallTarget;
use crate::commands::project::lock::LockMode;
use crate::commands::project::{
    ProjectEnvironment, ProjectError, ProjectInterpreter, UniversalState, default_dependency_groups,
};
use crate::commands::{ExitStatus, diagnostics, project};
use crate::printer::Printer;
use crate::settings::{NetworkSettings, ResolverInstallerSettings};

/// Display version information for uv itself (`uv self version`)
pub(crate) fn self_version(
    short: bool,
    output_format: VersionFormat,
    printer: Printer,
) -> Result<ExitStatus> {
    let version_info = uv_cli::version::uv_self_version();
    print_version(version_info, None, short, output_format, printer)?;

    Ok(ExitStatus::Success)
}

/// Read or update project version (`uv version`)
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn project_version(
    value: Option<String>,
    bump: Option<VersionBump>,
    short: bool,
    output_format: VersionFormat,
    strict: bool,
    project_dir: &Path,
    package: Option<PackageName>,
    dry_run: bool,
    locked: bool,
    frozen: bool,
    active: Option<bool>,
    no_sync: bool,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverInstallerSettings,
    network_settings: NetworkSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    no_config: bool,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    // Read the metadata
    let project = match find_target(project_dir, package.as_ref()).await {
        Ok(target) => target,
        Err(err) => {
            // If strict, hard bail on failing to find the pyproject.toml
            if strict {
                return Err(err)?;
            }
            // Otherwise, warn and provide fallback to the old `uv version` from before 0.7.0
            warn_user!(
                "Failed to read project metadata ({err}). Running `{}` for compatibility. This fallback will be removed in the future; pass `--preview` to force an error.",
                "uv self version".green()
            );
            return self_version(short, output_format, printer);
        }
    };

    let pyproject_path = project.root().join("pyproject.toml");
    let Some(name) = project.project_name().cloned() else {
        return Err(anyhow!(
            "Missing `project.name` field in: {}",
            pyproject_path.user_display()
        ));
    };

    // Short-circuit early for a frozen read
    let is_read_only = value.is_none() && bump.is_none();
    if frozen && is_read_only {
        return Box::pin(print_frozen_version(
            project,
            &name,
            project_dir,
            active,
            python,
            install_mirrors,
            &settings,
            network_settings,
            python_preference,
            python_downloads,
            concurrency,
            no_config,
            cache,
            short,
            output_format,
            printer,
            preview,
        ))
        .await;
    }

    let mut toml = PyProjectTomlMut::from_toml(
        project.pyproject_toml().raw.as_ref(),
        DependencyTarget::PyProjectToml,
    )?;

    let old_version = toml.version().map_err(|err| match err {
        Error::MalformedWorkspace => {
            if toml.has_dynamic_version() {
                anyhow!(
                    "We cannot get or set dynamic project versions in: {}",
                    pyproject_path.user_display()
                )
            } else {
                anyhow!(
                    "There is no 'project.version' field in: {}",
                    pyproject_path.user_display()
                )
            }
        }
        err => {
            anyhow!("{err}: {}", pyproject_path.user_display())
        }
    })?;

    // Figure out new metadata
    let new_version = if let Some(value) = value {
        match Version::from_str(&value) {
            Ok(version) => Some(version),
            Err(err) => match &*value {
                "major" | "minor" | "patch" => {
                    return Err(anyhow!(
                        "Invalid version `{value}`, did you mean to pass `--bump {value}`?"
                    ));
                }
                _ => {
                    return Err(err)?;
                }
            },
        }
    } else if let Some(bump) = bump {
        Some(bumped_version(&old_version, bump, printer)?)
    } else {
        None
    };

    // Update the toml and lock
    let status = if dry_run {
        ExitStatus::Success
    } else if let Some(new_version) = &new_version {
        let project = update_project(project, new_version, &mut toml, &pyproject_path)?;
        Box::pin(lock_and_sync(
            project,
            project_dir,
            locked,
            frozen,
            active,
            no_sync,
            python,
            install_mirrors,
            &settings,
            network_settings,
            python_preference,
            python_downloads,
            installer_metadata,
            concurrency,
            no_config,
            cache,
            printer,
            preview,
        ))
        .await?
    } else {
        debug!("No changes to version; skipping update");
        ExitStatus::Success
    };

    // Report the results
    let old_version = VersionInfo::new(Some(&name), &old_version);
    let new_version = new_version.map(|version| VersionInfo::new(Some(&name), &version));
    print_version(old_version, new_version, short, output_format, printer)?;

    Ok(status)
}

/// Find the pyproject.toml we're modifying
///
/// Note that `uv version` never needs to support PEP 723 scripts, as those are unversioned.
async fn find_target(project_dir: &Path, package: Option<&PackageName>) -> Result<VirtualProject> {
    // Find the project in the workspace.
    // No workspace caching since `uv version` changes the workspace definition.
    let project = if let Some(package) = package {
        VirtualProject::Project(
            Workspace::discover(
                project_dir,
                &DiscoveryOptions::default(),
                &WorkspaceCache::default(),
            )
            .await?
            .with_current_project(package.clone())
            .with_context(|| format!("Package `{package}` not found in workspace"))?,
        )
    } else {
        VirtualProject::discover(
            project_dir,
            &DiscoveryOptions::default(),
            &WorkspaceCache::default(),
        )
        .await?
    };
    Ok(project)
}

/// Update the pyproject.toml on-disk and in-memory with a new version
fn update_project(
    project: VirtualProject,
    new_version: &Version,
    toml: &mut PyProjectTomlMut,
    pyproject_path: &Path,
) -> Result<VirtualProject> {
    // Save to disk
    toml.set_version(new_version)?;
    let content = toml.to_string();
    fs_err::write(pyproject_path, &content)?;

    // Update the `pyproject.toml` in-memory.
    let project = project
        .with_pyproject_toml(toml::from_str(&content).map_err(ProjectError::PyprojectTomlParse)?)
        .ok_or(ProjectError::PyprojectTomlUpdate)?;

    Ok(project)
}

/// Do the minimal work to try to find the package in the lockfile and print its version
async fn print_frozen_version(
    project: VirtualProject,
    name: &PackageName,
    project_dir: &Path,
    active: Option<bool>,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: &ResolverInstallerSettings,
    network_settings: NetworkSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    concurrency: Concurrency,
    no_config: bool,
    cache: &Cache,
    short: bool,
    output_format: VersionFormat,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    // Discover the interpreter (this is the same interpreter --no-sync uses).
    let interpreter = ProjectInterpreter::discover(
        project.workspace(),
        project_dir,
        python.as_deref().map(PythonRequest::parse),
        &network_settings,
        python_preference,
        python_downloads,
        &install_mirrors,
        false,
        no_config,
        active,
        cache,
        printer,
    )
    .await?
    .into_interpreter();

    let target = AddTarget::Project(project, Box::new(PythonTarget::Interpreter(interpreter)));

    // Initialize any shared state.
    let state = UniversalState::default();

    // Lock and sync the environment, if necessary.
    let lock = match project::lock::LockOperation::new(
        LockMode::Frozen,
        &settings.resolver,
        &network_settings,
        &state,
        Box::new(DefaultResolveLogger),
        concurrency,
        cache,
        printer,
        preview,
    )
    .execute((&target).into())
    .await
    {
        Ok(result) => result.into_lock(),
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
        }
        Err(err) => return Err(err.into()),
    };

    // Try to find the package of interest in the lock
    let Some(package) = lock
        .packages()
        .iter()
        .find(|package| package.name() == name)
    else {
        return Err(anyhow!(
            "Failed to find the {name}'s version in the frozen lockfile"
        ));
    };
    let Some(version) = package.version() else {
        return Err(anyhow!(
            "Failed to find the {name}'s version in the frozen lockfile"
        ));
    };

    // Finally, print!
    let old_version = VersionInfo::new(Some(name), version);
    print_version(old_version, None, short, output_format, printer)?;

    Ok(ExitStatus::Success)
}

/// Re-lock and re-sync the project after a series of edits.
#[allow(clippy::fn_params_excessive_bools)]
async fn lock_and_sync(
    project: VirtualProject,
    project_dir: &Path,
    locked: bool,
    frozen: bool,
    active: Option<bool>,
    no_sync: bool,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: &ResolverInstallerSettings,
    network_settings: NetworkSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    no_config: bool,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    // If frozen, don't touch the lock or sync at all
    if frozen {
        return Ok(ExitStatus::Success);
    }

    // Convert to an `AddTarget` by attaching the appropriate interpreter or environment.
    let target = if no_sync {
        // Discover the interpreter.
        let interpreter = ProjectInterpreter::discover(
            project.workspace(),
            project_dir,
            python.as_deref().map(PythonRequest::parse),
            &network_settings,
            python_preference,
            python_downloads,
            &install_mirrors,
            false,
            no_config,
            active,
            cache,
            printer,
        )
        .await?
        .into_interpreter();

        AddTarget::Project(project, Box::new(PythonTarget::Interpreter(interpreter)))
    } else {
        // Discover or create the virtual environment.
        let environment = ProjectEnvironment::get_or_init(
            project.workspace(),
            python.as_deref().map(PythonRequest::parse),
            &install_mirrors,
            &network_settings,
            python_preference,
            python_downloads,
            no_sync,
            no_config,
            active,
            cache,
            DryRun::Disabled,
            printer,
        )
        .await?
        .into_environment()?;

        AddTarget::Project(project, Box::new(PythonTarget::Environment(environment)))
    };

    // Determine the lock mode.
    let mode = if locked {
        LockMode::Locked(target.interpreter())
    } else {
        LockMode::Write(target.interpreter())
    };

    // Initialize any shared state.
    let state = UniversalState::default();

    // Lock and sync the environment, if necessary.
    let lock = match project::lock::LockOperation::new(
        mode,
        &settings.resolver,
        &network_settings,
        &state,
        Box::new(DefaultResolveLogger),
        concurrency,
        cache,
        printer,
        preview,
    )
    .execute((&target).into())
    .await
    {
        Ok(result) => result.into_lock(),
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
        }
        Err(err) => return Err(err.into()),
    };

    let AddTarget::Project(project, environment) = target else {
        // If we're not adding to a project, exit early.
        return Ok(ExitStatus::Success);
    };

    let PythonTarget::Environment(venv) = &*environment else {
        // If we're not syncing, exit early.
        return Ok(ExitStatus::Success);
    };

    // Perform a full sync, because we don't know what exactly is affected by the version.
    // TODO(ibraheem): Should we accept CLI overrides for this? Should we even sync here?
    let extras = ExtrasSpecification::from_all_extras();
    let install_options = InstallOptions::default();

    // Determine the default groups to include.
    let default_groups = default_dependency_groups(project.pyproject_toml())?;

    // Determine the default extras to include.
    let default_extras = DefaultExtras::default();

    // Identify the installation target.
    let target = match &project {
        VirtualProject::Project(project) => InstallTarget::Project {
            workspace: project.workspace(),
            name: project.project_name(),
            lock: &lock,
        },
        VirtualProject::NonProject(workspace) => InstallTarget::NonProjectWorkspace {
            workspace,
            lock: &lock,
        },
    };

    let state = state.fork();

    match project::sync::do_sync(
        target,
        venv,
        &extras.with_defaults(default_extras),
        &DependencyGroups::default().with_defaults(default_groups),
        EditableMode::Editable,
        install_options,
        Modifications::Sufficient,
        settings.into(),
        &network_settings,
        &state,
        Box::new(DefaultInstallLogger),
        installer_metadata,
        concurrency,
        cache,
        DryRun::Disabled,
        printer,
        preview,
    )
    .await
    {
        Ok(()) => {}
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::native_tls(network_settings.native_tls)
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
        }
        Err(err) => return Err(err.into()),
    }

    Ok(ExitStatus::Success)
}

fn print_version(
    old_version: VersionInfo,
    new_version: Option<VersionInfo>,
    short: bool,
    output_format: VersionFormat,
    printer: Printer,
) -> Result<()> {
    match output_format {
        VersionFormat::Text => {
            if let Some(name) = &old_version.package_name {
                if !short {
                    write!(printer.stdout(), "{name} ")?;
                }
            }
            if let Some(new_version) = new_version {
                if short {
                    writeln!(printer.stdout(), "{}", new_version.cyan())?;
                } else {
                    writeln!(
                        printer.stdout(),
                        "{} => {}",
                        old_version.cyan(),
                        new_version.cyan()
                    )?;
                }
            } else {
                writeln!(printer.stdout(), "{}", old_version.cyan())?;
            }
        }
        VersionFormat::Json => {
            let final_version = new_version.unwrap_or(old_version);
            let string = serde_json::to_string_pretty(&final_version)?;
            writeln!(printer.stdout(), "{string}")?;
        }
    }
    Ok(())
}

fn bumped_version(from: &Version, bump: VersionBump, printer: Printer) -> Result<Version> {
    // All prereleasey details "carry to 0" with every currently supported mode of `--bump`
    // We could go out of our way to preserve epoch information but no one uses those...
    if from.any_prerelease() || from.is_post() || from.is_local() || from.epoch() > 0 {
        writeln!(
            printer.stderr(),
            "warning: prerelease information will be cleared as part of the version bump"
        )?;
    }

    let index = match bump {
        VersionBump::Major => 0,
        VersionBump::Minor => 1,
        VersionBump::Patch => 2,
    };

    // Use `max` here to try to do 0.2 => 0.3 instead of 0.2 => 0.3.0
    let old_parts = from.release();
    let len = old_parts.len().max(index + 1);
    let new_release_vec = (0..len)
        .map(|i| match i.cmp(&index) {
            // Everything before the bumped value is preserved (or is an implicit 0)
            Ordering::Less => old_parts.get(i).copied().unwrap_or(0),
            // This is the value to bump (could be implicit 0)
            Ordering::Equal => old_parts.get(i).copied().unwrap_or(0) + 1,
            // Everything after the bumped value becomes 0
            Ordering::Greater => 0,
        })
        .collect::<Vec<u64>>();
    Ok(Version::new(new_release_vec))
}

use std::fmt::Write;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result, anyhow};
use owo_colors::OwoColorize;

use tracing::debug;
use uv_cache::Cache;
use uv_cli::version::VersionInfo;
use uv_cli::{VersionBump, VersionFormat};
use uv_configuration::{
    Concurrency, DependencyGroups, DependencyGroupsWithDefaults, DryRun, EditableMode,
    ExtrasSpecification, InstallOptions, PreviewMode,
};
use uv_fs::Simplified;
use uv_normalize::DefaultExtras;
use uv_pep440::{BumpCommand, PrereleaseKind, Version};
use uv_pep508::PackageName;
use uv_python::{PythonDownloads, PythonPreference, PythonRequest};
use uv_settings::PythonInstallMirrors;
use uv_workspace::pyproject_mut::Error;
use uv_workspace::{
    DiscoveryOptions, WorkspaceCache, WorkspaceError,
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
    mut bump: Vec<VersionBump>,
    short: bool,
    output_format: VersionFormat,
    project_dir: &Path,
    package: Option<PackageName>,
    explicit_project: bool,
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
    let project = find_target(project_dir, package.as_ref(), explicit_project).await?;

    let pyproject_path = project.root().join("pyproject.toml");
    let Some(name) = project.project_name().cloned() else {
        return Err(anyhow!(
            "Missing `project.name` field in: {}",
            pyproject_path.user_display()
        ));
    };

    // Short-circuit early for a frozen read
    let is_read_only = value.is_none() && bump.is_empty();
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
                "major" | "minor" | "patch" | "alpha" | "beta" | "rc" | "dev" | "post"
                | "stable" => {
                    return Err(anyhow!(
                        "Invalid version `{value}`, did you mean to pass `--bump {value}`?"
                    ));
                }
                _ => {
                    return Err(err)?;
                }
            },
        }
    } else if !bump.is_empty() {
        // While we can rationalize many of these combinations of operations together,
        // we want to conservatively refuse to support any of them until users demand it.
        //
        // The most complex thing we *do* allow is `--bump major --bump beta --bump dev`
        // because that makes perfect sense and is reasonable to do.
        let release_components: Vec<_> = bump
            .iter()
            .filter(|bump| {
                matches!(
                    bump,
                    VersionBump::Major | VersionBump::Minor | VersionBump::Patch
                )
            })
            .collect();
        let prerelease_components: Vec<_> = bump
            .iter()
            .filter(|bump| {
                matches!(
                    bump,
                    VersionBump::Alpha | VersionBump::Beta | VersionBump::Rc | VersionBump::Dev
                )
            })
            .collect();
        let post_count = bump
            .iter()
            .filter(|bump| *bump == &VersionBump::Post)
            .count();
        let stable_count = bump
            .iter()
            .filter(|bump| *bump == &VersionBump::Stable)
            .count();

        // Very little reason to do "bump to stable" and then do other things,
        // even if we can make sense of it.
        if stable_count > 0 && bump.len() > 1 {
            let components = bump
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(anyhow!(
                "`--bump stable` cannot be used with another `--bump` value, got: {components}"
            ));
        }

        // Very little reason to "bump to post" and then do other things,
        // how is it a post-release otherwise?
        if post_count > 0 && bump.len() > 1 {
            let components = bump
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(anyhow!(
                "`--bump post` cannot be used with another `--bump` value, got: {components}"
            ));
        }

        // `--bump major --bump minor` makes perfect sense (1.2.3 => 2.1.0)
        // ...but it's weird and probably a mistake?
        // `--bump major --bump major` perfect sense (1.2.3 => 3.0.0)
        // ...but it's weird and probably a mistake?
        if release_components.len() > 1 {
            let components = release_components
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(anyhow!(
                "Only one release version component can be provided to `--bump`, got: {components}"
            ));
        }

        // `--bump alpha --bump beta` is basically completely incoherent
        // `--bump beta --bump beta` makes perfect sense (1.2.3b4 => 1.2.3b6)
        // ...but it's weird and probably a mistake?
        // `--bump beta --bump dev` makes perfect sense (1.2.3 => 1.2.3b1.dev1)
        // ...but we want to discourage mixing `dev` with pre-releases
        if prerelease_components.len() > 1 {
            let components = prerelease_components
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(anyhow!(
                "Only one pre-release version component can be provided to `--bump`, got: {components}"
            ));
        }

        // Sort the given commands so the user doesn't have to care about
        // the ordering of `--bump minor --bump beta` (only one ordering is ever useful)
        bump.sort();

        // Apply all the bumps
        let mut new_version = old_version.clone();
        for bump in &bump {
            let command = match *bump {
                VersionBump::Major => BumpCommand::BumpRelease { index: 0 },
                VersionBump::Minor => BumpCommand::BumpRelease { index: 1 },
                VersionBump::Patch => BumpCommand::BumpRelease { index: 2 },
                VersionBump::Alpha => BumpCommand::BumpPrerelease {
                    kind: PrereleaseKind::Alpha,
                },
                VersionBump::Beta => BumpCommand::BumpPrerelease {
                    kind: PrereleaseKind::Beta,
                },
                VersionBump::Rc => BumpCommand::BumpPrerelease {
                    kind: PrereleaseKind::Rc,
                },
                VersionBump::Post => BumpCommand::BumpPost,
                VersionBump::Dev => BumpCommand::BumpDev,
                VersionBump::Stable => BumpCommand::MakeStable,
            };
            new_version.bump(command);
        }

        if new_version <= old_version {
            if old_version.is_stable() && new_version.is_pre() {
                return Err(anyhow!(
                    "{old_version} => {new_version} didn't increase the version; when bumping to a pre-release version you also need to increase a release version component, e.g., with `--bump <major|minor|patch>`"
                ));
            }
            return Err(anyhow!(
                "{old_version} => {new_version} didn't increase the version; provide the exact version to force an update"
            ));
        }

        Some(new_version)
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

/// Add hint to use `uv self version` when workspace discovery fails due to missing pyproject.toml
/// and --project was not explicitly passed
fn hint_uv_self_version(err: WorkspaceError, explicit_project: bool) -> anyhow::Error {
    if matches!(err, WorkspaceError::MissingPyprojectToml) && !explicit_project {
        anyhow!(
            "{}\n\n{}{} If you meant to view uv's version, use `{}` instead",
            err,
            "hint".bold().cyan(),
            ":".bold(),
            "uv self version".green()
        )
    } else {
        err.into()
    }
}

/// Find the pyproject.toml we're modifying
///
/// Note that `uv version` never needs to support PEP 723 scripts, as those are unversioned.
async fn find_target(
    project_dir: &Path,
    package: Option<&PackageName>,
    explicit_project: bool,
) -> Result<VirtualProject> {
    // Find the project in the workspace.
    // No workspace caching since `uv version` changes the workspace definition.
    let project = if let Some(package) = package {
        VirtualProject::Project(
            Workspace::discover(
                project_dir,
                &DiscoveryOptions::default(),
                &WorkspaceCache::default(),
            )
            .await
            .map_err(|err| hint_uv_self_version(err, explicit_project))?
            .with_current_project(package.clone())
            .with_context(|| format!("Package `{package}` not found in workspace"))?,
        )
    } else {
        VirtualProject::discover(
            project_dir,
            &DiscoveryOptions::default(),
            &WorkspaceCache::default(),
        )
        .await
        .map_err(|err| hint_uv_self_version(err, explicit_project))?
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
        &DependencyGroupsWithDefaults::none(),
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
        preview,
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
        &WorkspaceCache::default(),
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

    // Determine the groups and extras that should be enabled.
    let default_groups = default_dependency_groups(project.pyproject_toml())?;
    let default_extras = DefaultExtras::default();
    let groups = DependencyGroups::default().with_defaults(default_groups);
    let extras = ExtrasSpecification::default().with_defaults(default_extras);
    let install_options = InstallOptions::default();

    // Convert to an `AddTarget` by attaching the appropriate interpreter or environment.
    let target = if no_sync {
        // Discover the interpreter.
        let interpreter = ProjectInterpreter::discover(
            project.workspace(),
            project_dir,
            &groups,
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
            preview,
        )
        .await?
        .into_interpreter();

        AddTarget::Project(project, Box::new(PythonTarget::Interpreter(interpreter)))
    } else {
        // Discover or create the virtual environment.
        let environment = ProjectEnvironment::get_or_init(
            project.workspace(),
            &groups,
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
            preview,
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
    let workspace_cache = WorkspaceCache::default();

    // Lock and sync the environment, if necessary.
    let lock = match project::lock::LockOperation::new(
        mode,
        &settings.resolver,
        &network_settings,
        &state,
        Box::new(DefaultResolveLogger),
        concurrency,
        cache,
        &workspace_cache,
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
        &extras,
        &groups,
        EditableMode::Editable,
        install_options,
        Modifications::Sufficient,
        None,
        settings.into(),
        &network_settings,
        &state,
        Box::new(DefaultInstallLogger),
        installer_metadata,
        concurrency,
        cache,
        workspace_cache,
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

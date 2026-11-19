use std::fmt::Write;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Result, anyhow};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_command_support::{ExitStatus, Printer, UvError};
use uv_configuration::{
    ActiveEnvironment, Concurrency, DependencyGroups, DryRun, ExtrasSpecification, InstallOptions,
    Modifications, VersionBump, VersionBumpSpec, VersionFormat,
};
use uv_dispatch::UniversalState;
use uv_environment_operations::install_target::{InstallTarget, PackageSelection};
use uv_environment_operations::malware::MalwareCheckContext;
use uv_environment_operations::{
    LinkErrorReporting, ProjectEnvironment, ProjectEnvironmentPolicy, ProjectEnvironmentTarget,
    ProjectInterpreter, sync_from_lock,
};
use uv_fs::Simplified;
use uv_install_operations::loggers::DefaultInstallLogger;
use uv_lock_operations::{LockMode, LockOperation, LockTarget};
use uv_normalize::{DefaultExtras, PackageName};
use uv_pep440::{BumpCommand, PrereleaseKind, Version};
use uv_preview::Preview;
use uv_project_edit::{DependencyTarget, Error, PyProjectTomlMut};
use uv_python_discovery::ConfigDiscovery;
use uv_python_discovery::ProjectPythonRequest;
use uv_python_types::{PythonArchitecture, PythonDownloads, PythonPreference, PythonRequest};
use uv_resolve_operations::loggers::DefaultResolveLogger;
use uv_settings::{
    FrozenSource, LockCheck, MalwareCheckSettings, PythonInstallMirrors, ResolverInstallerSettings,
};
use uv_workspace::pyproject::PyProjectToml;
use uv_workspace::{
    DiscoveryOptions, ProjectWorkspace, VirtualProject, WorkspaceCache, WorkspaceError,
    WorkspaceErrorKind,
};

use crate::ProjectError;
use crate::edit::{ProjectEdit, PythonTarget};

/// Version information for a project (`uv version`).
#[derive(serde::Serialize)]
struct ProjectVersionInfo {
    /// Name of the package.
    package_name: Option<String>,
    /// Version, such as "0.5.1".
    version: String,
    /// Always `null` for project versions, kept for backwards compatibility.
    // TODO(zanieb): Remove this field in a breaking release.
    commit_info: Option<()>,
}

impl ProjectVersionInfo {
    fn new(package_name: Option<&PackageName>, version: &Version) -> Self {
        Self {
            package_name: package_name.map(ToString::to_string),
            version: version.to_string(),
            commit_info: None,
        }
    }
}

impl std::fmt::Display for ProjectVersionInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.version)
    }
}

/// Read or update project version (`uv version`)
#[expect(clippy::fn_params_excessive_bools)]
pub async fn project_version(
    value: Option<String>,
    mut bump: Vec<VersionBumpSpec>,
    short: bool,
    output_format: VersionFormat,
    project_dir: &Path,
    package: Option<PackageName>,
    explicit_project: bool,
    dry_run: bool,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    active: ActiveEnvironment,
    no_sync: bool,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverInstallerSettings,
    client_builder: BaseClientBuilder<'_>,
    python_preference: PythonPreference,
    python_arch: Option<PythonArchitecture>,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    config_discovery: ConfigDiscovery,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
    malware_settings: MalwareCheckSettings,
) -> Result<ExitStatus> {
    // Read the metadata
    let project = find_target(
        project_dir,
        package.as_ref(),
        explicit_project,
        cache,
        workspace_cache,
    )
    .await?;

    let pyproject_path = project.root().join("pyproject.toml");
    let Some(name) = project.project_name().cloned() else {
        return Err(anyhow!(
            "Missing `project.name` field in: {}",
            pyproject_path.user_display()
        ));
    };

    // Short-circuit early for a frozen read
    let is_read_only = value.is_none() && bump.is_empty();
    if let Some(frozen_source) = frozen {
        if is_read_only {
            return print_frozen_version(
                project,
                &name,
                frozen_source,
                short,
                output_format,
                printer,
            )
            .await;
        }
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
            .filter(|spec| {
                matches!(
                    spec.bump,
                    VersionBump::Major | VersionBump::Minor | VersionBump::Patch
                )
            })
            .collect();
        let prerelease_components: Vec<_> = bump
            .iter()
            .filter(|spec| {
                matches!(
                    spec.bump,
                    VersionBump::Alpha | VersionBump::Beta | VersionBump::Rc | VersionBump::Dev
                )
            })
            .collect();
        let post_count = bump
            .iter()
            .filter(|spec| spec.bump == VersionBump::Post)
            .count();
        let stable_count = bump
            .iter()
            .filter(|spec| spec.bump == VersionBump::Stable)
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

        for spec in &bump {
            match spec.bump {
                VersionBump::Major => new_version.bump(BumpCommand::BumpRelease {
                    index: 0,
                    value: spec.value,
                }),
                VersionBump::Minor => new_version.bump(BumpCommand::BumpRelease {
                    index: 1,
                    value: spec.value,
                }),
                VersionBump::Patch => new_version.bump(BumpCommand::BumpRelease {
                    index: 2,
                    value: spec.value,
                }),
                VersionBump::Stable => new_version.bump(BumpCommand::MakeStable),
                VersionBump::Alpha => new_version.bump(BumpCommand::BumpPrerelease {
                    kind: PrereleaseKind::Alpha,
                    value: spec.value,
                }),
                VersionBump::Beta => new_version.bump(BumpCommand::BumpPrerelease {
                    kind: PrereleaseKind::Beta,
                    value: spec.value,
                }),
                VersionBump::Rc => new_version.bump(BumpCommand::BumpPrerelease {
                    kind: PrereleaseKind::Rc,
                    value: spec.value,
                }),
                VersionBump::Post => new_version.bump(BumpCommand::BumpPost { value: spec.value }),
                VersionBump::Dev => new_version.bump(BumpCommand::BumpDev { value: spec.value }),
            }
        }

        if new_version <= old_version {
            if old_version.is_stable() && new_version.is_pre() {
                return Err(anyhow!(
                    "{old_version} => {new_version} didn't increase the version; when bumping to a pre-release version you also need to increase a release version component, e.g., with `--bump <major|minor|patch>`"
                ));
            }
            if new_version.is_dev() && !old_version.is_dev() {
                return Err(anyhow!(
                    "{old_version} => {new_version} didn't increase the version; when bumping to a dev version you also need to increase another version component, e.g., with `--bump <major|minor|patch|alpha|beta|rc>`"
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
        let edit = ProjectEdit::new(
            [pyproject_path.clone()].into_iter().chain(
                frozen
                    .is_none()
                    .then(|| LockTarget::from(project.workspace()).lock_path()),
            ),
        )?;
        let project = update_project(
            project,
            new_version,
            &mut toml,
            &pyproject_path,
            workspace_cache,
        )?;
        let status = Box::pin(lock_and_sync(
            project,
            project_dir,
            lock_check,
            frozen,
            active,
            no_sync,
            python,
            install_mirrors,
            &settings,
            client_builder,
            python_preference,
            python_arch,
            python_downloads,
            installer_metadata,
            &concurrency,
            config_discovery,
            cache,
            printer,
            preview,
            &malware_settings,
        ))
        .await?;
        edit.commit();
        status
    } else {
        debug!("No changes to version; skipping update");
        ExitStatus::Success
    };

    // Report the results
    let old_version = ProjectVersionInfo::new(Some(&name), &old_version);
    let new_version = new_version.map(|version| ProjectVersionInfo::new(Some(&name), &version));
    print_version(old_version, new_version, short, output_format, printer)?;

    Ok(status)
}

/// Add hint to use `uv self version` when workspace discovery fails due to missing pyproject.toml
/// and --project was not explicitly passed
fn hint_uv_self_version(err: WorkspaceError, explicit_project: bool) -> ProjectError {
    if matches!(err.as_ref(), WorkspaceErrorKind::MissingPyprojectToml) && !explicit_project {
        ProjectError::MissingProjectVersion(err)
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
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
) -> Result<VirtualProject, ProjectError> {
    // Find the project in the workspace.
    let project = if let Some(package) = package {
        VirtualProject::discover_with_package(
            project_dir,
            &DiscoveryOptions::default(),
            cache,
            workspace_cache,
            package.clone(),
        )
        .await
        .map_err(|err| hint_uv_self_version(err, explicit_project))?
    } else {
        // Configuration discovery may have cached errors from virtual workspace member discovery.
        // `uv version` requires a project, so reject non-project roots before consulting that cache.
        let project_workspace_cache = WorkspaceCache::default();
        VirtualProject::Project(
            ProjectWorkspace::discover(
                project_dir,
                &DiscoveryOptions::default(),
                cache,
                &project_workspace_cache,
            )
            .await
            .map_err(|err| hint_uv_self_version(err, explicit_project))?,
        )
    };
    Ok(project)
}

/// Update the pyproject.toml on-disk and in-memory with a new version
fn update_project(
    project: VirtualProject,
    new_version: &Version,
    toml: &mut PyProjectTomlMut,
    pyproject_path: &Path,
    workspace_cache: &WorkspaceCache,
) -> Result<VirtualProject> {
    // Save to disk
    toml.set_version(new_version)?;
    let content = toml.to_string();
    fs_err::write(pyproject_path, &content)?;

    // Update the `pyproject.toml` in-memory.
    let project = project
        .update_member(
            PyProjectToml::from_string(content, pyproject_path)
                .map_err(ProjectError::PyprojectTomlParse)?,
            workspace_cache,
        )?
        .ok_or(ProjectError::PyprojectTomlUpdate)?;

    Ok(project)
}

/// Print the project's version from its existing lockfile.
async fn print_frozen_version(
    project: VirtualProject,
    name: &PackageName,
    frozen_source: FrozenSource,
    short: bool,
    output_format: VersionFormat,
    printer: Printer,
) -> Result<ExitStatus> {
    let target = LockTarget::Workspace(project.workspace());
    let lock = target
        .read_frozen(frozen_source.into())
        .await
        .map_err(UvError::from)?;

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
    let old_version = ProjectVersionInfo::new(Some(name), version);
    print_version(old_version, None, short, output_format, printer)?;

    Ok(ExitStatus::Success)
}

/// Re-lock and re-sync the project after a series of edits.
async fn lock_and_sync(
    project: VirtualProject,
    project_dir: &Path,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    active: ActiveEnvironment,
    no_sync: bool,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    settings: &ResolverInstallerSettings,
    client_builder: BaseClientBuilder<'_>,
    python_preference: PythonPreference,
    python_arch: Option<PythonArchitecture>,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: &Concurrency,
    config_discovery: ConfigDiscovery,
    cache: &Cache,
    printer: Printer,
    preview: Preview,
    malware_settings: &MalwareCheckSettings,
) -> Result<ExitStatus> {
    // If frozen, don't touch the lock or sync at all
    if frozen.is_some() {
        return Ok(ExitStatus::Success);
    }

    // Determine the groups and extras that should be enabled.
    let default_groups = project.default_groups()?;
    let default_extras = DefaultExtras::default();
    let groups = DependencyGroups::default().with_defaults(default_groups);
    let extras = ExtrasSpecification::default().with_defaults(default_extras);
    let install_options = InstallOptions::default();

    // Discover the interpreter or environment used to lock and sync the project.
    let python_target = if no_sync {
        // Discover the interpreter.
        let project_python = ProjectPythonRequest::from_request(
            python.as_deref().map(PythonRequest::parse),
            Some(project.workspace()),
            &groups,
            project_dir,
            config_discovery,
        )
        .await?;
        let interpreter = ProjectInterpreter::discover(
            ProjectEnvironmentTarget::from(project.workspace()),
            project_python,
            &client_builder,
            python_preference,
            python_arch,
            python_downloads,
            &install_mirrors,
            ProjectEnvironmentPolicy::Optional,
            active,
            cache,
            printer,
        )
        .await?
        .into_interpreter();

        PythonTarget::Interpreter(interpreter)
    } else {
        // Discover or create the virtual environment.
        let environment = ProjectEnvironment::get_or_init(
            ProjectEnvironmentTarget::from(project.workspace()),
            None,
            &groups,
            python.as_deref().map(PythonRequest::parse),
            &install_mirrors,
            &client_builder,
            python_preference,
            python_arch,
            python_downloads,
            no_sync,
            config_discovery,
            active,
            cache,
            DryRun::Disabled,
            LinkErrorReporting::User,
            printer,
        )
        .await?
        .into_environment()?;

        PythonTarget::Environment(environment)
    };

    // Determine the lock mode.
    let mode = if let LockCheck::Enabled(lock_check) = lock_check {
        LockMode::Locked(python_target.interpreter(), lock_check)
    } else {
        LockMode::Write(python_target.interpreter())
    };

    // Initialize any shared state.
    let state = UniversalState::default();
    let workspace_cache = WorkspaceCache::default();

    // Lock and sync the environment, if necessary.
    let lock = match Box::pin(
        LockOperation::new(
            mode,
            &settings.resolver,
            &client_builder,
            &state,
            Box::new(DefaultResolveLogger),
            concurrency,
            cache,
            &workspace_cache,
            printer,
            preview,
        )
        .execute(project.workspace().into()),
    )
    .await
    {
        Ok(result) => result.into_lock(),
        Err(err) => return Err(UvError::from(err).into()),
    };

    let PythonTarget::Environment(venv) = &python_target else {
        // If we're not syncing, exit early.
        return Ok(ExitStatus::Success);
    };

    // Perform a full sync, because we don't know what exactly is affected by the version.

    // Identify the installation target.
    let target = InstallTarget::from_project(
        &project,
        &lock,
        PackageSelection::from_args(false, &[], project.project_name()),
    );

    let state = state.fork();

    match sync_from_lock(
        target,
        venv,
        &extras,
        &groups,
        None,
        install_options,
        Modifications::Sufficient,
        None,
        settings.into(),
        &client_builder,
        &state,
        Box::new(DefaultInstallLogger),
        installer_metadata,
        concurrency,
        cache,
        &workspace_cache,
        DryRun::Disabled,
        printer,
        preview,
        MalwareCheckContext::from(malware_settings),
    )
    .await
    {
        Ok(_) => {}
        Err(err) => return Err(UvError::from(err).into()),
    }

    Ok(ExitStatus::Success)
}

fn print_version(
    old_version: ProjectVersionInfo,
    new_version: Option<ProjectVersionInfo>,
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
            writeln!(printer.stdout_important(), "{string}")?;
        }
    }
    Ok(())
}

use std::path::Path;

use anyhow::{Result, anyhow};

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_command_support::Printer;
use uv_configuration::{
    ActiveEnvironment, Concurrency, DependencyGroupsWithDefaults, InstallOptions,
};
use uv_dispatch::UniversalState;
use uv_distribution_types::Resolution;
use uv_environment_operations::environment::CachedEnvironment;
use uv_environment_operations::install_target::{InstallTarget, PackageSelection};
use uv_environment_operations::{
    EnvironmentError, ProjectEnvironmentPolicy, ProjectEnvironmentTarget, ProjectInterpreter,
    apply_no_virtual_project, store_credentials_from_target,
};
use uv_fs::{Simplified, normalize_path};
use uv_install_operations::loggers::SummaryInstallLogger;
use uv_lock::{Installable, Lock, Package, SelectedDependency};
use uv_lock_operations::{LockMode, LockOperation, LockTarget};
use uv_normalize::{DEV_DEPENDENCIES, GroupName, PackageName};
use uv_preview::Preview;
use uv_python_discovery::{ConfigDiscovery, ProjectPythonRequest};
use uv_python_interpreter::{Interpreter, PythonEnvironment};
use uv_python_types::{PythonArchitecture, PythonDownloads, PythonPreference};
use uv_resolve_operations::loggers::SummaryResolveLogger;
use uv_settings::{
    LockCheck, MalwareCheckSettings, PythonInstallMirrors, ResolverInstallerSettings,
    ResolverSettings,
};
use uv_warnings::warn_user;
use uv_workspace::{
    DiscoveryOptions, MemberDiscovery, VirtualProject, WorkspaceCache, WorkspaceErrorKind,
};

use crate::ProjectError;

/// A locked package selected for use as a project tool.
pub(super) struct LockedTool<'lock> {
    dependency: SelectedDependency<'lock>,
    requires_separate_environment: bool,
}

impl<'lock> LockedTool<'lock> {
    pub(super) fn dependency(&self) -> &SelectedDependency<'lock> {
        &self.dependency
    }

    /// Returns `true` if the tool must be installed outside the selected project environment.
    pub(super) fn requires_separate_environment(&self) -> bool {
        self.requires_separate_environment
    }
}

/// Find a locked tool in a dependency group, falling back to the project's production dependencies.
pub(super) fn find_locked_tool<'lock>(
    project_name: Option<&PackageName>,
    lock: &'lock Lock,
    interpreter: &Interpreter,
    package_name: &PackageName,
    dependency_group: &GroupName,
    groups: &DependencyGroupsWithDefaults,
) -> Result<Option<LockedTool<'lock>>> {
    let marker_environment = interpreter.to_resolver_marker_environment();
    let selection = lock
        .dependency_selection(project_name, package_name, marker_environment.markers())
        .map_err(anyhow::Error::msg)?;
    let (dependency, installed) = if let Some(dependency) = selection.group(dependency_group) {
        (dependency.clone(), groups.contains(dependency_group))
    } else if let Some(dependency) = selection.production() {
        (dependency.clone(), groups.prod())
    } else {
        return Ok(None);
    };

    Ok(Some(LockedTool {
        dependency,
        requires_separate_environment: !installed,
    }))
}

/// Installs a locked tool and its dependencies into a cached environment.
pub(super) struct LockedToolInstaller<'a> {
    pub(super) settings: &'a ResolverInstallerSettings,
    pub(super) malware_settings: &'a MalwareCheckSettings,
    pub(super) client_builder: &'a BaseClientBuilder<'a>,
    pub(super) state: &'a UniversalState,
    pub(super) installer_metadata: bool,
    pub(super) concurrency: &'a Concurrency,
    pub(super) cache: &'a Cache,
    pub(super) printer: Printer,
    pub(super) preview: Preview,
}

impl LockedToolInstaller<'_> {
    pub(super) async fn install(
        &self,
        target: InstallTarget<'_>,
        dependency: &SelectedDependency<'_>,
        interpreter: &Interpreter,
    ) -> Result<(PythonEnvironment, Resolution), EnvironmentError> {
        let markers = interpreter.to_resolver_marker_environment();
        let resolution = target.lock().to_resolution_from_dependency(
            target.install_path(),
            dependency,
            target.project_name(),
            &markers,
            interpreter.tags()?,
            &self.settings.resolver.build_options,
            &InstallOptions::default(),
        )?;
        let resolution = apply_no_virtual_project(resolution);
        store_credentials_from_target(target, self.client_builder)?;
        let environment = CachedEnvironment::from_locked_resolution(
            &resolution,
            target.lock().build_constraints(target.install_path()),
            interpreter,
            self.settings,
            self.malware_settings,
            self.client_builder,
            &self.state.fork(),
            Box::new(SummaryInstallLogger),
            self.installer_metadata,
            self.concurrency,
            self.cache,
            self.printer,
            self.preview,
        )
        .await?;
        Ok((PythonEnvironment::from(environment), resolution))
    }

    /// Install the checker from the script's containing workspace if its lockfile supplies one.
    pub(super) async fn install_from_script_workspace(
        &self,
        script_dir: &Path,
        name: &PackageName,
        lock_check: LockCheck,
        workspace_resolver_settings: impl FnOnce(&Path) -> Result<ResolverSettings>,
        workspace_cache: &WorkspaceCache,
        config_discovery: ConfigDiscovery,
        python_preference: PythonPreference,
        python_arch: Option<PythonArchitecture>,
        python_downloads: PythonDownloads,
        install_mirrors: &PythonInstallMirrors,
    ) -> Result<Option<PythonEnvironment>, ProjectError> {
        // Collapse `..` before walking ancestors, keeping symlinked members under their workspace
        // root. Use the physical path if collapsing `symlink/..` changes the directory it names.
        let mut directory = normalize_path(script_dir);
        if directory.as_ref() != script_dir {
            let physical = fs_err::tokio::canonicalize(script_dir).await?;
            if !fs_err::tokio::canonicalize(directory.as_ref())
                .await
                .is_ok_and(|path| path == physical)
            {
                directory = physical.into();
            }
        }
        let project = match VirtualProject::discover(
            directory.simplified(),
            &DiscoveryOptions {
                members: MemberDiscovery::None,
                ..DiscoveryOptions::default()
            },
            self.cache,
            workspace_cache,
        )
        .await
        {
            Ok(project) => project,
            Err(err)
                if matches!(
                    err.as_ref(),
                    WorkspaceErrorKind::MissingPyprojectToml
                        | WorkspaceErrorKind::MissingProject(_)
                        | WorkspaceErrorKind::NonWorkspace(_)
                ) =>
            {
                return Ok(None);
            }
            Err(err) => return Err(err.into()),
        };
        let workspace = project.workspace();
        let Some(lock) = LockTarget::Workspace(workspace).read().await? else {
            return Ok(None);
        };
        let dependency_group = &*DEV_DEPENDENCIES;
        // Check for a direct declaration before discovering or downloading workspace Python.
        if !lock.has_root_dependency(name, dependency_group) {
            return Ok(None);
        }

        let selection = PackageSelection::from_args(false, &[], lock.root().map(Package::name));
        let target = InstallTarget::from_project(&project, &lock, selection);
        // Only the checker subgraph is installed. Select one workspace interpreter without
        // group-specific bounds, then evaluate both declarations against its markers.
        let groups = DependencyGroupsWithDefaults::none();
        let requirement = target.python_requirement(&groups)?;
        let project_python = ProjectPythonRequest::from_requirements(
            None,
            Some(workspace.install_path()),
            Some(requirement),
            workspace.install_path(),
            config_discovery,
        )
        .await
        .map_err(EnvironmentError::from)?;
        let interpreter = ProjectInterpreter::discover(
            ProjectEnvironmentTarget::from(workspace),
            project_python,
            self.client_builder,
            python_preference,
            python_arch,
            python_downloads,
            install_mirrors,
            ProjectEnvironmentPolicy::Optional,
            ActiveEnvironment::Ignore,
            self.cache,
            self.printer,
        )
        .await?
        .into_interpreter();
        let interpreter = CachedEnvironment::base_interpreter(&interpreter, self.cache)
            .map_err(EnvironmentError::from)?;
        target.validate_platform(&interpreter.to_resolver_marker_environment())?;
        let Some(tool) = find_locked_tool(
            target.project_name(),
            &lock,
            &interpreter,
            name,
            dependency_group,
            &groups,
        )?
        else {
            warn_user!(
                "No locked `{name}` dependency applies to workspace Python {}. Using the standalone checker.",
                interpreter.python_minor_version()
            );
            return Ok(None);
        };
        let workspace_settings = workspace_resolver_settings(workspace.install_path())?;
        if let LockCheck::Enabled(source) = lock_check {
            // Once the workspace supplies the tool, validate its lock against all members.
            // TODO(zsol): Limit validation to the selected tool and its dependencies.
            let project = VirtualProject::discover(
                workspace.install_path(),
                &DiscoveryOptions::default(),
                self.cache,
                workspace_cache,
            )
            .await?;
            let validated = Box::pin(
                LockOperation::new(
                    LockMode::Locked(&interpreter, source),
                    &workspace_settings,
                    self.client_builder,
                    &UniversalState::default(),
                    Box::new(SummaryResolveLogger),
                    self.concurrency,
                    self.cache,
                    workspace_cache,
                    self.printer,
                    self.preview,
                )
                .execute(LockTarget::Workspace(project.workspace())),
            )
            .await?;
            if validated.lock() != &lock {
                return Err(anyhow!(
                    "The workspace lockfile changed while selecting `{name}`. Retry the command"
                )
                .into());
            }
        }
        // Use the workspace's index names to look up credentials for locked URLs.
        // Copy only the index locations to retain the build policy of `uv check`.
        let mut settings = self.settings.clone();
        settings.resolver.index_locations = workspace_settings.index_locations;
        let installer = LockedToolInstaller {
            settings: &settings,
            ..*self
        };
        let (environment, _) = installer
            .install(target, tool.dependency(), &interpreter)
            .await?;
        Ok(Some(environment))
    }
}

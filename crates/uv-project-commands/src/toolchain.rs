use std::collections::hash_map::Entry;
use std::path::Path;
use std::slice;

use anyhow::Result;
use rustc_hash::FxHashMap;

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_command_support::{Printer, UvError};
use uv_configuration::{
    ActiveEnvironment, Concurrency, DependencyGroups, DependencyGroupsWithDefaults, InstallOptions,
};
use uv_dispatch::UniversalState;
use uv_distribution_types::Resolution;
use uv_environment_operations::environment::CachedEnvironment;
use uv_environment_operations::install_target::{InstallTarget, PackageSelection};
use uv_environment_operations::{
    ProjectEnvironmentPolicy, ProjectEnvironmentTarget, ProjectInterpreter,
    store_credentials_from_target,
};
use uv_install_operations::loggers::SummaryInstallLogger;
use uv_lock::{Installable, Lock, SelectedDependency};
use uv_lock_operations::LockTarget;
use uv_normalize::{DEV_DEPENDENCIES, DefaultGroups, GroupName, PackageName};
use uv_pep508::{MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString};
use uv_preview::Preview;
use uv_python_discovery::{ConfigDiscovery, ProjectPythonRequest};
use uv_python_interpreter::{Interpreter, PythonEnvironment};
use uv_python_types::{
    ImplementationName, PythonArchitecture, PythonDownloadRequest, PythonDownloads,
    PythonPreference,
};
use uv_settings::{MalwareCheckSettings, PythonInstallMirrors, ResolverInstallerSettings};
use uv_workspace::{
    DiscoveryOptions, MemberDiscovery, VirtualProject, WorkspaceCache, WorkspaceErrorKind,
};

/// A locked package selected for use as a project tool.
pub(super) struct LockedTool<'lock> {
    pub(super) dependency: SelectedDependency<'lock>,
    /// Whether the tool must be installed outside the selected project environment.
    pub(super) requires_separate_environment: bool,
}

/// Find a tool in a dependency group, falling back to the current project's production
/// dependencies.
pub(super) fn find_locked_tool<'lock>(
    project: &VirtualProject,
    lock: &'lock Lock,
    interpreter: &Interpreter,
    package_name: &PackageName,
    dependency_group: &GroupName,
    groups: &DependencyGroupsWithDefaults,
) -> Result<Option<LockedTool<'lock>>> {
    let marker_environment = interpreter.to_resolver_marker_environment();
    let selection = lock
        .dependency_selection(
            project.project_name(),
            package_name,
            marker_environment.markers(),
        )
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

/// A direct declaration at the workspace root. Its group determines the checker's Python requirement.
enum WorkspaceToolSource<'lock> {
    Production,
    Group(&'lock GroupName),
}

impl<'lock> WorkspaceToolSource<'lock> {
    fn candidates(
        lock: &'lock Lock,
        name: &PackageName,
    ) -> impl Iterator<Item = (Self, MarkerTree)> {
        let groups: Vec<_> = if let Some(root) = lock.root() {
            root.resolved_dependency_groups().keys().collect()
        } else {
            lock.dependency_groups().keys().collect()
        };
        [Self::Group(&DEV_DEPENDENCIES), Self::Production]
            .into_iter()
            .chain(
                groups
                    .into_iter()
                    .filter(|&group| group != &*DEV_DEPENDENCIES)
                    .map(Self::Group),
            )
            .filter_map(move |source| {
                let marker = if let Some(root) = lock.root() {
                    let dependencies = match source {
                        Self::Production => root.dependencies(),
                        Self::Group(group) => {
                            root.resolved_dependency_groups().get(group)?.as_slice()
                        }
                    };
                    dependencies
                        .iter()
                        .filter_map(|dependency| {
                            (dependency.package_name() == name)
                                .then_some(dependency.pep508_marker())
                        })
                        .reduce(MarkerTree::or)?
                } else {
                    let Self::Group(group) = source else {
                        return None;
                    };
                    lock.dependency_groups()
                        .get(group)?
                        .iter()
                        .filter(|requirement| &requirement.name == name)
                        .map(|requirement| requirement.marker)
                        .reduce(MarkerTree::or)?
                };
                Some((source, marker))
            })
    }
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
    ) -> Result<(PythonEnvironment, Resolution)> {
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
        .await
        .map_err(UvError::from)?;
        Ok((PythonEnvironment::from(environment), resolution))
    }

    /// Use the script's containing workspace only to locate and install the checker.
    pub(super) async fn install_from_script_workspace(
        &self,
        script_dir: &Path,
        sys_platform: &str,
        name: &PackageName,
        workspace_cache: &WorkspaceCache,
        config_discovery: ConfigDiscovery,
        python_preference: PythonPreference,
        python_arch: Option<PythonArchitecture>,
        python_downloads: PythonDownloads,
        install_mirrors: &PythonInstallMirrors,
    ) -> Result<Option<PythonEnvironment>> {
        let project = match VirtualProject::discover(
            script_dir,
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
        // Only the declaring root contributes group-specific Python requirements. A virtual
        // root can contribute groups without selecting any of its members.
        let roots = match lock.root() {
            Some(root) => slice::from_ref(root.name()),
            None => &[],
        };
        let target =
            InstallTarget::from_project(&project, &lock, PackageSelection::Projects(roots));
        let mut interpreters = FxHashMap::default();
        let mut selected = None;
        let mut other_group = None;
        // The script and checker may use different Python versions, but run on the same OS.
        let platform = MarkerTree::expression(MarkerExpression::String {
            key: MarkerValueString::SysPlatform,
            operator: MarkerOperator::Equal,
            value: sys_platform.into(),
        });
        for (source, declaration) in WorkspaceToolSource::candidates(&lock, name) {
            if declaration.is_disjoint(platform) {
                continue;
            }
            let groups = match source {
                WorkspaceToolSource::Production => DependencyGroupsWithDefaults::none(),
                WorkspaceToolSource::Group(group) => DependencyGroups::from_group(group.clone())
                    .with_defaults(DefaultGroups::default()),
            };
            let requirement = target.python_requirement(&groups)?;
            let interpreter = match interpreters.entry(requirement.requires_python.clone()) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => {
                    let project_python = ProjectPythonRequest::from_requirements(
                        None,
                        Some(workspace.install_path()),
                        Some(requirement),
                        workspace.install_path(),
                        config_discovery,
                    )
                    .await?;
                    // An explicit workspace implementation can rule out the group before its
                    // Python requirement is enforced (e.g., a PyPy group in a CPython workspace).
                    if let Some(implementation) = project_python
                        .python_request
                        .as_ref()
                        .and_then(PythonDownloadRequest::from_request)
                        .and_then(|request| request.implementation().copied())
                        // Pyodide requests match the Emscripten platform.
                        && implementation != ImplementationName::Pyodide
                    {
                        let implementation = MarkerTree::expression(MarkerExpression::String {
                            key: MarkerValueString::ImplementationName,
                            operator: MarkerOperator::Equal,
                            value: implementation.long_name().into(),
                        });
                        if declaration.is_disjoint(platform.and(implementation)) {
                            continue;
                        }
                    }
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
                    entry.insert(CachedEnvironment::base_interpreter(
                        &interpreter,
                        self.cache,
                    )?)
                }
            };
            let selection = lock
                .dependency_selection(
                    target.project_name(),
                    name,
                    interpreter.to_resolver_marker_environment().markers(),
                )
                .map_err(anyhow::Error::msg)?;
            let dependency = match source {
                WorkspaceToolSource::Production => selection.production(),
                WorkspaceToolSource::Group(group) => selection.group(group),
            };
            let Some(dependency) = dependency else {
                continue;
            };
            selected = Some((dependency.clone(), interpreter.clone()));
            if let WorkspaceToolSource::Group(group) = source
                && group != &*DEV_DEPENDENCIES
            {
                if let Some(first) = other_group.replace(group) {
                    anyhow::bail!(
                        "Cannot select `{name}` from workspace dependency groups `{first}` and `{group}`. Use `TY` or `--ty-version` to select the checker."
                    );
                }
            } else {
                break;
            }
        }
        let Some((dependency, interpreter)) = selected else {
            return Ok(None);
        };
        target.validate_platform(&interpreter.to_resolver_marker_environment())?;
        let (environment, _) = self.install(target, &dependency, &interpreter).await?;
        Ok(Some(environment))
    }
}

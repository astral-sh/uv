use anyhow::Result;

use uv_configuration::{
    BuildOptions, DependencyGroupsWithDefaults, ExtrasSpecification, InstallOptions,
};
use uv_distribution_types::Resolution;
use uv_normalize::{DefaultExtras, GroupName, PackageName};
use uv_python::Interpreter;
use uv_resolver::{Lock, Package};
use uv_workspace::VirtualProject;

use crate::commands::pip::{resolution_markers, resolution_tags};

/// A locked package selected for use as a project tool.
pub(crate) struct LockedTool<'lock> {
    package: &'lock Package,
    requires_separate_environment: bool,
}

impl<'lock> LockedTool<'lock> {
    pub(crate) fn package(&self) -> &'lock Package {
        self.package
    }

    /// Returns `true` if the tool must be installed outside the selected project environment.
    pub(crate) fn requires_separate_environment(&self) -> bool {
        self.requires_separate_environment
    }
}

/// Find a tool in a dependency group, falling back to the current project's production
/// dependencies.
pub(crate) fn find_locked_tool<'lock>(
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
    let (package, installed) = if let Some(package) = selection.group(dependency_group) {
        (package, groups.contains(dependency_group))
    } else if let Some(package) = selection.production() {
        (package, groups.prod())
    } else {
        return Ok(None);
    };

    Ok(Some(LockedTool {
        package,
        requires_separate_environment: !installed,
    }))
}

/// Materialize the exact dependency subgraph rooted at a locked package.
pub(crate) fn resolution_from_lock(
    project: &VirtualProject,
    lock: &Lock,
    package: &Package,
    interpreter: &Interpreter,
    build_options: &BuildOptions,
) -> Result<Resolution> {
    let marker_environment = resolution_markers(None, None, interpreter);
    let tags = resolution_tags(None, None, interpreter)?;
    let extras = ExtrasSpecification::default().with_defaults(DefaultExtras::default());
    let groups = DependencyGroupsWithDefaults::none();
    Ok(lock.to_resolution(
        project.workspace().install_path(),
        [package],
        project.project_name(),
        &marker_environment,
        &tags,
        &extras,
        &groups,
        build_options,
        &InstallOptions::default(),
    )?)
}

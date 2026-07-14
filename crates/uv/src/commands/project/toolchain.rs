use anyhow::Result;

use uv_configuration::{BuildOptions, DependencyGroupsWithDefaults, InstallOptions};
use uv_distribution_types::Resolution;
use uv_normalize::{GroupName, PackageName};
use uv_python::Interpreter;
use uv_resolver::{Lock, SelectedDependency};
use uv_types::LockedBuildResolutions;
use uv_workspace::VirtualProject;

use crate::commands::pip::{resolution_markers, resolution_tags};

/// A locked package selected for use as a project tool.
pub(crate) struct LockedTool<'lock> {
    dependency: SelectedDependency<'lock>,
    requires_separate_environment: bool,
}

impl<'lock> LockedTool<'lock> {
    fn dependency(&self) -> &SelectedDependency<'lock> {
        &self.dependency
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

/// Materialize the exact dependency subgraph and locked build resolutions for a tool selection.
pub(crate) fn resolution_from_lock(
    project: &VirtualProject,
    lock: &Lock,
    tool: &LockedTool<'_>,
    interpreter: &Interpreter,
    build_options: &BuildOptions,
    require_locked_build_resolutions: bool,
) -> Result<(Resolution, LockedBuildResolutions)> {
    let marker_environment = resolution_markers(None, None, interpreter);
    let tags = resolution_tags(None, None, interpreter)?;
    let resolution = lock.to_resolution_from_dependency(
        project.workspace().install_path(),
        tool.dependency(),
        project.project_name(),
        &marker_environment,
        &tags,
        build_options,
        &InstallOptions::default(),
    )?;
    let locked_build_resolutions = lock.all_build_resolutions(
        &resolution,
        project.workspace().install_path(),
        tags.as_ref(),
        build_options,
        marker_environment.markers(),
        interpreter.markers(),
        require_locked_build_resolutions,
    )?;
    Ok((
        resolution,
        LockedBuildResolutions::new(locked_build_resolutions),
    ))
}

use anyhow::Result;

use uv_configuration::{
    BuildOptions, DependencyGroupsWithDefaults, ExtrasSpecification, InstallOptions,
};
use uv_distribution_types::Resolution;
use uv_normalize::{DEV_DEPENDENCIES, DefaultExtras, PackageName};
use uv_python::Interpreter;
use uv_resolver::{Lock, Package};
use uv_workspace::VirtualProject;

use crate::commands::pip::{resolution_markers, resolution_tags};

/// Select a concrete package reachable from the current project's development group.
pub(crate) fn package_from_lock<'lock>(
    project: &VirtualProject,
    lock: &'lock Lock,
    interpreter: &Interpreter,
    package_name: &PackageName,
) -> Result<Option<&'lock Package>> {
    let marker_environment = interpreter.resolver_marker_environment();
    lock.find_dependency_group_package(
        project.project_name(),
        &DEV_DEPENDENCIES,
        package_name,
        marker_environment.markers(),
    )
    .map_err(anyhow::Error::msg)
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

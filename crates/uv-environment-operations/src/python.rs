//! Adapt frozen lockfile requirements for Python discovery.

use std::path::Path;

use uv_configuration::DependencyGroupsWithDefaults;
use uv_distribution_types::RequiresPython;
use uv_lock::Installable;
use uv_python::{ConfigDiscovery, PythonRequest};
use uv_python_context::{ProjectPythonRequest, ProjectPythonRequirement, PythonRequirementSource};
use uv_workspace::{RequiresPythonDeclaration, RequiresPythonSources};

use crate::ProjectError;
use crate::install_target::InstallTarget;

/// Determine the Python request and requirement from a frozen lockfile.
pub async fn from_lockfile(
    python_request: Option<PythonRequest>,
    target: InstallTarget<'_>,
    groups: &DependencyGroupsWithDefaults,
    project_dir: &Path,
    config_discovery: ConfigDiscovery,
) -> Result<ProjectPythonRequest, ProjectError> {
    Ok(ProjectPythonRequest::from_requirements(
        python_request,
        Some(target.install_path()),
        Some(find_lockfile_requires_python(target, groups)?),
        project_dir,
        config_discovery,
    )
    .await?)
}

/// Intersect the lockfile's Python requirement with the selected groups' requirements.
fn find_lockfile_requires_python(
    target: InstallTarget<'_>,
    groups: &DependencyGroupsWithDefaults,
) -> Result<ProjectPythonRequirement, ProjectError> {
    let lock = target.lock();
    let mut group_requirements = RequiresPythonSources::new();

    if let Some(members) = lock.member_group_metadata() {
        let group_root = target.group_root(groups);

        for (member, member_groups) in members {
            // The group root can contribute groups without being an install root.
            let is_install_root = target.roots().any(|root| root == member);
            if !is_install_root && group_root != Some(member) {
                continue;
            }

            for (group, metadata) in member_groups {
                if target.includes_group(Some(member), group, groups)
                    && let Some(requires_python) = &metadata.requires_python
                {
                    group_requirements.insert(
                        RequiresPythonDeclaration::Member(member.clone(), Some(group.clone())),
                        requires_python.clone(),
                    );
                }
            }
        }
    }

    for (group, metadata) in lock.workspace_group_metadata() {
        if target.includes_group(None, group, groups)
            && let Some(requires_python) = &metadata.requires_python
        {
            group_requirements.insert(
                RequiresPythonDeclaration::Workspace(group.clone()),
                requires_python.clone(),
            );
        }
    }

    let Some(requires_python) = RequiresPython::intersection(
        std::iter::once(lock.requires_python().specifiers()).chain(group_requirements.values()),
    ) else {
        return Err(ProjectError::DisjointLockedRequiresPython {
            locked: lock.requires_python().clone(),
            groups: group_requirements,
        });
    };
    Ok(ProjectPythonRequirement {
        requires_python,
        source: PythonRequirementSource::Lockfile {
            locked: lock.requires_python().clone(),
            groups: group_requirements,
        },
    })
}

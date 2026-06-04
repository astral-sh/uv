use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use uv_normalize::PackageName;
use uv_pep440::{Operator, VersionSpecifier, VersionSpecifiers};
use uv_pep508::{Requirement, VersionOrUrl};
use uv_pypi_types::VerbatimParsedUrl;
use uv_workspace::pyproject::Source;
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache, WorkspaceError};

use crate::commands::ExitStatus;

pub(crate) async fn upgrade(
    project_dir: &Path,
    package: PackageName,
    workspace_cache: &WorkspaceCache,
) -> Result<ExitStatus> {
    let project =
        match VirtualProject::discover(project_dir, &DiscoveryOptions::default(), workspace_cache)
            .await
        {
            Ok(VirtualProject::Project(project)) => project,
            Ok(VirtualProject::NonProject(_))
            | Err(WorkspaceError::MissingPyprojectToml | WorkspaceError::MissingProject(_)) => {
                bail!("`uv upgrade` requires a project with a `[project]` table");
            }
            Err(err) => return Err(err.into()),
        };

    if project.workspace().packages().len() != 1 {
        bail!("`uv upgrade` does not support workspaces with multiple members yet");
    }

    let dependencies = project
        .current_project()
        .project()
        .dependencies
        .as_deref()
        .unwrap_or_default();
    let mut matching = Vec::new();
    for dependency in dependencies {
        let requirement =
            Requirement::<VerbatimParsedUrl>::from_str(dependency).with_context(|| {
                format!("Failed to parse dependency `{dependency}` in `project.dependencies`")
            })?;
        if requirement.name == package {
            matching.push(requirement);
        }
    }

    let requirement = match matching.as_slice() {
        [] => bail!("Dependency `{package}` was not found in `project.dependencies`"),
        [requirement] => requirement,
        _ => bail!("Dependency `{package}` is declared multiple times in `project.dependencies`"),
    };

    if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
        bail!("Dependency `{package}` is a direct URL requirement and cannot be upgraded");
    }

    let sources = project
        .current_project()
        .pyproject_toml()
        .tool
        .as_ref()
        .and_then(|tool| tool.uv.as_ref())
        .and_then(|uv| uv.sources.as_ref())
        .and_then(|sources| sources.inner().get(&package))
        .or_else(|| project.workspace().sources().get(&package));
    let extra = requirement.marker.top_level_extra_name();
    if sources.is_some_and(|sources| {
        sources.iter().any(|source| {
            source
                .extra()
                .is_none_or(|target| extra.as_deref() == Some(target))
                && source.group().is_none()
                && !source.marker().is_disjoint(requirement.marker)
                && !matches!(source, Source::Registry { .. })
        })
    }) {
        bail!(
            "Dependency `{package}` uses a non-registry source in `tool.uv.sources` and cannot be upgraded"
        );
    }

    let _relaxed_requirement = relax_requirement(requirement);

    // TODO: Resolve the relaxed requirement and update the manifest in the next slice.
    bail!("`uv upgrade` resolution is not implemented yet")
}

fn relax_requirement(
    requirement: &Requirement<VerbatimParsedUrl>,
) -> Requirement<VerbatimParsedUrl> {
    let mut relaxed = requirement.clone();
    let Some(VersionOrUrl::VersionSpecifier(specifiers)) = &requirement.version_or_url else {
        return relaxed;
    };

    let specifiers = specifiers
        .iter()
        .filter_map(|specifier| match specifier.operator() {
            Operator::GreaterThan
            | Operator::GreaterThanEqual
            | Operator::NotEqual
            | Operator::NotEqualStar => Some(specifier.clone()),
            Operator::TildeEqual => Some(VersionSpecifier::greater_than_equal_version(
                specifier.version().clone(),
            )),
            Operator::Equal
            | Operator::EqualStar
            | Operator::ExactEqual
            | Operator::LessThan
            | Operator::LessThanEqual => None,
        })
        .collect::<VersionSpecifiers>();

    relaxed.version_or_url = if specifiers.is_empty() {
        None
    } else {
        Some(VersionOrUrl::VersionSpecifier(specifiers))
    };
    relaxed
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_pep508::Requirement;
    use uv_pypi_types::VerbatimParsedUrl;

    use super::relax_requirement;

    #[test]
    fn relax_requirement_preserves_lower_bounds_and_exclusions() {
        let requirement = Requirement::<VerbatimParsedUrl>::from_str(
            "Requests>=1,>1.5,!=2,!=2.1.*,==2.5,===2.6,<=3,<4",
        )
        .expect("valid requirement");

        let relaxed = relax_requirement(&requirement);

        assert_eq!(relaxed.to_string(), "requests>=1,>1.5,!=2,!=2.1.*");
    }

    #[test]
    fn relax_requirement_converts_compatible_release_to_lower_bound() {
        let requirement = Requirement::<VerbatimParsedUrl>::from_str("requests~=2.32.1")
            .expect("valid requirement");

        let relaxed = relax_requirement(&requirement);

        assert_eq!(relaxed.to_string(), "requests>=2.32.1");
    }

    #[test]
    fn relax_requirement_removes_blocking_only_constraints() {
        for requirement in [
            "requests==2.32.1",
            "requests===2.32.1",
            "requests==2.32.*",
            "requests<3",
            "requests<=3",
        ] {
            let requirement =
                Requirement::<VerbatimParsedUrl>::from_str(requirement).expect("valid requirement");

            let relaxed = relax_requirement(&requirement);

            assert_eq!(relaxed.to_string(), "requests");
        }
    }

    #[test]
    fn relax_requirement_preserves_requirement_metadata() {
        let requirement = Requirement::<VerbatimParsedUrl>::from_str(
            "Requests_Plus[security,tests]~=2.32 ; python_version >= '3.12'",
        )
        .expect("valid requirement");

        let relaxed = relax_requirement(&requirement);

        assert_eq!(
            relaxed.to_string(),
            "requests-plus[security,tests]>=2.32 ; python_full_version >= '3.12'"
        );
    }
}

use std::fmt::Write;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use uv_cache::{Cache, Refresh};
use uv_client::BaseClientBuilder;
use uv_configuration::{Concurrency, DependencyGroupsWithDefaults, DryRun};
use uv_distribution::{ArchiveMetadata, Metadata};
use uv_distribution_types::Identifier;
use uv_normalize::PackageName;
use uv_pep440::{Operator, VersionSpecifier, VersionSpecifiers};
use uv_pep508::{Requirement, VerbatimUrl, VersionOrUrl};
use uv_preview::Preview;
use uv_pypi_types::{PyProjectToml, ResolutionMetadata, VerbatimParsedUrl};
use uv_python::{PythonDownloads, PythonPreference};
use uv_redacted::DisplaySafeUrl;
use uv_resolver::MetadataResponse;
use uv_settings::PythonInstallMirrors;
use uv_workspace::pyproject::Source;
use uv_workspace::pyproject_mut::{DependencyTarget, PyProjectTomlMut};
use uv_workspace::{DiscoveryOptions, VirtualProject, WorkspaceCache, WorkspaceErrorKind};

use crate::commands::pip::loggers::DefaultResolveLogger;
use crate::commands::project::lock::{LockEvent, LockMode, LockOperation, LockResult};
use crate::commands::project::lock_target::LockTarget;
use crate::commands::project::{ProjectError, ProjectInterpreter, UniversalState, WorkspacePython};
use crate::commands::{ExitStatus, diagnostics};
use crate::printer::Printer;
use crate::settings::ResolverSettings;

pub(crate) async fn upgrade(
    project_dir: &Path,
    package: PackageName,
    install_mirrors: PythonInstallMirrors,
    settings: ResolverSettings,
    client_builder: BaseClientBuilder<'_>,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    concurrency: Concurrency,
    no_config: bool,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    let project = match VirtualProject::discover(
        project_dir,
        &DiscoveryOptions::default(),
        cache,
        workspace_cache,
    )
    .await
    {
        Ok(VirtualProject::Project(project)) => project,
        Ok(VirtualProject::NonProject(_)) => {
            bail!("`uv upgrade` requires a project with a `[project]` table")
        }
        Err(err)
            if matches!(
                err.as_ref(),
                WorkspaceErrorKind::MissingPyprojectToml | WorkspaceErrorKind::MissingProject(_)
            ) =>
        {
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

    if &requirement.name == project.project_name() {
        bail!("Dependency `{package}` refers to the current project and cannot be upgraded");
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

    let relaxed_requirement = relax_requirement(requirement);
    let Requirement {
        name,
        extras,
        version_or_url,
        marker,
        origin,
    } = relaxed_requirement;
    let version_or_url = match version_or_url {
        Some(VersionOrUrl::VersionSpecifier(specifiers)) => {
            Some(VersionOrUrl::VersionSpecifier(specifiers))
        }
        Some(VersionOrUrl::Url(_)) => {
            bail!("Dependency `{package}` is a direct URL requirement and cannot be upgraded");
        }
        None => None,
    };
    let relaxed_requirement = Requirement::<VerbatimUrl> {
        name,
        extras,
        version_or_url,
        marker,
        origin,
    };

    let mut pyproject = PyProjectTomlMut::from_toml(
        &project.current_project().pyproject_toml().raw,
        DependencyTarget::PyProjectToml,
    )?;
    if pyproject
        .replace_dependency(&relaxed_requirement, false)?
        .is_none()
    {
        bail!("Dependency `{package}` was not found in `project.dependencies`");
    }
    let pyproject = pyproject.to_string();
    let pyproject = PyProjectToml::from_toml(
        &pyproject,
        project.project_root().join("pyproject.toml").display(),
    )?;
    if pyproject
        .project
        .as_ref()
        .is_some_and(|project| project.version.is_none())
    {
        // TODO: Support dynamic project metadata by building the project before resolution.
        bail!("`uv upgrade` does not support projects with dynamic versions yet");
    }
    let metadata = ResolutionMetadata::parse_pyproject_toml(pyproject, None)?;
    let metadata = Metadata::from_workspace(
        metadata,
        project.project_root(),
        None,
        &settings.index_locations,
        settings.sources.clone(),
        true,
        cache,
        workspace_cache,
        client_builder.credentials_cache(),
    )
    .await?;

    let groups = DependencyGroupsWithDefaults::none();
    let workspace_python = WorkspacePython::from_request(
        None,
        Some(project.workspace()),
        &groups,
        project_dir,
        no_config,
    )
    .await?;
    let interpreter = ProjectInterpreter::discover(
        project.workspace(),
        &groups,
        workspace_python,
        &client_builder,
        python_preference,
        python_downloads,
        &install_mirrors,
        false,
        Some(false),
        cache,
        printer,
    )
    .await?
    .into_interpreter();

    let state = UniversalState::default();
    let distribution_id = DisplaySafeUrl::from_file_path(project.project_root())
        .map_err(|()| anyhow!("Project root is not a valid file URL"))?
        .distribution_id();
    state.index().distributions().done(
        distribution_id,
        Arc::new(MetadataResponse::Found(ArchiveMetadata::from(metadata))),
    );

    let refresh = Refresh::from(settings.upgrade.clone());
    let result = match Box::pin(
        LockOperation::new(
            LockMode::DryRun(&interpreter),
            &settings,
            &client_builder,
            &state,
            Box::new(DefaultResolveLogger),
            &concurrency,
            cache,
            workspace_cache,
            printer,
            preview,
        )
        .with_refresh(&refresh)
        .execute(LockTarget::Workspace(project.workspace())),
    )
    .await
    {
        Ok(result) => result,
        Err(ProjectError::Operation(err)) => {
            return diagnostics::OperationDiagnostic::with_system_certs(
                client_builder.system_certs(),
            )
            .report(err)
            .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
        }
        Err(err) => return Err(err.into()),
    };

    let event = match &result {
        LockResult::Changed(previous, lock) => {
            LockEvent::detect_changes(previous.as_ref(), lock, DryRun::Enabled)
                .find(|event| event.package() == &package)
        }
        LockResult::Unchanged(_) => None,
    };
    if let Some(event) = event {
        writeln!(printer.stderr(), "{event}")?;
    } else {
        writeln!(printer.stderr(), "No version change for {package}")?;
    }

    Ok(ExitStatus::Success)
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

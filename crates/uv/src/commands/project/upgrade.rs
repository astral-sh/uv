use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use uv_cache::{Cache, Refresh};
use uv_client::BaseClientBuilder;
use uv_configuration::{Concurrency, DependencyGroupsWithDefaults, DryRun};
use uv_distribution::{ArchiveMetadata, GitWorkspaceSourceContext, Metadata};
use uv_distribution_types::Identifier;
use uv_normalize::PackageName;
use uv_pep440::{Operator, Version, VersionSpecifier, VersionSpecifiers};
use uv_pep508::{MarkerTree, Requirement, VerbatimUrl, VersionOrUrl};
use uv_preview::Preview;
use uv_pypi_types::{PyProjectToml, ResolutionMetadata, VerbatimParsedUrl};
use uv_python::{ConfigDiscovery, PythonDownloads, PythonPreference};
use uv_redacted::DisplaySafeUrl;
use uv_resolver::MetadataResponse;
use uv_settings::PythonInstallMirrors;
use uv_workspace::pyproject::Source;
use uv_workspace::pyproject_mut::{DependencyTarget, PyProjectTomlMut};
use uv_workspace::{
    DiscoveryOptions, ProjectWorkspace, VirtualProject, WorkspaceCache, WorkspaceErrorKind,
};

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
    config_discovery: ConfigDiscovery,
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
    let (requirement_text, requirement) = select_requirement(&project, &package)?;

    let relaxed_requirement =
        into_verbatim_requirement(relax_requirement(requirement.clone()), &package)?;

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
    let state = UniversalState::default();
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
        &GitWorkspaceSourceContext::new(state.git(), |url| client_builder.git_http_settings(url)),
    )
    .await?;

    let groups = DependencyGroupsWithDefaults::none();
    let workspace_python = WorkspacePython::from_request(
        None,
        Some(project.workspace()),
        &groups,
        project_dir,
        config_discovery,
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
            return diagnostics::OperationDiagnostic::default()
                .report(err)
                .map_or(Ok(ExitStatus::Failure), |err| Err(err.into()));
        }
        Err(err) => return Err(err.into()),
    };

    let mut resolved_versions = BTreeSet::new();
    for resolved_package in result.lock().packages() {
        // A universal lock can contain versions from disjoint forks, so only collect versions
        // from forks where the selected requirement applies.
        if resolved_package.name() == &package
            && resolved_package
                .index(project.workspace().install_path())?
                .is_some()
            && resolved_package.is_included_by_marker(requirement.marker)
            && let Some(version) = resolved_package.version()
        {
            resolved_versions.insert(version.clone());
        }
    }
    let proposed_requirement = propose_requirement(&requirement, &resolved_versions)?;
    let updated_requirement = if proposed_requirement == requirement {
        None
    } else {
        let proposed_requirement = into_verbatim_requirement(proposed_requirement, &package)?;
        let proposed_text = proposed_requirement.to_string();
        let mut pyproject = PyProjectTomlMut::from_toml(
            &project.current_project().pyproject_toml().raw,
            DependencyTarget::PyProjectToml,
        )?;
        if pyproject
            .replace_dependency(&proposed_requirement, false)?
            .is_none()
        {
            bail!("Dependency `{package}` was not found in `project.dependencies`");
        }
        let pyproject_path = project.project_root().join("pyproject.toml");
        fs_err::write(pyproject_path, pyproject.to_string())?;
        Some(proposed_text)
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
    if let Some(proposed_text) = updated_requirement {
        writeln!(
            printer.stderr(),
            "Updated requirement: `{requirement_text}` -> `{proposed_text}`"
        )?;
    }

    Ok(ExitStatus::Success)
}

/// Select the single production dependency declaration targeted by `uv upgrade`.
fn select_requirement(
    project: &ProjectWorkspace,
    package: &PackageName,
) -> Result<(String, Requirement<VerbatimParsedUrl>)> {
    if project.workspace().packages().len() != 1 {
        bail!("`uv upgrade` does not support workspaces with multiple members yet");
    }

    let dependencies = project
        .current_project()
        .project()
        .dependencies
        .as_deref()
        .unwrap_or_default();
    let pyproject_path = project.project_root().join("pyproject.toml");
    let mut matching = Vec::new();
    for dependency in dependencies {
        let requirement =
            Requirement::<VerbatimParsedUrl>::from_str(dependency).with_context(|| {
                format!(
                    "Failed to parse dependency `{dependency}` from `project.dependencies` in `{}`",
                    pyproject_path.display()
                )
            })?;
        if requirement.name == *package {
            matching.push((dependency.clone(), requirement));
        }
    }

    let (requirement_text, requirement) = match matching.as_slice() {
        [] => bail!("Dependency `{package}` was not found in `project.dependencies`"),
        [(requirement_text, requirement)] => (requirement_text.clone(), requirement.clone()),
        _ => bail!("Dependency `{package}` is declared multiple times in `project.dependencies`"),
    };

    if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
        bail!("Dependency `{package}` is a direct URL requirement and cannot be upgraded");
    }

    if requirement.name == *project.project_name() {
        bail!("Dependency `{package}` refers to the current project and cannot be upgraded");
    }

    let sources = project
        .current_project()
        .pyproject_toml()
        .tool
        .as_ref()
        .and_then(|tool| tool.uv.as_ref())
        .and_then(|uv| uv.sources.as_ref())
        .and_then(|sources| sources.inner().get(package))
        .or_else(|| project.workspace().sources().get(package));
    if sources.is_some_and(|sources| {
        sources.iter().any(|source| {
            source_is_applicable(source, requirement.marker)
                && matches!(source, Source::Git { rev: Some(_), .. })
        })
    }) {
        bail!(
            "Dependency `{package}` is pinned to a Git revision and cannot be upgraded commit-to-commit"
        );
    }
    if sources.is_some_and(|sources| {
        sources.iter().any(|source| {
            source_is_applicable(source, requirement.marker)
                && !matches!(source, Source::Registry { .. })
        })
    }) {
        bail!(
            "Dependency `{package}` uses a non-registry source in `tool.uv.sources` and cannot be upgraded"
        );
    }

    Ok((requirement_text, requirement))
}

/// Return whether a source applies to the selected requirement declaration.
fn source_is_applicable(source: &Source, requirement_marker: MarkerTree) -> bool {
    let extra = requirement_marker.top_level_extra_name();
    source
        .extra()
        .is_none_or(|target| extra.as_deref() == Some(target))
        && source.group().is_none()
        && !source.marker().is_disjoint(requirement_marker)
}

/// Convert a parsed requirement into the representation used by the mutable manifest.
fn into_verbatim_requirement(
    requirement: Requirement<VerbatimParsedUrl>,
    package: &PackageName,
) -> Result<Requirement<VerbatimUrl>> {
    let Requirement {
        name,
        extras,
        version_or_url,
        marker,
        origin,
    } = requirement;
    let version_or_url = match version_or_url {
        Some(VersionOrUrl::VersionSpecifier(specifiers)) => {
            Some(VersionOrUrl::VersionSpecifier(specifiers))
        }
        Some(VersionOrUrl::Url(_)) => {
            bail!("Dependency `{package}` is a direct URL requirement and cannot be upgraded");
        }
        None => None,
    };
    Ok(Requirement::<VerbatimUrl> {
        name,
        extras,
        version_or_url,
        marker,
        origin,
    })
}

/// Return a requirement that admits every applicable resolved version.
///
/// For example, `foo>=1,<2` resolving to `2.4` becomes `foo>=1,<3`. Preserve
/// [`VersionSpecifier`]s that already admit the resolution, and rewrite only the specifiers that
/// exclude it. If a requirement resolves to multiple versions, rewrite each specifier using the
/// appropriate version boundary for its operator, then verify that the result admits every
/// resolved version.
fn propose_requirement(
    requirement: &Requirement<VerbatimParsedUrl>,
    resolved_versions: &BTreeSet<Version>,
) -> Result<Requirement<VerbatimParsedUrl>> {
    if resolved_versions.is_empty() {
        return Ok(requirement.clone());
    }

    let Some(VersionOrUrl::VersionSpecifier(specifiers)) = &requirement.version_or_url else {
        return Ok(requirement.clone());
    };
    if resolved_versions
        .iter()
        .all(|version| specifiers.contains(version))
    {
        return Ok(requirement.clone());
    }
    let Some(highest_resolved_version) = resolved_versions.last() else {
        return Ok(requirement.clone());
    };

    let specifiers = specifiers
        .iter()
        .cloned()
        .map(|specifier| rewrite_specifier(specifier, resolved_versions))
        .collect::<Result<VersionSpecifiers>>()?;
    if !resolved_versions
        .iter()
        .all(|version| specifiers.contains(version))
    {
        tracing::debug!(
            dependency = %requirement.name,
            resolved_versions = ?resolved_versions,
            rewritten_specifiers = %specifiers,
            "Rewritten dependency constraint does not admit every resolved version"
        );
        if resolved_versions.len() == 1 {
            bail!(
                "Dependency `{}` resolved to version `{highest_resolved_version}` which cannot be represented by the upgraded requirement; this is not supported yet",
                requirement.name
            );
        }
        let resolved_versions = resolved_versions
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("`, `");
        bail!(
            "Dependency `{}` resolved to versions `{resolved_versions}` which cannot be represented by the upgraded requirement; this is not supported yet",
            requirement.name
        );
    }
    let mut proposed = requirement.clone();
    proposed.version_or_url = Some(VersionOrUrl::VersionSpecifier(specifiers));
    Ok(proposed)
}

/// Attempt to rewrite a [`VersionSpecifier`] to admit all resolved versions while preserving its
/// operator.
fn rewrite_specifier(
    specifier: VersionSpecifier,
    resolved_versions: &BTreeSet<Version>,
) -> Result<VersionSpecifier> {
    if resolved_versions
        .iter()
        .all(|version| specifier.contains(version))
    {
        return Ok(specifier);
    }
    let (Some(lowest_resolved_version), Some(highest_resolved_version)) =
        (resolved_versions.first(), resolved_versions.last())
    else {
        return Ok(specifier);
    };

    Ok(match specifier.operator() {
        Operator::GreaterThan
        | Operator::GreaterThanEqual
        | Operator::NotEqual
        | Operator::NotEqualStar => specifier,
        Operator::TildeEqual => VersionSpecifier::from_version(
            Operator::TildeEqual,
            compatible_version_at_precision(
                lowest_resolved_version,
                specifier.version().release().len(),
            )?,
        )?,
        Operator::Equal => VersionSpecifier::equals_version(lowest_resolved_version.clone()),
        Operator::EqualStar => VersionSpecifier::equals_star_version(
            lowest_resolved_version
                .only_release_at_precision(specifier.version().release().len())
                .context("Cannot rewrite a version constraint without a release segment")?,
        ),
        Operator::ExactEqual => {
            VersionSpecifier::from_version(Operator::ExactEqual, lowest_resolved_version.clone())?
        }
        Operator::LessThan => VersionSpecifier::less_than_version(increment_version_at_precision(
            highest_resolved_version,
            specifier.version().release().len(),
        )?),
        Operator::LessThanEqual => VersionSpecifier::from_version(
            Operator::LessThanEqual,
            highest_resolved_version.clone().without_local(),
        )?,
    })
}

/// Project a version to the given precision while preserving its compatible-release suffixes.
fn compatible_version_at_precision(version: &Version, precision: usize) -> Result<Version> {
    let release = version
        .release()
        .iter()
        .copied()
        .chain(std::iter::repeat(0))
        .take(precision)
        .collect::<Vec<_>>();
    if release.is_empty() {
        bail!("Cannot rewrite a version constraint without a release segment");
    }
    Ok(version.clone().with_release(release).without_local())
}

/// Increment the last release segment after projecting a version to the given precision.
fn increment_version_at_precision(version: &Version, precision: usize) -> Result<Version> {
    let projected = version
        .only_release_at_precision(precision)
        .context("Cannot rewrite a version constraint without a release segment")?;
    let mut release = projected.release().to_vec();
    let segment_index = release.len();
    let Some(last) = release.last_mut() else {
        bail!("Cannot rewrite a version constraint without a release segment");
    };
    let segment = *last;
    *last = segment.checked_add(1).with_context(|| {
        format!(
            "Cannot expand version `{version}` at release segment {segment_index} (`{segment}`) beyond its maximum value"
        )
    })?;
    Ok(projected.with_release(release))
}

/// Remove upper and exact constraints while retaining lower bounds and exclusions.
fn relax_requirement(
    mut requirement: Requirement<VerbatimParsedUrl>,
) -> Requirement<VerbatimParsedUrl> {
    let Some(VersionOrUrl::VersionSpecifier(specifiers)) = &requirement.version_or_url else {
        return requirement;
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

    requirement.version_or_url = if specifiers.is_empty() {
        None
    } else {
        Some(VersionOrUrl::VersionSpecifier(specifiers))
    };
    requirement
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::str::FromStr;

    use uv_pep440::Version;
    use uv_pep508::Requirement;
    use uv_pypi_types::VerbatimParsedUrl;

    use super::{increment_version_at_precision, propose_requirement, relax_requirement};

    fn resolved_versions(versions: &[&str]) -> BTreeSet<Version> {
        versions
            .iter()
            .map(|version| Version::from_str(version).expect("valid version"))
            .collect()
    }

    #[test]
    fn propose_requirement_preserves_satisfied_constraints() {
        for requirement in ["requests", "requests>=1.2", "requests!=2.3"] {
            let requirement =
                Requirement::<VerbatimParsedUrl>::from_str(requirement).expect("valid requirement");

            let proposed = propose_requirement(&requirement, &resolved_versions(&["2.4.0"]))
                .expect("requirement can be proposed");

            assert_eq!(proposed, requirement);
        }
    }

    #[test]
    fn propose_requirement_expands_exclusive_upper_bounds_at_existing_precision() {
        for (requirement, version, expected) in [
            ("requests>=1.2,<2", "2.4.0", "requests>=1.2,<3"),
            ("requests>=1.2,<1.3", "1.4.2", "requests>=1.2,<1.5"),
        ] {
            let requirement =
                Requirement::<VerbatimParsedUrl>::from_str(requirement).expect("valid requirement");

            let proposed = propose_requirement(&requirement, &resolved_versions(&[version]))
                .expect("requirement can be proposed");

            assert_eq!(proposed.to_string(), expected);
        }
    }

    #[test]
    fn propose_requirement_only_rewrites_blocking_specifiers() {
        let requirement = Requirement::<VerbatimParsedUrl>::from_str("requests>=1,<2,<4")
            .expect("valid requirement");

        let proposed = propose_requirement(&requirement, &resolved_versions(&["2.4.0"]))
            .expect("requirement can be proposed");

        assert_eq!(proposed.to_string(), "requests>=1,<3,<4");
    }

    #[test]
    fn propose_requirement_preserves_operator_style() {
        for (requirement, version, expected) in [
            ("requests==1.2.3", "2.4.5", "requests==2.4.5"),
            ("requests===1.2.3", "2.4.5", "requests===2.4.5"),
            ("requests==1.2.*", "2.4.5", "requests==2.4.*"),
            ("requests~=1.2", "2.4.5", "requests~=2.4"),
            ("requests~=1.2.3", "2.4.5", "requests~=2.4.5"),
            ("requests<=1.2.3", "2.4.5", "requests<=2.4.5"),
        ] {
            let requirement =
                Requirement::<VerbatimParsedUrl>::from_str(requirement).expect("valid requirement");

            let proposed = propose_requirement(&requirement, &resolved_versions(&[version]))
                .expect("requirement can be proposed");

            assert_eq!(proposed.to_string(), expected);
        }
    }

    #[test]
    fn propose_requirement_preserves_compatible_release_suffixes() {
        let requirement =
            Requirement::<VerbatimParsedUrl>::from_str("requests~=1.2").expect("valid requirement");

        let proposed = propose_requirement(
            &requirement,
            &resolved_versions(&["1!2.4rc1.post2.dev3+local"]),
        )
        .expect("requirement can be proposed");

        assert_eq!(proposed.to_string(), "requests~=1!2.4rc1.post2.dev3");
    }

    #[test]
    fn propose_requirement_strips_local_version_from_inclusive_upper_bound() {
        let requirement = Requirement::<VerbatimParsedUrl>::from_str("requests<=1.2.3")
            .expect("valid requirement");

        let proposed = propose_requirement(&requirement, &resolved_versions(&["2.4.5+local"]))
            .expect("requirement can be proposed");

        assert_eq!(proposed.to_string(), "requests<=2.4.5");
    }

    #[test]
    fn propose_requirement_rejects_constraint_that_still_excludes_resolved_version() {
        let requirement = Requirement::<VerbatimParsedUrl>::from_str("requests!=2.4,<2")
            .expect("valid requirement");

        let error = propose_requirement(&requirement, &resolved_versions(&["2.4"]))
            .expect_err("rewritten requirement must admit the resolved version");

        assert_eq!(
            error.to_string(),
            "Dependency `requests` resolved to version `2.4` which cannot be represented by the upgraded requirement; this is not supported yet"
        );
    }

    #[test]
    fn propose_requirement_preserves_metadata_lower_bounds_and_exclusions() {
        let requirement = Requirement::<VerbatimParsedUrl>::from_str(
            "Requests_Plus[security,tests]>=1.2,!=2.3,<2 ; python_version >= '3.12'",
        )
        .expect("valid requirement");

        let proposed = propose_requirement(&requirement, &resolved_versions(&["2.4.0"]))
            .expect("requirement can be proposed");

        assert_eq!(
            proposed.to_string(),
            "requests-plus[security,tests]>=1.2,!=2.3,<3 ; python_full_version >= '3.12'"
        );
    }

    #[test]
    fn propose_requirement_expands_upper_bound_for_multiple_versions() {
        let requirement =
            Requirement::<VerbatimParsedUrl>::from_str("requests<2").expect("valid requirement");

        let proposed = propose_requirement(&requirement, &resolved_versions(&["1.5.0", "2.4.0"]))
            .expect("upper bound can admit both versions");

        assert_eq!(proposed.to_string(), "requests<3");
    }

    #[test]
    fn propose_requirement_uses_lowest_compatible_version_for_multiple_versions() {
        let requirement =
            Requirement::<VerbatimParsedUrl>::from_str("requests~=1.2").expect("valid requirement");

        let proposed = propose_requirement(&requirement, &resolved_versions(&["2.4", "2.5"]))
            .expect("compatible release can admit both versions");

        assert_eq!(proposed.to_string(), "requests~=2.4");
    }

    #[test]
    fn propose_requirement_rejects_unrepresentable_multiple_versions() {
        let requirement =
            Requirement::<VerbatimParsedUrl>::from_str("requests==1.*").expect("valid requirement");

        let error = propose_requirement(&requirement, &resolved_versions(&["1.5.0", "2.4.0"]))
            .expect_err("wildcard cannot admit versions from different major lines");

        assert_eq!(
            error.to_string(),
            "Dependency `requests` resolved to versions `1.5.0`, `2.4.0` which cannot be represented by the upgraded requirement; this is not supported yet"
        );
    }

    #[test]
    fn increment_version_at_precision_reports_upper_bound_overflow() {
        let version = Version::new([1, 2, u64::MAX]);

        let error = increment_version_at_precision(&version, 3)
            .expect_err("maximum release segment cannot be incremented");

        assert_eq!(
            error.to_string(),
            "Cannot expand version `1.2.18446744073709551615` at release segment 3 (`18446744073709551615`) beyond its maximum value"
        );
    }

    #[test]
    fn relax_requirement_preserves_lower_bounds_and_exclusions() {
        let requirement = Requirement::<VerbatimParsedUrl>::from_str(
            "Requests>=1,>1.5,!=2,!=2.1.*,==2.5,===2.6,<=3,<4",
        )
        .expect("valid requirement");

        let relaxed = relax_requirement(requirement);

        assert_eq!(relaxed.to_string(), "requests>=1,>1.5,!=2,!=2.1.*");
    }

    #[test]
    fn relax_requirement_converts_compatible_release_to_lower_bound() {
        let requirement = Requirement::<VerbatimParsedUrl>::from_str("requests~=2.32.1")
            .expect("valid requirement");

        let relaxed = relax_requirement(requirement);

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

            let relaxed = relax_requirement(requirement);

            assert_eq!(relaxed.to_string(), "requests");
        }
    }

    #[test]
    fn relax_requirement_preserves_requirement_metadata() {
        let requirement = Requirement::<VerbatimParsedUrl>::from_str(
            "Requests_Plus[security,tests]~=2.32 ; python_version >= '3.12'",
        )
        .expect("valid requirement");

        let relaxed = relax_requirement(requirement);

        assert_eq!(
            relaxed.to_string(),
            "requests-plus[security,tests]>=2.32 ; python_full_version >= '3.12'"
        );
    }
}

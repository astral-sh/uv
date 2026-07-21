use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use itertools::Itertools;
use uv_cache::{Cache, Refresh};
use uv_client::BaseClientBuilder;
use uv_configuration::{Concurrency, DependencyGroupsWithDefaults, DryRun, Upgrade};
use uv_distribution::{ArchiveMetadata, Metadata};
use uv_distribution_types::{Identifier, RequiresPython};
use uv_normalize::PackageName;
use uv_pep440::{Operator, Version, VersionSpecifier, VersionSpecifiers};
use uv_pep508::{MarkerTree, Pep508ErrorSource, Requirement, VerbatimUrl, VersionOrUrl};
use uv_preview::Preview;
use uv_pypi_types::{PyProjectToml, ResolutionMetadata, SupportedEnvironments, VerbatimParsedUrl};
use uv_python::{ConfigDiscovery, Interpreter, PythonDownloads, PythonPreference};
use uv_redacted::DisplaySafeUrl;
use uv_resolver::{MetadataResponse, implicit_constraints_marker};
use uv_settings::PythonInstallMirrors;
use uv_workspace::pyproject::{DependencyType, Source};
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

/// A dependency requirement selected for upgrading.
struct UpgradableRequirement {
    package: PackageName,
    dependency_type: DependencyType,
    original_text: String,
    requirement: Requirement<VerbatimParsedUrl>,
    effective_marker: MarkerTree,
    resolved_versions: BTreeSet<Version>,
}

/// A selected requirement that cannot apply to the project.
struct SkippedRequirement {
    package: PackageName,
    dependency_type: DependencyType,
    original_text: String,
    display_text: String,
    reason: SkippedReason,
}

enum SkippedReason {
    EnvironmentOrPythonRequirement,
    UndefinedExtra,
}

/// The requirements selected for upgrade and those that cannot apply.
struct DeclarationSelection {
    active: Vec<UpgradableRequirement>,
    skipped: Vec<SkippedRequirement>,
    packages: Vec<PackageName>,
}

#[derive(Clone)]
struct DeclarationIdentity {
    package: PackageName,
    dependency_type: DependencyType,
    text: String,
}

/// An existing requirement and its proposed replacement.
#[derive(Clone)]
struct RequirementUpdate {
    package: PackageName,
    dependency_type: DependencyType,
    original_text: String,
    existing: Requirement<VerbatimUrl>,
    replacement: Requirement<VerbatimUrl>,
}

enum DeclarationOutcome {
    Changed(Box<RequirementUpdate>),
    Unchanged(DeclarationIdentity),
    Blocked {
        declaration: DeclarationIdentity,
        reason: BlockedReason,
    },
}

enum BlockedReason {
    UnrepresentableRequirement(ProposeRequirementError),
    ConflictExtra { extra: String },
}

#[derive(Debug, thiserror::Error)]
enum ProposeRequirementError {
    #[error("Dependency `{package}` resolved to {} `{}` which cannot be represented by the upgraded requirement; this is not supported yet", if resolved_versions.len() == 1 { "version" } else { "versions" }, resolved_versions.iter().join("`, `"))]
    Unrepresentable {
        package: PackageName,
        resolved_versions: BTreeSet<Version>,
    },
    #[error(transparent)]
    Rewrite(#[from] anyhow::Error),
}

impl DeclarationOutcome {
    fn package(&self) -> &PackageName {
        match self {
            Self::Changed(update) => &update.package,
            Self::Unchanged(declaration) | Self::Blocked { declaration, .. } => {
                &declaration.package
            }
        }
    }

    fn supports_lock_event(&self) -> bool {
        matches!(self, Self::Changed(_) | Self::Unchanged(_))
    }

    fn invalidates_relaxed_solve(&self) -> bool {
        matches!(
            self,
            Self::Blocked {
                reason: BlockedReason::UnrepresentableRequirement(_),
                ..
            }
        )
    }
}

impl BlockedReason {
    fn message(&self) -> String {
        match self {
            Self::UnrepresentableRequirement(error) => error.to_string(),
            Self::ConflictExtra { extra } => {
                format!("declared under conflicting extra `{extra}`")
            }
        }
    }
}

impl UpgradableRequirement {
    fn identity(&self) -> DeclarationIdentity {
        DeclarationIdentity {
            package: self.package.clone(),
            dependency_type: self.dependency_type.clone(),
            text: self.original_text.clone(),
        }
    }
}

pub(crate) async fn upgrade(
    project_dir: &Path,
    packages: Vec<PackageName>,
    exclude: Vec<PackageName>,
    install_mirrors: PythonInstallMirrors,
    mut settings: ResolverSettings,
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
    // Locking defaults a missing `requires-python` to the discovered interpreter's minor version.
    // Use that same bound when deciding whether selected declarations and sources can apply.
    let fallback_interpreter = if requires_fallback_interpreter(&project, &packages, &exclude)? {
        let selection = select_requirements(&project, &packages, &exclude, None)?;
        if selection.active.is_empty() {
            render_skipped_requirements(&selection.skipped, printer)?;
            return Ok(ExitStatus::Success);
        }

        let groups = DependencyGroupsWithDefaults::none();
        let workspace_python = WorkspacePython::from_request(
            None,
            Some(project.workspace()),
            &groups,
            project_dir,
            config_discovery,
        )
        .await?;
        match ProjectInterpreter::discover(
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
        .await
        {
            Ok(interpreter) => Some(interpreter.into_interpreter()),
            Err(error) => {
                let DeclarationSelection {
                    active,
                    skipped,
                    packages,
                } = select_requirements(&project, &packages, &exclude, None)?;
                render_skipped_requirements(&skipped, printer)?;
                validate_requirements(&project, &active, &packages)?;
                return Err(error.into());
            }
        }
    } else {
        None
    };
    let DeclarationSelection {
        active: requirements,
        skipped,
        packages: mut selected_packages,
    } = select_requirements(&project, &packages, &exclude, fallback_interpreter.as_ref())?;
    render_skipped_requirements(&skipped, printer)?;
    validate_requirements(&project, &requirements, &selected_packages)?;
    let mut declaration_outcomes = Vec::new();
    let conflicts = project.workspace().conflicts()?;
    let mut requirements = requirements
        .into_iter()
        .filter_map(|selected| {
            let Some(extra) = selected.requirement.marker.top_level_extra_name() else {
                return Some(selected);
            };
            if conflicts.contains(project.project_name(), extra.as_ref()) {
                declaration_outcomes.push(DeclarationOutcome::Blocked {
                    declaration: selected.identity(),
                    reason: BlockedReason::ConflictExtra {
                        extra: extra.to_string(),
                    },
                });
                None
            } else {
                Some(selected)
            }
        })
        .collect::<Vec<_>>();
    selected_packages.retain(|package| {
        requirements
            .iter()
            .any(|selected| selected.package == *package)
    });
    if requirements.is_empty() {
        render_declaration_outcomes(&declaration_outcomes, printer)?;
        return Ok(ExitStatus::Success);
    }
    settings.upgrade = Upgrade::from_packages(selected_packages.clone());
    let refresh = Refresh::from(settings.upgrade.clone());
    let cache = cache.clone().with_refresh(refresh.clone());

    // Relax the selected requirements in a temporary manifest so the resolver can select newer
    // versions without changing the user's manifest.
    let relaxed_requirements = requirements
        .iter()
        .map(|selected| {
            Ok(RequirementUpdate {
                package: selected.package.clone(),
                dependency_type: selected.dependency_type.clone(),
                original_text: selected.original_text.clone(),
                existing: into_verbatim_requirement(
                    selected.requirement.clone(),
                    &selected.package,
                )?,
                replacement: into_verbatim_requirement(
                    relax_requirement(selected.requirement.clone()),
                    &selected.package,
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let mut pyproject = PyProjectTomlMut::from_toml(
        &project.current_project().pyproject_toml().raw,
        DependencyTarget::PyProjectToml,
    )?;
    // Remove unreachable requirements from the temporary manifest, since URL requirements can be
    // fetched during metadata preparation even when their markers cannot apply.
    for (index, requirement) in skipped.iter().enumerate() {
        if skipped[..index].iter().any(|removed| {
            removed.dependency_type == requirement.dependency_type
                && removed.original_text == requirement.original_text
        }) {
            continue;
        }
        if pyproject
            .remove_dependency_declaration_text(
                &requirement.dependency_type,
                &requirement.original_text,
            )?
            .is_empty()
        {
            bail!(
                "Dependency `{}` was not found in {}",
                requirement.package,
                requirement.dependency_type.toml_table_name()
            );
        }
    }
    apply_requirement_replacements(&mut pyproject, relaxed_requirements.iter())?;
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
        &cache,
        workspace_cache,
        client_builder.credentials_cache(),
    )
    .await?;

    let interpreter = if let Some(interpreter) = fallback_interpreter {
        interpreter
    } else {
        let groups = DependencyGroupsWithDefaults::none();
        let workspace_python = WorkspacePython::from_request(
            None,
            Some(project.workspace()),
            &groups,
            project_dir,
            config_discovery,
        )
        .await?;
        ProjectInterpreter::discover(
            project.workspace(),
            &groups,
            workspace_python,
            &client_builder,
            python_preference,
            python_downloads,
            &install_mirrors,
            false,
            Some(false),
            &cache,
            printer,
        )
        .await?
        .into_interpreter()
    };

    let state = UniversalState::default();
    let distribution_id = DisplaySafeUrl::from_file_path(project.project_root())
        .map_err(|()| anyhow!("Project root is not a valid file URL"))?
        .distribution_id();
    state.index().distributions().done(
        distribution_id,
        Arc::new(MetadataResponse::Found(ArchiveMetadata::from(metadata))),
    );

    let result = match Box::pin(
        LockOperation::new(
            LockMode::DryRun(&interpreter),
            &settings,
            &client_builder,
            &state,
            Box::new(DefaultResolveLogger),
            &concurrency,
            &cache,
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

    let lock = result.lock();
    for resolved_package in lock.packages() {
        if !requirements
            .iter()
            .any(|selected| resolved_package.name() == &selected.package)
        {
            continue;
        }
        if resolved_package
            .index(project.workspace().install_path())?
            .is_some()
            && let Some(version) = resolved_package.version()
        {
            // A universal lock can contain versions from disjoint forks, so collect each version
            // only for the declarations that apply to its fork.
            for selected in &mut requirements {
                if resolved_package.name() != &selected.package {
                    continue;
                }
                if resolved_package.is_included_by_marker(selected.effective_marker) {
                    selected.resolved_versions.insert(version.clone());
                }
            }
        }
    }

    let mut updated_requirements = Vec::new();
    for selected in &requirements {
        let proposed_requirement =
            match propose_requirement(&selected.requirement, &selected.resolved_versions) {
                Ok(proposed_requirement) => proposed_requirement,
                Err(err) => {
                    declaration_outcomes.push(DeclarationOutcome::Blocked {
                        declaration: selected.identity(),
                        reason: BlockedReason::UnrepresentableRequirement(err),
                    });
                    continue;
                }
            };
        if proposed_requirement != selected.requirement {
            let existing =
                into_verbatim_requirement(selected.requirement.clone(), &selected.package)?;
            let replacement = into_verbatim_requirement(proposed_requirement, &selected.package)?;
            let update = RequirementUpdate {
                package: selected.package.clone(),
                dependency_type: selected.dependency_type.clone(),
                original_text: selected.original_text.clone(),
                existing,
                replacement,
            };
            declaration_outcomes.push(DeclarationOutcome::Changed(Box::new(update.clone())));
            if !updated_requirements
                .iter()
                .any(|existing_update: &RequirementUpdate| {
                    existing_update.dependency_type == selected.dependency_type
                        && existing_update.existing == update.existing
                        && existing_update.replacement == update.replacement
                })
            {
                updated_requirements.push(update);
            }
        } else {
            declaration_outcomes.push(DeclarationOutcome::Unchanged(selected.identity()));
        }
    }

    let unsafe_blocked = declaration_outcomes
        .iter()
        .any(DeclarationOutcome::invalidates_relaxed_solve);
    // The blocked declaration was relaxed for the solve, but cannot be rewritten to admit the
    // resolved versions. Applying other updates would leave a manifest inconsistent with that solve.
    if unsafe_blocked && !updated_requirements.is_empty() {
        render_declaration_outcomes(&declaration_outcomes, printer)?;
        bail!(
            "Could not safely apply dependency updates because one or more selected requirements could not be represented"
        );
    }

    if !updated_requirements.is_empty() {
        let mut pyproject = PyProjectTomlMut::from_toml(
            &project.current_project().pyproject_toml().raw,
            DependencyTarget::PyProjectToml,
        )?;
        apply_requirement_replacements(&mut pyproject, updated_requirements.iter())?;
        let pyproject_path = project.project_root().join("pyproject.toml");
        fs_err::write(pyproject_path, pyproject.to_string())?;
    }

    let events = match &result {
        LockResult::Changed(previous, lock) => {
            LockEvent::detect_changes(previous.as_ref(), lock, DryRun::Enabled)
                .filter(|event| {
                    selected_packages
                        .iter()
                        .any(|package| event.package() == package)
                })
                .collect::<Vec<_>>()
        }
        LockResult::Unchanged(_) => Vec::new(),
    };
    for package in &selected_packages {
        if !declaration_outcomes
            .iter()
            .any(|outcome| outcome.package() == package && outcome.supports_lock_event())
        {
            continue;
        }
        if let Some(event) = events.iter().find(|event| event.package() == package) {
            writeln!(printer.stderr(), "{event}")?;
        } else {
            writeln!(printer.stderr(), "No version change for {package}")?;
        }
    }
    render_declaration_outcomes(&declaration_outcomes, printer)?;
    for update in updated_requirements {
        writeln!(
            printer.stderr(),
            "Updated requirement: `{}` -> `{}`",
            update.original_text,
            update.replacement
        )?;
    }

    Ok(ExitStatus::Success)
}

/// Return whether selected declarations need the interpreter-derived Python bound used by locking.
fn requires_fallback_interpreter(
    project: &ProjectWorkspace,
    packages: &[PackageName],
    exclude: &[PackageName],
) -> Result<bool> {
    let target = LockTarget::from(project.workspace());
    if target.requires_python()?.is_some() {
        return Ok(false);
    }

    let is_explicit_selection = !packages.is_empty();
    let pyproject_path = project.project_root().join("pyproject.toml");
    for dependency in project
        .current_project()
        .project()
        .dependencies
        .as_deref()
        .unwrap_or_default()
    {
        let requirement = parse_dependency(dependency, "`project.dependencies`", &pyproject_path)?;
        if exclude.contains(&requirement.name)
            || (is_explicit_selection && !packages.contains(&requirement.name))
        {
            continue;
        }
        return Ok(true);
    }

    Ok(false)
}

fn parse_dependency(
    dependency: &str,
    table_name: &str,
    pyproject_path: &Path,
) -> Result<Requirement<VerbatimParsedUrl>> {
    Requirement::<VerbatimParsedUrl>::from_str(dependency).map_err(|error| {
        let message = match error.message {
            Pep508ErrorSource::String(message)
            | Pep508ErrorSource::UnsupportedRequirement(message) => message,
            Pep508ErrorSource::UrlError(_) => "Invalid URL requirement".to_string(),
        };
        anyhow!(
            "Failed to parse dependency from {table_name} in `{}`: {message}",
            pyproject_path.display()
        )
    })
}

/// Select the dependency declarations targeted by `uv upgrade`.
fn select_requirements(
    project: &ProjectWorkspace,
    packages: &[PackageName],
    exclude: &[PackageName],
    fallback_interpreter: Option<&Interpreter>,
) -> Result<DeclarationSelection> {
    if project.workspace().packages().len() != 1 {
        bail!("`uv upgrade` does not support workspaces with multiple members yet");
    }

    let is_explicit_selection = !packages.is_empty();
    let resolution_marker = project_resolution_marker(project, fallback_interpreter)?;
    let dependencies = project
        .current_project()
        .project()
        .dependencies
        .as_deref()
        .unwrap_or_default();
    let pyproject_path = project.project_root().join("pyproject.toml");
    let optional_dependencies = project
        .current_project()
        .project()
        .optional_dependencies
        .as_ref();
    let mut to_upgrade = Vec::new();
    let mut skipped = Vec::new();
    let mut found_packages = BTreeSet::new();
    let mut selected_packages = Vec::new();
    for dependency in dependencies {
        let requirement = parse_dependency(dependency, "`project.dependencies`", &pyproject_path)?;
        if exclude.contains(&requirement.name)
            || (is_explicit_selection && !packages.contains(&requirement.name))
        {
            continue;
        }

        found_packages.insert(requirement.name.clone());
        let mut effective_marker = requirement.marker;
        effective_marker.and(resolution_marker);
        if effective_marker.is_false() {
            skipped.push(SkippedRequirement {
                package: requirement.name.clone(),
                dependency_type: DependencyType::Production,
                original_text: dependency.clone(),
                display_text: display_requirement(&requirement),
                reason: SkippedReason::EnvironmentOrPythonRequirement,
            });
            continue;
        }

        effective_marker = effective_marker.simplify_not_extras_with(|extra| {
            optional_dependencies
                .is_none_or(|optional_dependencies| !optional_dependencies.contains_key(extra))
        });
        if effective_marker.is_false() {
            skipped.push(SkippedRequirement {
                package: requirement.name.clone(),
                dependency_type: DependencyType::Production,
                original_text: dependency.clone(),
                display_text: display_requirement(&requirement),
                reason: SkippedReason::UndefinedExtra,
            });
            continue;
        }
        if !selected_packages.contains(&requirement.name) {
            selected_packages.push(requirement.name.clone());
        }
        to_upgrade.push(UpgradableRequirement {
            package: requirement.name.clone(),
            dependency_type: DependencyType::Production,
            original_text: dependency.clone(),
            requirement,
            effective_marker,
            resolved_versions: BTreeSet::new(),
        });
    }

    for package in packages
        .iter()
        .filter(|package| !exclude.contains(*package))
    {
        if !found_packages.contains(package) {
            bail!("Dependency `{package}` was not found in `project.dependencies`");
        }
    }
    if is_explicit_selection {
        selected_packages.clear();
        for package in packages
            .iter()
            .filter(|package| !exclude.contains(*package))
        {
            if to_upgrade
                .iter()
                .any(|selected| selected.package == *package)
                && !selected_packages.contains(package)
            {
                selected_packages.push(package.clone());
            }
        }
    }

    if selected_packages.is_empty() && skipped.is_empty() {
        bail!("No dependencies selected for upgrade");
    }

    Ok(DeclarationSelection {
        active: to_upgrade,
        skipped,
        packages: selected_packages,
    })
}

fn validate_requirements(
    project: &ProjectWorkspace,
    requirements: &[UpgradableRequirement],
    packages: &[PackageName],
) -> Result<()> {
    for package in packages {
        if requirements.iter().any(|selected| {
            selected.package == *package
                && matches!(
                    selected.requirement.version_or_url,
                    Some(VersionOrUrl::Url(_))
                )
        }) {
            bail!("Dependency `{package}` is a direct URL requirement and cannot be upgraded");
        }
    }

    for package in packages {
        if package == project.project_name() {
            bail!("Dependency `{package}` refers to the current project and cannot be upgraded");
        }
    }

    for package in packages {
        let sources = project
            .current_project()
            .pyproject_toml()
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.sources.as_ref())
            .and_then(|sources| sources.inner().get(package))
            .or_else(|| project.workspace().sources().get(package));
        let applies_to_selected_requirement = |source| {
            requirements.iter().any(|selected| {
                selected.package == *package
                    && source_is_applicable(
                        source,
                        &selected.dependency_type,
                        selected.effective_marker,
                    )
            })
        };
        if sources.is_some_and(|sources| {
            sources.iter().any(|source| {
                applies_to_selected_requirement(source)
                    && matches!(source, Source::Git { rev: Some(_), .. })
            })
        }) {
            bail!(
                "Dependency `{package}` is pinned to a Git revision and cannot be upgraded commit-to-commit"
            );
        }
        if sources.is_some_and(|sources| {
            sources.iter().any(|source| {
                applies_to_selected_requirement(source)
                    && !matches!(source, Source::Registry { .. })
            })
        }) {
            bail!(
                "Dependency `{package}` uses a non-registry source in `tool.uv.sources` and cannot be upgraded"
            );
        }
    }

    Ok(())
}

/// Return a requirement suitable for diagnostics without exposing URL query parameters.
fn display_requirement(requirement: &Requirement<VerbatimParsedUrl>) -> String {
    let mut requirement = requirement.clone();
    if let Some(VersionOrUrl::Url(url)) = &mut requirement.version_or_url {
        let mut display_url = url.verbatim.to_url();
        display_url.set_query(None);
        url.verbatim = VerbatimUrl::from_url(display_url);
    }
    requirement.to_string()
}

fn render_skipped_requirements(skipped: &[SkippedRequirement], printer: Printer) -> Result<()> {
    for requirement in skipped {
        let reason = match requirement.reason {
            SkippedReason::EnvironmentOrPythonRequirement => {
                "excluded by the project's environments or Python requirement"
            }
            SkippedReason::UndefinedExtra => {
                "references an extra that the project does not provide"
            }
        };
        writeln!(
            printer.stderr(),
            "warning: Skipping dependency `{}` in {}: `{}` ({reason})",
            requirement.package,
            requirement.dependency_type.toml_table_name(),
            requirement.display_text
        )?;
    }
    Ok(())
}

fn render_declaration_outcomes(outcomes: &[DeclarationOutcome], printer: Printer) -> Result<()> {
    for outcome in outcomes {
        match outcome {
            DeclarationOutcome::Blocked {
                declaration,
                reason,
            } => {
                writeln!(
                    printer.stderr(),
                    "warning: Could not update dependency `{}` in {}: `{}` ({})",
                    declaration.package,
                    declaration.dependency_type.toml_table_name(),
                    declaration.text,
                    reason.message()
                )?;
            }
            DeclarationOutcome::Changed(_) | DeclarationOutcome::Unchanged(_) => {}
        }
    }
    Ok(())
}

/// Return the marker domain that the project will resolve for dependency declarations.
fn project_resolution_marker(
    project: &ProjectWorkspace,
    fallback_interpreter: Option<&Interpreter>,
) -> Result<MarkerTree> {
    let target = LockTarget::from(project.workspace());
    let requires_python = match target.requires_python()? {
        Some(requires_python) => requires_python.to_marker_tree(),
        None => fallback_interpreter.map_or(MarkerTree::TRUE, |interpreter| {
            RequiresPython::greater_than_equal_version(&interpreter.python_minor_version())
                .to_marker_tree()
        }),
    };
    let environments = target
        .environments()
        .map(SupportedEnvironments::as_markers)
        .unwrap_or_default();
    Ok(implicit_constraints_marker(requires_python, environments))
}

/// Apply exact requirement replacements, coalescing repeated identical declarations.
fn apply_requirement_replacements<'a>(
    pyproject: &mut PyProjectTomlMut,
    replacements: impl IntoIterator<Item = &'a RequirementUpdate>,
) -> Result<()> {
    let mut applied = Vec::new();
    for RequirementUpdate {
        package,
        dependency_type,
        existing,
        replacement,
        ..
    } in replacements
    {
        if let Some((_, _, applied_replacement)) =
            applied
                .iter()
                .find(|(applied_dependency_type, applied_existing, _)| {
                    *applied_dependency_type == dependency_type && *applied_existing == existing
                })
        {
            if *applied_replacement != replacement {
                bail!("Dependency `{package}` has conflicting requirement updates");
            }
            continue;
        }

        if pyproject
            .replace_dependency_declaration(dependency_type, existing, replacement)?
            .is_empty()
        {
            bail!(
                "Dependency `{package}` was not found in {}",
                dependency_type.toml_table_name()
            );
        }
        applied.push((dependency_type, existing, replacement));
    }
    Ok(())
}

/// Return whether a source applies to the selected requirement declaration.
fn source_is_applicable(
    source: &Source,
    dependency_type: &DependencyType,
    requirement_marker: MarkerTree,
) -> bool {
    let source_origin_applies = match dependency_type {
        DependencyType::Production => {
            let extra = requirement_marker.top_level_extra_name();
            source
                .extra()
                .is_none_or(|target| extra.as_deref() == Some(target))
                && source.group().is_none()
        }
        DependencyType::Dev => source.extra().is_none() && source.group().is_none(),
        DependencyType::Optional(extra) => {
            source.extra() == Some(extra) && source.group().is_none()
        }
        DependencyType::Group(group) => source.extra().is_none() && source.group() == Some(group),
    };
    source_origin_applies && !source.marker().is_disjoint(requirement_marker)
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
) -> Result<Requirement<VerbatimParsedUrl>, ProposeRequirementError> {
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
        return Err(ProposeRequirementError::Unrepresentable {
            package: requirement.name.clone(),
            resolved_versions: resolved_versions.clone(),
        });
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

    use super::{
        ProposeRequirementError, increment_version_at_precision, propose_requirement,
        relax_requirement,
    };

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

        assert!(matches!(
            &error,
            ProposeRequirementError::Unrepresentable {
                package,
                resolved_versions: actual_resolved_versions,
            } if package.as_ref() == "requests"
                && *actual_resolved_versions == resolved_versions(&["2.4"])
        ));
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

        assert!(matches!(
            &error,
            ProposeRequirementError::Unrepresentable {
                package,
                resolved_versions: actual_resolved_versions,
            } if package.as_ref() == "requests"
                && *actual_resolved_versions == resolved_versions(&["1.5.0", "2.4.0"])
        ));
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

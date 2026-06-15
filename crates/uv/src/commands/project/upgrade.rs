use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use itertools::Itertools;
use serde::Serialize;
use uv_cache::{Cache, Refresh};
use uv_cli::SyncFormat;
use uv_client::BaseClientBuilder;
use uv_configuration::{Concurrency, DependencyGroupsWithDefaults, DryRun, NoSources, Upgrade};
use uv_distribution::{ArchiveMetadata, Metadata};
use uv_distribution_types::{Identifier, RequiresPython};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::{Operator, Version, VersionSpecifier, VersionSpecifiers};
use uv_pep508::{MarkerTree, Pep508ErrorSource, Requirement, VerbatimUrl, VersionOrUrl};
use uv_preview::{Preview, PreviewFeature};
use uv_pypi_types::{
    DependencyGroupSpecifier, PyProjectToml, ResolutionMetadata, SupportedEnvironments,
    VerbatimParsedUrl,
};
use uv_python::{ConfigDiscovery, Interpreter, PythonDownloads, PythonPreference};
use uv_redacted::DisplaySafeUrl;
use uv_resolver::{MetadataResponse, implicit_constraints_marker};
use uv_settings::PythonInstallMirrors;
use uv_warnings::warn_user;
use uv_workspace::dependency_groups::FlatDependencyGroups;
use uv_workspace::pyproject::{DependencyType, PyProjectToml as WorkspacePyProjectToml, Source};
use uv_workspace::pyproject_mut::{DependencyTarget, PyProjectTomlMut};
use uv_workspace::{
    DiscoveryOptions, ProjectWorkspace, VirtualProject, Workspace, WorkspaceCache,
    WorkspaceErrorKind,
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
    member: PackageName,
    package: PackageName,
    dependency_type: DependencyType,
    original_text: String,
    requirement: Requirement<VerbatimParsedUrl>,
    effective_marker: MarkerTree,
    contexts: Vec<DeclarationContext>,
    resolved_versions: BTreeSet<Version>,
}

/// A selected requirement that cannot apply to the project.
#[derive(Clone)]
struct SkippedRequirement {
    member: PackageName,
    package: PackageName,
    dependency_type: DependencyType,
    contexts: Vec<DeclarationContext>,
    original_text: String,
    display_text: String,
    reason: SkippedReason,
}

#[derive(Clone)]
enum SkippedReason {
    EnvironmentOrPythonRequirement,
    UndefinedExtra,
}

#[derive(Clone)]
struct DeclarationContext {
    dependency_type: DependencyType,
    marker: MarkerTree,
}

struct DependencyDeclaration {
    dependency_type: DependencyType,
    text: String,
    contexts: Vec<DeclarationContext>,
}

/// The requirements selected for upgrade and those that cannot apply.
struct DeclarationSelection {
    active: Vec<UpgradableRequirement>,
    skipped: Vec<SkippedRequirement>,
    packages: Vec<PackageName>,
}

#[derive(Clone)]
struct DeclarationIdentity {
    member: PackageName,
    package: PackageName,
    dependency_type: DependencyType,
    text: String,
    resolved_versions: BTreeSet<Version>,
}

/// An existing requirement and its proposed replacement.
#[derive(Clone)]
struct RequirementUpdate {
    member: PackageName,
    package: PackageName,
    dependency_type: DependencyType,
    original_text: String,
    existing: Requirement<VerbatimUrl>,
    replacement: Requirement<VerbatimUrl>,
    resolved_versions: BTreeSet<Version>,
}

struct UpgradeWorkspace {
    target: VirtualProject,
    selected_projects: Vec<ProjectWorkspace>,
    display_members: bool,
}

enum DeclarationOutcome {
    Changed(Box<RequirementUpdate>),
    Unchanged(DeclarationIdentity),
    Blocked {
        declaration: DeclarationIdentity,
        reason: BlockedReason,
    },
    Skipped(SkippedRequirement),
}

enum BlockedReason {
    UnrepresentableRequirement(ProposeRequirementError),
    ConflictExtra { extra: String },
    ConflictGroup { group: String },
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

#[derive(Serialize, Debug)]
struct UpgradeReport {
    schema: SchemaReport,
    dry_run: bool,
    packages: Vec<PackageName>,
    declarations: Vec<UpgradeDeclarationReport>,
}

#[derive(Serialize, Debug, Default)]
struct SchemaReport {
    version: SchemaVersion,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "snake_case")]
enum SchemaVersion {
    #[default]
    Preview,
}

#[derive(Serialize, Debug)]
struct UpgradeDeclarationReport {
    member: PackageName,
    package: PackageName,
    dependency_type: UpgradeDependencyType,
    location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    extra: Option<ExtraName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<GroupName>,
    original_requirement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_requirement: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    resolved_versions: Vec<Version>,
    status: UpgradeDeclarationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<UpgradeReasonReport>,
}

#[derive(Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum UpgradeDependencyType {
    Production,
    Optional,
    Group,
    Dev,
}

#[derive(Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum UpgradeDeclarationStatus {
    Changed,
    Unchanged,
    Blocked,
    Skipped,
}

#[derive(Serialize, Debug)]
struct UpgradeReasonReport {
    code: UpgradeReasonCode,
    message: String,
}

#[derive(Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum UpgradeReasonCode {
    UnrepresentableRequirement,
    ConflictExtra,
    ConflictGroup,
    EnvironmentOrPythonRequirement,
    UndefinedExtra,
}

impl DeclarationOutcome {
    fn package(&self) -> &PackageName {
        match self {
            Self::Changed(update) => &update.package,
            Self::Unchanged(declaration) | Self::Blocked { declaration, .. } => {
                &declaration.package
            }
            Self::Skipped(declaration) => &declaration.package,
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

impl UpgradeReport {
    fn from_outcomes(outcomes: &[DeclarationOutcome], dry_run: DryRun) -> Self {
        let mut packages = Vec::new();
        for outcome in outcomes {
            if !packages.contains(outcome.package()) {
                packages.push(outcome.package().clone());
            }
        }

        Self {
            schema: SchemaReport::default(),
            dry_run: dry_run.enabled(),
            packages,
            declarations: outcomes
                .iter()
                .map(UpgradeDeclarationReport::from_outcome)
                .collect(),
        }
    }

    fn format(&self, output_format: SyncFormat) -> Option<String> {
        match output_format {
            SyncFormat::Json => serde_json::to_string_pretty(self).ok(),
            SyncFormat::Text => None,
        }
    }
}

impl UpgradeDeclarationReport {
    fn from_outcome(outcome: &DeclarationOutcome) -> Self {
        match outcome {
            DeclarationOutcome::Changed(update) => Self::from_changed(update),
            DeclarationOutcome::Unchanged(declaration) => {
                Self::from_identity(declaration, UpgradeDeclarationStatus::Unchanged, None, None)
            }
            DeclarationOutcome::Blocked {
                declaration,
                reason,
            } => Self::from_identity(
                declaration,
                UpgradeDeclarationStatus::Blocked,
                Some(UpgradeReasonReport::from(reason)),
                None,
            ),
            DeclarationOutcome::Skipped(declaration) => Self::from_skipped(declaration),
        }
    }

    fn from_changed(update: &RequirementUpdate) -> Self {
        Self {
            member: update.member.clone(),
            package: update.package.clone(),
            dependency_type: UpgradeDependencyType::from(&update.dependency_type),
            location: dependency_location(&update.dependency_type),
            extra: dependency_extra(&update.dependency_type),
            group: dependency_group(&update.dependency_type),
            original_requirement: update.original_text.clone(),
            new_requirement: Some(update.replacement.to_string()),
            resolved_versions: update.resolved_versions.iter().cloned().collect(),
            status: UpgradeDeclarationStatus::Changed,
            reason: None,
        }
    }

    fn from_identity(
        declaration: &DeclarationIdentity,
        status: UpgradeDeclarationStatus,
        reason: Option<UpgradeReasonReport>,
        new_requirement: Option<String>,
    ) -> Self {
        Self {
            member: declaration.member.clone(),
            package: declaration.package.clone(),
            dependency_type: UpgradeDependencyType::from(&declaration.dependency_type),
            location: dependency_location(&declaration.dependency_type),
            extra: dependency_extra(&declaration.dependency_type),
            group: dependency_group(&declaration.dependency_type),
            original_requirement: declaration.text.clone(),
            new_requirement,
            resolved_versions: declaration.resolved_versions.iter().cloned().collect(),
            status,
            reason,
        }
    }

    fn from_skipped(declaration: &SkippedRequirement) -> Self {
        Self {
            member: declaration.member.clone(),
            package: declaration.package.clone(),
            dependency_type: UpgradeDependencyType::from(&declaration.dependency_type),
            location: dependency_location(&declaration.dependency_type),
            extra: dependency_extra(&declaration.dependency_type),
            group: dependency_group(&declaration.dependency_type),
            original_requirement: declaration.display_text.clone(),
            new_requirement: None,
            resolved_versions: Vec::new(),
            status: UpgradeDeclarationStatus::Skipped,
            reason: Some(UpgradeReasonReport {
                code: declaration.reason.code(),
                message: declaration.reason.message().to_string(),
            }),
        }
    }
}

impl From<&DependencyType> for UpgradeDependencyType {
    fn from(value: &DependencyType) -> Self {
        match value {
            DependencyType::Production => Self::Production,
            DependencyType::Optional(_) => Self::Optional,
            DependencyType::Group(_) => Self::Group,
            DependencyType::Dev => Self::Dev,
        }
    }
}

impl From<&BlockedReason> for UpgradeReasonReport {
    fn from(value: &BlockedReason) -> Self {
        Self {
            code: value.code(),
            message: value.message(),
        }
    }
}

impl BlockedReason {
    fn code(&self) -> UpgradeReasonCode {
        match self {
            Self::UnrepresentableRequirement(_) => UpgradeReasonCode::UnrepresentableRequirement,
            Self::ConflictExtra { .. } => UpgradeReasonCode::ConflictExtra,
            Self::ConflictGroup { .. } => UpgradeReasonCode::ConflictGroup,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::UnrepresentableRequirement(error) => error.to_string(),
            Self::ConflictExtra { extra } => {
                format!("declared under conflicting extra `{extra}`")
            }
            Self::ConflictGroup { group } => {
                format!("declared under conflicting dependency group `{group}`")
            }
        }
    }
}

impl SkippedReason {
    fn code(&self) -> UpgradeReasonCode {
        match self {
            Self::EnvironmentOrPythonRequirement => {
                UpgradeReasonCode::EnvironmentOrPythonRequirement
            }
            Self::UndefinedExtra => UpgradeReasonCode::UndefinedExtra,
        }
    }

    fn message(&self) -> &'static str {
        match self {
            Self::EnvironmentOrPythonRequirement => {
                "excluded by the project's environments or Python requirement"
            }
            Self::UndefinedExtra => "references an extra that the project does not provide",
        }
    }
}

fn dependency_location(dependency_type: &DependencyType) -> String {
    match dependency_type {
        DependencyType::Production => "project.dependencies".to_string(),
        DependencyType::Optional(extra) => format!("project.optional-dependencies.{extra}"),
        DependencyType::Group(group) => format!("dependency-groups.{group}"),
        DependencyType::Dev => "tool.uv.dev-dependencies".to_string(),
    }
}

fn dependency_extra(dependency_type: &DependencyType) -> Option<ExtraName> {
    match dependency_type {
        DependencyType::Optional(extra) => Some(extra.clone()),
        DependencyType::Production | DependencyType::Group(_) | DependencyType::Dev => None,
    }
}

fn dependency_group(dependency_type: &DependencyType) -> Option<GroupName> {
    match dependency_type {
        DependencyType::Group(group) => Some(group.clone()),
        DependencyType::Production | DependencyType::Optional(_) | DependencyType::Dev => None,
    }
}

impl UpgradableRequirement {
    fn identity(&self) -> DeclarationIdentity {
        DeclarationIdentity {
            member: self.member.clone(),
            package: self.package.clone(),
            dependency_type: self.dependency_type.clone(),
            text: self.original_text.clone(),
            resolved_versions: self.resolved_versions.clone(),
        }
    }
}

pub(crate) async fn upgrade(
    project_dir: &Path,
    packages: Vec<PackageName>,
    exclude: Vec<PackageName>,
    package: Option<PackageName>,
    all_packages: bool,
    dry_run: DryRun,
    output_format: SyncFormat,
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
    if matches!(output_format, SyncFormat::Json) && !preview.is_enabled(PreviewFeature::JsonOutput)
    {
        warn_user!(
            "The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features {}` to disable this warning.",
            PreviewFeature::JsonOutput
        );
    }

    let upgrade_workspace =
        discover_upgrade_workspace(project_dir, package, all_packages, cache, workspace_cache)
            .await?;
    let fallback_interpreter = if requires_fallback_interpreter(
        upgrade_workspace.target.workspace(),
        &upgrade_workspace.selected_projects,
        &packages,
        &exclude,
    )? {
        let selection = select_requirements(
            upgrade_workspace.target.workspace(),
            &upgrade_workspace.selected_projects,
            &packages,
            &exclude,
            None,
        )?;
        if selection.active.is_empty() {
            render_skipped_requirements(
                &selection.skipped,
                upgrade_workspace.display_members,
                printer,
            )?;
            let outcomes = selection
                .skipped
                .into_iter()
                .map(DeclarationOutcome::Skipped)
                .collect::<Vec<_>>();
            render_upgrade_report(
                &outcomes,
                &[],
                &[],
                upgrade_workspace.display_members,
                dry_run,
                output_format,
                printer,
            )?;
            return Ok(ExitStatus::Success);
        }

        let groups = DependencyGroupsWithDefaults::none();
        let workspace_python = WorkspacePython::from_request(
            None,
            Some(upgrade_workspace.target.workspace()),
            &groups,
            project_dir,
            config_discovery,
        )
        .await?;
        match ProjectInterpreter::discover(
            upgrade_workspace.target.workspace(),
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
                } = select_requirements(
                    upgrade_workspace.target.workspace(),
                    &upgrade_workspace.selected_projects,
                    &packages,
                    &exclude,
                    None,
                )?;
                render_skipped_requirements(&skipped, upgrade_workspace.display_members, printer)?;
                validate_requirements(
                    upgrade_workspace.target.workspace(),
                    &upgrade_workspace.selected_projects,
                    &active,
                    &packages,
                    &settings.sources,
                )?;
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
    } = select_requirements(
        upgrade_workspace.target.workspace(),
        &upgrade_workspace.selected_projects,
        &packages,
        &exclude,
        fallback_interpreter.as_ref(),
    )?;
    render_skipped_requirements(&skipped, upgrade_workspace.display_members, printer)?;
    validate_requirements(
        upgrade_workspace.target.workspace(),
        &upgrade_workspace.selected_projects,
        &requirements,
        &selected_packages,
        &settings.sources,
    )?;
    let mut declaration_outcomes = skipped
        .iter()
        .cloned()
        .map(DeclarationOutcome::Skipped)
        .collect::<Vec<_>>();
    let mut conflicts = upgrade_workspace.target.workspace().conflicts()?;
    for selected_project in &upgrade_workspace.selected_projects {
        if let Some(groups) = &selected_project
            .current_project()
            .pyproject_toml()
            .dependency_groups
        {
            conflicts.expand_transitive_group_includes(selected_project.project_name(), groups);
        }
    }
    let mut requirements = requirements
        .into_iter()
        .filter_map(|selected| {
            let blocked_reason =
                selected
                    .contexts
                    .iter()
                    .find_map(|context| match &context.dependency_type {
                        DependencyType::Production => context
                            .marker
                            .top_level_extra_name()
                            .map(std::borrow::Cow::into_owned)
                            .filter(|extra| conflicts.contains(&selected.member, extra))
                            .map(|extra| BlockedReason::ConflictExtra {
                                extra: extra.to_string(),
                            }),
                        DependencyType::Optional(extra) => {
                            if conflicts.contains(&selected.member, extra) {
                                Some(BlockedReason::ConflictExtra {
                                    extra: extra.to_string(),
                                })
                            } else {
                                None
                            }
                        }
                        DependencyType::Group(group) => {
                            if conflicts.contains(&selected.member, group) {
                                Some(BlockedReason::ConflictGroup {
                                    group: group.to_string(),
                                })
                            } else {
                                None
                            }
                        }
                        DependencyType::Dev => None,
                    });
            if let Some(reason) = blocked_reason {
                declaration_outcomes.push(DeclarationOutcome::Blocked {
                    declaration: selected.identity(),
                    reason,
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
        render_upgrade_report(
            &declaration_outcomes,
            &[],
            &selected_packages,
            upgrade_workspace.display_members,
            dry_run,
            output_format,
            printer,
        )?;
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
                member: selected.member.clone(),
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
                resolved_versions: BTreeSet::new(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let resolution_marker = workspace_resolution_marker(
        upgrade_workspace.target.workspace(),
        fallback_interpreter.as_ref(),
    )?;

    let state = UniversalState::default();
    for selected_project in &upgrade_workspace.selected_projects {
        let metadata = temporary_member_metadata(
            selected_project,
            relaxed_requirements
                .iter()
                .filter(|replacement| replacement.member == *selected_project.project_name()),
            skipped
                .iter()
                .filter(|requirement| requirement.member == *selected_project.project_name()),
            &settings,
            resolution_marker,
            &client_builder,
        )?;
        if let Some(metadata) = metadata {
            let distribution_id = DisplaySafeUrl::from_file_path(selected_project.project_root())
                .map_err(|()| anyhow!("Project root is not a valid file URL"))?
                .distribution_id();
            state.index().distributions().done(
                distribution_id,
                Arc::new(MetadataResponse::Found(ArchiveMetadata::from(metadata))),
            );
        }
    }
    let interpreter = if let Some(interpreter) = fallback_interpreter {
        interpreter
    } else {
        let groups = DependencyGroupsWithDefaults::none();
        let workspace_python = WorkspacePython::from_request(
            None,
            Some(upgrade_workspace.target.workspace()),
            &groups,
            project_dir,
            config_discovery,
        )
        .await?;
        ProjectInterpreter::discover(
            upgrade_workspace.target.workspace(),
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
        .execute(LockTarget::Workspace(upgrade_workspace.target.workspace())),
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
            .index(upgrade_workspace.target.workspace().install_path())?
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
                member: selected.member.clone(),
                package: selected.package.clone(),
                dependency_type: selected.dependency_type.clone(),
                original_text: selected.original_text.clone(),
                existing,
                replacement,
                resolved_versions: selected.resolved_versions.clone(),
            };
            declaration_outcomes.push(DeclarationOutcome::Changed(Box::new(update.clone())));
            if !updated_requirements
                .iter()
                .any(|existing_update: &RequirementUpdate| {
                    existing_update.member == selected.member
                        && existing_update.dependency_type == selected.dependency_type
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
        render_declaration_outcomes(
            &declaration_outcomes,
            upgrade_workspace.display_members,
            printer,
        )?;
        bail!(
            "Could not safely apply dependency updates because one or more selected requirements could not be represented"
        );
    }

    if !dry_run.enabled() && !updated_requirements.is_empty() {
        let mut pyproject_writes = Vec::new();
        for selected_project in &upgrade_workspace.selected_projects {
            if !updated_requirements
                .iter()
                .any(|update| update.member == *selected_project.project_name())
            {
                continue;
            }
            let mut pyproject = PyProjectTomlMut::from_toml(
                &selected_project.current_project().pyproject_toml().raw,
                DependencyTarget::PyProjectToml,
            )?;
            apply_requirement_replacements(
                &mut pyproject,
                updated_requirements
                    .iter()
                    .filter(|update| update.member == *selected_project.project_name()),
            )?;
            let pyproject_path = selected_project.project_root().join("pyproject.toml");
            pyproject_writes.push((pyproject_path, pyproject.to_string()));
        }
        for (pyproject_path, contents) in pyproject_writes {
            fs_err::write(pyproject_path, contents)?;
        }
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
    render_upgrade_report(
        &declaration_outcomes,
        &events,
        &selected_packages,
        upgrade_workspace.display_members,
        dry_run,
        output_format,
        printer,
    )?;

    Ok(ExitStatus::Success)
}

/// Return whether selected declarations need the interpreter-derived Python bound used by locking.
fn requires_fallback_interpreter(
    workspace: &Workspace,
    selected_projects: &[ProjectWorkspace],
    packages: &[PackageName],
    exclude: &[PackageName],
) -> Result<bool> {
    let target = LockTarget::from(workspace);
    if target.requires_python()?.is_some() {
        return Ok(false);
    }

    let is_explicit_selection = !packages.is_empty();
    for project in selected_projects {
        let member = project.project_name();
        let pyproject_path = project.project_root().join("pyproject.toml");
        for declaration in current_project_dependency_declarations(project) {
            let DependencyDeclaration {
                dependency_type,
                text: dependency,
                contexts: _,
            } = declaration;
            let table_name = dependency_type.toml_table_name();
            let requirement = parse_dependency(&dependency, &table_name, &pyproject_path)?;
            if exclude.contains(&requirement.name)
                || (is_explicit_selection && !packages.contains(&requirement.name))
                || (!is_explicit_selection && requirement.name == *member)
            {
                continue;
            }
            return Ok(true);
        }
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

async fn discover_upgrade_workspace(
    project_dir: &Path,
    package: Option<PackageName>,
    all_packages: bool,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
) -> Result<UpgradeWorkspace> {
    if all_packages {
        let target = discover_virtual_project(project_dir, cache, workspace_cache).await?;
        let member_names = target
            .workspace()
            .packages()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut selected_projects = Vec::new();
        for member_name in member_names {
            selected_projects.push(
                discover_package_project(project_dir, member_name, cache, workspace_cache).await?,
            );
        }
        return Ok(UpgradeWorkspace {
            display_members: selected_projects.len() > 1,
            target,
            selected_projects,
        });
    }

    if let Some(package) = package {
        let project =
            discover_package_project(project_dir, package, cache, workspace_cache).await?;
        return Ok(UpgradeWorkspace {
            target: VirtualProject::Project(project.clone()),
            selected_projects: vec![project],
            display_members: false,
        });
    }

    match discover_virtual_project(project_dir, cache, workspace_cache).await? {
        VirtualProject::Project(project) => Ok(UpgradeWorkspace {
            target: VirtualProject::Project(project.clone()),
            selected_projects: vec![project],
            display_members: false,
        }),
        VirtualProject::NonProject(_) => {
            bail!(
                "`uv upgrade` requires a project with a `[project]` table; use `--package` or `--all-packages` to select workspace members"
            )
        }
    }
}

async fn discover_virtual_project(
    project_dir: &Path,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
) -> Result<VirtualProject> {
    match VirtualProject::discover(
        project_dir,
        &DiscoveryOptions::default(),
        cache,
        workspace_cache,
    )
    .await
    {
        Ok(project) => Ok(project),
        Err(err)
            if matches!(
                err.as_ref(),
                WorkspaceErrorKind::MissingPyprojectToml | WorkspaceErrorKind::MissingProject(_)
            ) =>
        {
            bail!("`uv upgrade` requires a project with a `[project]` table");
        }
        Err(err) => Err(err.into()),
    }
}

async fn discover_package_project(
    project_dir: &Path,
    package: PackageName,
    cache: &Cache,
    workspace_cache: &WorkspaceCache,
) -> Result<ProjectWorkspace> {
    match VirtualProject::discover_with_package(
        project_dir,
        &DiscoveryOptions::default(),
        cache,
        workspace_cache,
        package,
    )
    .await
    {
        Ok(VirtualProject::Project(project)) => Ok(project),
        Ok(VirtualProject::NonProject(_)) => {
            bail!("`uv upgrade --package` requires a workspace member with a `[project]` table")
        }
        Err(err)
            if matches!(
                err.as_ref(),
                WorkspaceErrorKind::MissingPyprojectToml | WorkspaceErrorKind::MissingProject(_)
            ) =>
        {
            bail!("`uv upgrade` requires a project with a `[project]` table");
        }
        Err(err) => Err(err.into()),
    }
}

fn temporary_member_metadata<'a>(
    project: &ProjectWorkspace,
    relaxed_requirements: impl IntoIterator<Item = &'a RequirementUpdate>,
    skipped_requirements: impl IntoIterator<Item = &'a SkippedRequirement>,
    settings: &ResolverSettings,
    resolution_marker: MarkerTree,
    client_builder: &BaseClientBuilder<'_>,
) -> Result<Option<Metadata>> {
    let relaxed_requirements = relaxed_requirements.into_iter().collect::<Vec<_>>();
    let skipped_requirements = skipped_requirements.into_iter().collect::<Vec<_>>();
    if relaxed_requirements.is_empty() && skipped_requirements.is_empty() {
        return Ok(None);
    }

    let mut pyproject = PyProjectTomlMut::from_toml(
        &project.current_project().pyproject_toml().raw,
        DependencyTarget::PyProjectToml,
    )?;
    for (index, requirement) in skipped_requirements.iter().enumerate() {
        if skipped_requirements[..index].iter().any(|removed| {
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
    apply_requirement_replacements(&mut pyproject, relaxed_requirements)?;
    let pyproject = pyproject.to_string();
    let pyproject_path = project.project_root().join("pyproject.toml");
    let mut workspace_pyproject =
        WorkspacePyProjectToml::from_string(pyproject.clone(), pyproject_path.clone())?;
    let pyproject = PyProjectToml::from_toml(&pyproject, pyproject_path.display())?;
    if pyproject
        .project
        .as_ref()
        .is_some_and(|project| project.version.is_none())
    {
        // TODO: Support dynamic project metadata by building the project before resolution.
        bail!("`uv upgrade` does not support projects with dynamic versions yet");
    }
    let metadata = ResolutionMetadata::parse_pyproject_toml(pyproject, None)?;
    let dependency_groups = FlatDependencyGroups::from_pyproject_toml(
        project.current_project().root(),
        &workspace_pyproject,
    )?;
    if let Some(sources) = workspace_pyproject
        .tool
        .as_mut()
        .and_then(|tool| tool.uv.as_mut())
        .and_then(|uv| uv.sources.as_mut())
    {
        sources.retain(|package, source| {
            if !skipped_requirements.iter().any(|requirement| {
                requirement.package == *package
                    && requirement
                        .contexts
                        .iter()
                        .any(|context| source_origin_applies(source, context))
            }) {
                return true;
            }

            if let Some(extra) = source.extra() {
                return metadata.requires_dist.iter().any(|requirement| {
                    requirement.name == *package
                        && requirement.marker.top_level_extra_name().as_deref() == Some(extra)
                });
            }
            if let Some(group) = source.group() {
                return dependency_groups.get(group).is_some_and(|group| {
                    group
                        .requirements
                        .iter()
                        .any(|requirement| requirement.name == *package)
                });
            }

            true
        });
    }
    let metadata = Metadata::from_project_workspace(
        metadata,
        project,
        &workspace_pyproject,
        None,
        &settings.index_locations,
        &settings.sources,
        Some(resolution_marker),
        true,
        client_builder.credentials_cache(),
    )?;
    Ok(Some(metadata))
}

/// Select the dependency declarations targeted by `uv upgrade`.
fn select_requirements(
    workspace: &Workspace,
    selected_projects: &[ProjectWorkspace],
    packages: &[PackageName],
    exclude: &[PackageName],
    fallback_interpreter: Option<&Interpreter>,
) -> Result<DeclarationSelection> {
    let is_explicit_selection = !packages.is_empty();
    let resolution_marker = workspace_resolution_marker(workspace, fallback_interpreter)?;
    let mut to_upgrade = Vec::new();
    let mut skipped = Vec::new();
    let mut found_packages = BTreeSet::new();
    let mut selected_packages = Vec::new();
    for project in selected_projects {
        let member = project.project_name().clone();
        let pyproject_path = project.project_root().join("pyproject.toml");
        let optional_dependencies = project
            .current_project()
            .project()
            .optional_dependencies
            .as_ref();
        for declaration in current_project_dependency_declarations(project) {
            let DependencyDeclaration {
                dependency_type,
                text: dependency,
                contexts,
            } = declaration;
            let table_name = dependency_type.toml_table_name();
            let requirement = parse_dependency(&dependency, &table_name, &pyproject_path)?;
            if exclude.contains(&requirement.name)
                || (is_explicit_selection && !packages.contains(&requirement.name))
            {
                continue;
            }
            // Optional self-references are valid metadata, but not implicit upgrade targets.
            if !is_explicit_selection && requirement.name == member {
                continue;
            }

            found_packages.insert(requirement.name.clone());
            let declaration_contexts = contexts
                .into_iter()
                .map(|context| {
                    let mut marker = requirement.marker;
                    marker.and(context.marker);
                    DeclarationContext {
                        dependency_type: context.dependency_type,
                        marker,
                    }
                })
                .collect::<Vec<_>>();
            let mut active_contexts = Vec::new();
            let mut effective_marker = MarkerTree::FALSE;
            let mut has_environment_context = false;
            for context in &declaration_contexts {
                let mut context_marker = context.marker;
                context_marker.and(resolution_marker);
                if context_marker.is_false() {
                    continue;
                }
                has_environment_context = true;
                context_marker = context_marker.simplify_not_extras_with(|extra| {
                    optional_dependencies.is_none_or(|optional_dependencies| {
                        !optional_dependencies.contains_key(extra)
                    })
                });
                if context_marker.is_false() {
                    continue;
                }
                effective_marker.or(context_marker);
                active_contexts.push(DeclarationContext {
                    dependency_type: context.dependency_type.clone(),
                    marker: context_marker,
                });
            }
            if active_contexts.is_empty() {
                skipped.push(SkippedRequirement {
                    member: member.clone(),
                    package: requirement.name.clone(),
                    dependency_type,
                    contexts: declaration_contexts,
                    original_text: dependency,
                    display_text: display_requirement(&requirement),
                    reason: if has_environment_context {
                        SkippedReason::UndefinedExtra
                    } else {
                        SkippedReason::EnvironmentOrPythonRequirement
                    },
                });
                continue;
            }
            if !selected_packages.contains(&requirement.name) {
                selected_packages.push(requirement.name.clone());
            }
            to_upgrade.push(UpgradableRequirement {
                member: member.clone(),
                package: requirement.name.clone(),
                dependency_type,
                original_text: dependency,
                requirement,
                effective_marker,
                contexts: active_contexts,
                resolved_versions: BTreeSet::new(),
            });
        }
    }

    for package in packages
        .iter()
        .filter(|package| !exclude.contains(*package))
    {
        if !found_packages.contains(package) {
            if selected_projects.len() == 1 {
                bail!("Dependency `{package}` was not found in the current project");
            }
            bail!("Dependency `{package}` was not found in selected workspace members");
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
    workspace: &Workspace,
    selected_projects: &[ProjectWorkspace],
    requirements: &[UpgradableRequirement],
    packages: &[PackageName],
    sources_strategy: &NoSources,
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

    for selected in requirements {
        if selected.package == selected.member {
            if selected_projects.len() == 1 {
                bail!(
                    "Dependency `{}` refers to the current project and cannot be upgraded",
                    selected.package
                );
            }
            bail!(
                "Dependency `{}` refers to workspace member `{}` and cannot be upgraded",
                selected.package,
                selected.member
            );
        }
    }

    for project in selected_projects {
        for package in packages {
            if sources_strategy.for_package(package) {
                continue;
            }
            let sources = project
                .current_project()
                .pyproject_toml()
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.sources.as_ref())
                .and_then(|sources| sources.inner().get(package))
                .or_else(|| workspace.sources().get(package));
            let applies_to_selected_requirement = |source| {
                requirements.iter().any(|selected| {
                    selected.member == *project.project_name()
                        && selected.package == *package
                        && source_is_applicable(source, &selected.contexts)
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

fn render_skipped_requirements(
    skipped: &[SkippedRequirement],
    display_members: bool,
    printer: Printer,
) -> Result<()> {
    for requirement in skipped {
        let reason = requirement.reason.message();
        if display_members {
            writeln!(
                printer.stderr(),
                "warning: Skipping dependency `{}` in package `{}` {}: `{}` ({reason})",
                requirement.package,
                requirement.member,
                requirement.dependency_type.toml_table_name(),
                requirement.display_text
            )?;
        } else {
            writeln!(
                printer.stderr(),
                "warning: Skipping dependency `{}` in {}: `{}` ({reason})",
                requirement.package,
                requirement.dependency_type.toml_table_name(),
                requirement.display_text
            )?;
        }
    }
    Ok(())
}

/// Return direct dependency declarations in the selected current project.
fn current_project_dependency_declarations(
    project: &ProjectWorkspace,
) -> Vec<DependencyDeclaration> {
    let mut declarations = Vec::new();
    let current_project = project.current_project();
    let project_metadata = current_project.project();
    if let Some(dependencies) = &project_metadata.dependencies {
        declarations.extend(
            dependencies
                .iter()
                .cloned()
                .map(|dependency| DependencyDeclaration {
                    dependency_type: DependencyType::Production,
                    text: dependency,
                    contexts: vec![DeclarationContext {
                        dependency_type: DependencyType::Production,
                        marker: MarkerTree::TRUE,
                    }],
                }),
        );
    }
    if let Some(optional_dependencies) = &project_metadata.optional_dependencies {
        for (extra, dependencies) in optional_dependencies {
            declarations.extend(dependencies.iter().cloned().map(|dependency| {
                DependencyDeclaration {
                    dependency_type: DependencyType::Optional(extra.clone()),
                    text: dependency,
                    contexts: vec![DeclarationContext {
                        dependency_type: DependencyType::Optional(extra.clone()),
                        marker: MarkerTree::TRUE,
                    }],
                }
            }));
        }
    }
    if let Some(dependency_groups) = &current_project.pyproject_toml().dependency_groups {
        for (group, dependencies) in dependency_groups {
            let contexts =
                dependency_group_appearance_contexts(current_project.pyproject_toml(), group);
            declarations.extend(dependencies.iter().filter_map(|dependency| {
                let DependencyGroupSpecifier::Requirement(dependency) = dependency else {
                    return None;
                };
                Some(DependencyDeclaration {
                    dependency_type: DependencyType::Group(group.clone()),
                    text: dependency.clone(),
                    contexts: contexts.clone(),
                })
            }));
        }
    }
    declarations
}

fn dependency_group_appearance_contexts(
    pyproject_toml: &WorkspacePyProjectToml,
    group: &GroupName,
) -> Vec<DeclarationContext> {
    let mut contexts = Vec::new();
    let mut parents = Vec::new();
    collect_group_appearance_contexts(
        pyproject_toml,
        group,
        dependency_group_requires_python_marker(pyproject_toml, group),
        &mut parents,
        &mut contexts,
    );
    contexts
}

fn collect_group_appearance_contexts(
    pyproject_toml: &WorkspacePyProjectToml,
    group: &GroupName,
    marker: MarkerTree,
    parents: &mut Vec<GroupName>,
    contexts: &mut Vec<DeclarationContext>,
) {
    if parents.contains(group) {
        return;
    }
    contexts.push(DeclarationContext {
        dependency_type: DependencyType::Group(group.clone()),
        marker,
    });
    parents.push(group.clone());
    if let Some(dependency_groups) = &pyproject_toml.dependency_groups {
        for (parent, dependencies) in dependency_groups {
            let includes_group = dependencies.iter().any(|dependency| {
                matches!(
                    dependency,
                    DependencyGroupSpecifier::IncludeGroup { include_group }
                        if include_group == group
                )
            });
            if !includes_group {
                continue;
            }
            let mut parent_marker = marker;
            parent_marker.and(dependency_group_requires_python_marker(
                pyproject_toml,
                parent,
            ));
            collect_group_appearance_contexts(
                pyproject_toml,
                parent,
                parent_marker,
                parents,
                contexts,
            );
        }
    }
    let _ = parents.pop();
}

fn dependency_group_requires_python_marker(
    pyproject_toml: &WorkspacePyProjectToml,
    group: &GroupName,
) -> MarkerTree {
    pyproject_toml
        .dependency_group_requires_python(group)
        .map_or(MarkerTree::TRUE, |requires_python| {
            RequiresPython::from_specifiers(requires_python.clone()).to_marker_tree()
        })
}

fn render_upgrade_report(
    outcomes: &[DeclarationOutcome],
    events: &[LockEvent<'_>],
    selected_packages: &[PackageName],
    display_members: bool,
    dry_run: DryRun,
    output_format: SyncFormat,
    printer: Printer,
) -> Result<()> {
    match output_format {
        SyncFormat::Text => {
            render_upgrade_report_text(
                outcomes,
                events,
                selected_packages,
                display_members,
                dry_run,
                printer,
            )?;
        }
        SyncFormat::Json => {
            render_declaration_outcomes(outcomes, display_members, printer)?;
            if let Some(output) =
                UpgradeReport::from_outcomes(outcomes, dry_run).format(output_format)
            {
                writeln!(printer.stdout_important(), "{output}")?;
            }
        }
    }
    Ok(())
}

fn render_upgrade_report_text(
    outcomes: &[DeclarationOutcome],
    events: &[LockEvent<'_>],
    selected_packages: &[PackageName],
    display_members: bool,
    dry_run: DryRun,
    printer: Printer,
) -> Result<()> {
    for package in selected_packages {
        if !outcomes
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
    render_declaration_outcomes(outcomes, display_members, printer)?;

    let action = if dry_run.enabled() {
        "Would update"
    } else {
        "Updated"
    };
    let mut rendered_updates = Vec::new();
    for outcome in outcomes {
        let DeclarationOutcome::Changed(update) = outcome else {
            continue;
        };
        let update = update.as_ref();
        if rendered_updates
            .iter()
            .any(|rendered: &&RequirementUpdate| {
                rendered.member == update.member
                    && rendered.dependency_type == update.dependency_type
                    && rendered.existing == update.existing
                    && rendered.replacement == update.replacement
            })
        {
            continue;
        }
        if display_members {
            writeln!(
                printer.stderr(),
                "{action} requirement in package `{}`: `{}` -> `{}`",
                update.member,
                update.original_text,
                update.replacement
            )?;
        } else {
            writeln!(
                printer.stderr(),
                "{action} requirement: `{}` -> `{}`",
                update.original_text,
                update.replacement
            )?;
        }
        rendered_updates.push(update);
    }

    Ok(())
}

fn render_declaration_outcomes(
    outcomes: &[DeclarationOutcome],
    display_members: bool,
    printer: Printer,
) -> Result<()> {
    for outcome in outcomes {
        match outcome {
            DeclarationOutcome::Blocked {
                declaration,
                reason,
            } => {
                if display_members {
                    writeln!(
                        printer.stderr(),
                        "warning: Could not update dependency `{}` in package `{}` {}: `{}` ({})",
                        declaration.package,
                        declaration.member,
                        declaration.dependency_type.toml_table_name(),
                        declaration.text,
                        reason.message()
                    )?;
                } else {
                    writeln!(
                        printer.stderr(),
                        "warning: Could not update dependency `{}` in {}: `{}` ({})",
                        declaration.package,
                        declaration.dependency_type.toml_table_name(),
                        declaration.text,
                        reason.message()
                    )?;
                }
            }
            DeclarationOutcome::Changed(_)
            | DeclarationOutcome::Unchanged(_)
            | DeclarationOutcome::Skipped(_) => {}
        }
    }
    Ok(())
}

/// Return the marker domain that the workspace will resolve for dependency declarations.
fn workspace_resolution_marker(
    workspace: &Workspace,
    fallback_interpreter: Option<&Interpreter>,
) -> Result<MarkerTree> {
    let target = LockTarget::from(workspace);
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
fn source_is_applicable(source: &Source, contexts: &[DeclarationContext]) -> bool {
    contexts.iter().any(|context| {
        source_origin_applies(source, context) && !source.marker().is_disjoint(context.marker)
    })
}

/// Return whether a source is scoped to the selected requirement declaration.
fn source_origin_applies(source: &Source, context: &DeclarationContext) -> bool {
    match &context.dependency_type {
        DependencyType::Production => {
            let extra = context.marker.top_level_extra_name();
            source
                .extra()
                .is_none_or(|target| extra.as_deref() == Some(target))
                && source.group().is_none()
        }
        DependencyType::Dev => source.extra().is_none() && source.group().is_none(),
        DependencyType::Optional(extra) => {
            source.extra().is_none_or(|target| target == extra) && source.group().is_none()
        }
        DependencyType::Group(group) => {
            source.extra().is_none() && source.group().is_none_or(|target| target == group)
        }
    }
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

//! Project Python requests and compatibility validation.

use std::path::Path;

use itertools::Itertools;
use tracing::debug;
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::DependencyGroupsWithDefaults;
use uv_distribution_types::RequiresPython;
use uv_fs::Simplified;
use uv_lock::Installable;
use uv_pep440::TildeVersionSpecifier;
use uv_python::{
    ConfigDiscovery, EnvironmentPreference, Interpreter, PythonArchitecture, PythonDownloads,
    PythonInstallation, PythonPreference, PythonRequest, PythonVersionFile,
    VersionFileDiscoveryOptions,
};
use uv_settings::PythonInstallMirrors;
use uv_warnings::warn_user_once;
use uv_workspace::{RequiresPythonDeclaration, RequiresPythonSources, Workspace};

use crate::commands::project::ProjectError;
use crate::commands::project::install_target::InstallTarget;
use crate::commands::reporters::PythonDownloadReporter;

/// An interpreter that satisfies the Python requirement used to select it.
///
/// Created by [`ProjectPythonRequest::validate`] after checking the workspace or frozen lockfile
/// requirement, including the selected dependency groups. Warning-only commands and existing
/// environments preserved by `--no-sync` do not use this type.
#[derive(Debug)]
pub(crate) struct CompatibleProjectPython(Interpreter);

impl CompatibleProjectPython {
    /// Consume the compatible interpreter for use by the environment or resolver APIs.
    pub(super) fn into_interpreter(self) -> Interpreter {
        self.0
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PythonRequestSource {
    /// The request was provided by the user.
    UserRequest,
    /// The request was inferred from a `.python-version` or `.python-versions` file.
    DotPythonVersion(PythonVersionFile),
    /// The request was inferred from a `pyproject.toml` file.
    RequiresPython,
}

impl std::fmt::Display for PythonRequestSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserRequest => write!(f, "explicit request"),
            Self::DotPythonVersion(file) => {
                write!(f, "version file at `{}`", file.path().user_display())
            }
            Self::RequiresPython => write!(f, "`requires-python` metadata"),
        }
    }
}

/// A Python requirement and the source used to derive it.
#[derive(Debug, Clone)]
struct ProjectPythonRequirement {
    requires_python: RequiresPython,
    source: PythonRequirementSource,
}

/// The resolved Python request and requirement for a workspace or frozen lockfile.
#[derive(Debug, Clone)]
pub(crate) struct ProjectPythonRequest {
    /// The source of the Python request.
    source: PythonRequestSource,
    /// The resolved Python request, computed by considering (1) any explicit request from the user
    /// via `--python`, (2) any implicit request from the user via `.python-version`, and (3) the
    /// workspace or lockfile's `Requires-Python` specifier.
    pub(crate) python_request: Option<PythonRequest>,
    /// The resolved Python requirement for the project and its source.
    requirement: Option<ProjectPythonRequirement>,
}

impl ProjectPythonRequest {
    /// Determine the Python request and requirement from a frozen lockfile.
    pub(super) async fn from_lockfile(
        python_request: Option<PythonRequest>,
        target: InstallTarget<'_>,
        groups: &DependencyGroupsWithDefaults,
        project_dir: &Path,
        config_discovery: ConfigDiscovery,
    ) -> Result<Self, ProjectError> {
        Self::from_requirements(
            python_request,
            Some(target.install_path()),
            Some(find_lockfile_requires_python(target, groups)?),
            project_dir,
            config_discovery,
        )
        .await
    }

    /// Determine the [`ProjectPythonRequest`] for the current [`Workspace`].
    pub(crate) async fn from_request(
        python_request: Option<PythonRequest>,
        workspace: Option<&Workspace>,
        groups: &DependencyGroupsWithDefaults,
        project_dir: &Path,
        config_discovery: ConfigDiscovery,
    ) -> Result<Self, ProjectError> {
        let requirement = workspace
            .map(|workspace| find_workspace_python_requirement(workspace, groups))
            .transpose()?
            .flatten();

        Self::from_requirements(
            python_request,
            workspace.map(|workspace| workspace.install_path().as_path()),
            requirement,
            project_dir,
            config_discovery,
        )
        .await
    }

    /// Select a Python request using a project's root and Python requirement.
    async fn from_requirements(
        python_request: Option<PythonRequest>,
        workspace_root: Option<&Path>,
        requirement: Option<ProjectPythonRequirement>,
        project_dir: &Path,
        config_discovery: ConfigDiscovery,
    ) -> Result<Self, ProjectError> {
        let (source, python_request) = if let Some(request) = python_request {
            // (1) Explicit request from user
            let source = PythonRequestSource::UserRequest;
            let request = Some(request);
            (source, request)
        } else if let Some(file) = PythonVersionFile::discover(
            project_dir,
            &VersionFileDiscoveryOptions::default()
                .with_stop_discovery_at(workspace_root)
                .with_config_discovery(config_discovery),
        )
        .await?
        .filter(|file| {
            // Ignore global version files that are incompatible with requires-python
            if !file.is_global() {
                return true;
            }
            match (file.version(), requirement.as_ref()) {
                (Some(request), Some(requirement)) => request
                    .as_pep440_version()
                    .is_none_or(|version| requirement.requires_python.contains(&version)),
                _ => true,
            }
        }) {
            // (2) Request from `.python-version`
            let source = PythonRequestSource::DotPythonVersion(file.clone());
            let request = file.version().cloned();
            (source, request)
        } else {
            // (3) `requires-python` in `pyproject.toml`
            let request = requirement.as_ref().and_then(|requirement| {
                PythonRequest::from_requires_python(&requirement.requires_python)
            });
            let source = PythonRequestSource::RequiresPython;
            (source, request)
        };

        if let Some(python_request) = python_request.as_ref() {
            debug!(
                "Using Python request `{}` from {source}",
                python_request.to_canonical_string()
            );
        }

        Ok(Self {
            source,
            python_request,
            requirement,
        })
    }

    pub(super) fn requires_python(&self) -> Option<&RequiresPython> {
        self.requirement
            .as_ref()
            .map(|requirement| &requirement.requires_python)
    }

    /// Check the interpreter against the stored project and selected group requirements.
    ///
    /// Unlike [`Self::validate`], this borrows the interpreter so warning-only commands can
    /// continue using it after an incompatibility.
    pub(crate) fn check(&self, interpreter: &Interpreter) -> Result<(), ProjectError> {
        let Some(requirement) = &self.requirement else {
            return Ok(());
        };
        validate_python_requirement(
            interpreter,
            &requirement.requires_python,
            &self.source,
            &requirement.source,
        )
    }

    /// Validate a discovered interpreter before accepting it for a new project environment.
    ///
    /// Discovery is responsible for matching the Python request; this checks the project and
    /// selected group requirements.
    pub(super) fn validate(
        &self,
        interpreter: Interpreter,
    ) -> Result<CompatibleProjectPython, ProjectError> {
        self.check(&interpreter)?;
        Ok(CompatibleProjectPython(interpreter))
    }

    /// Find or download an interpreter for the resolved request, then check project compatibility.
    ///
    /// Rejects an incompatible selection instead of searching for another interpreter.
    pub(super) async fn find_or_download(
        &self,
        environment_preference: EnvironmentPreference,
        python_preference: PythonPreference,
        python_arch: Option<PythonArchitecture>,
        python_downloads: PythonDownloads,
        client_builder: &BaseClientBuilder<'_>,
        cache: &Cache,
        reporter: &PythonDownloadReporter,
        install_mirrors: &PythonInstallMirrors,
    ) -> Result<CompatibleProjectPython, ProjectError> {
        let interpreter = PythonInstallation::find_or_download(
            self.python_request.as_ref(),
            environment_preference,
            python_preference,
            python_arch,
            python_downloads,
            client_builder,
            cache,
            Some(reporter),
            install_mirrors.python_install_mirror.as_deref(),
            install_mirrors.pypy_install_mirror.as_deref(),
            install_mirrors.python_downloads_json_url.as_deref(),
        )
        .await?
        .into_interpreter();
        self.validate(interpreter)
    }
}

/// Compute the `Requires-Python` bound for the [`Workspace`].
///
/// For a [`Workspace`] with multiple packages, the `Requires-Python` bound is the union of the
/// `Requires-Python` bounds of all the packages.
pub(crate) fn find_requires_python(
    workspace: &Workspace,
    groups: &DependencyGroupsWithDefaults,
) -> Result<Option<RequiresPython>, ProjectError> {
    Ok(find_workspace_python_requirement(workspace, groups)?
        .map(|requirement| requirement.requires_python))
}

/// Compute the workspace's Python requirement together with its contributing declarations.
///
/// Retain the declarations so incompatibility diagnostics use the same inputs as the requirement.
fn find_workspace_python_requirement(
    workspace: &Workspace,
    groups: &DependencyGroupsWithDefaults,
) -> Result<Option<ProjectPythonRequirement>, ProjectError> {
    let requires_python = workspace.requires_python(groups)?;
    // If there are no `Requires-Python` specifiers in the workspace, return `None`.
    if requires_python.is_empty() {
        return Ok(None);
    }
    for (source, specifiers) in &requires_python {
        if let [spec] = &specifiers[..] {
            if let Some(spec) = TildeVersionSpecifier::from_specifier_ref(spec) {
                if spec.has_patch() {
                    continue;
                }
                let (lower, upper) = spec.bounding_specifiers();
                let spec_0 = spec.with_patch_version(0);
                let (lower_0, upper_0) = spec_0.bounding_specifiers();
                warn_user_once!(
                    "The `requires-python` specifier (`{spec}`) in `{source}` \
                    uses the tilde specifier (`~=`) without a patch version. This will be \
                    interpreted as `{lower}, {upper}`. Did you mean `{spec_0}` to constrain the \
                    version as `{lower_0}, {upper_0}`? We recommend only using \
                    the tilde specifier with a patch version to avoid ambiguity.",
                );
            }
        }
    }
    match RequiresPython::intersection(requires_python.iter().map(|(.., specifiers)| specifiers)) {
        Some(intersection) => Ok(Some(ProjectPythonRequirement {
            requires_python: intersection,
            source: PythonRequirementSource::Workspace {
                sources: requires_python,
                multiple_members: workspace.packages().len() > 1,
            },
        })),
        None => Err(ProjectError::DisjointRequiresPython(requires_python)),
    }
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

/// The requirements that exclude a Python version, and where they were read.
///
/// Formats as an optional suffix to a Python incompatibility diagnostic.
#[derive(Debug)]
pub(crate) enum PythonRequirementConflicts {
    Workspace {
        sources: RequiresPythonSources,
        /// Whether the workspace has multiple members, so a single-conflict diagnostic names the member.
        multiple_members: bool,
    },
    Lockfile {
        locked: Option<RequiresPython>,
        groups: RequiresPythonSources,
    },
}

impl std::fmt::Display for PythonRequirementConflicts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Workspace {
                sources,
                multiple_members,
            } => {
                if sources.len() > 1 {
                    return write!(
                        f,
                        ".\nThe following `requires-python` declarations do not permit this version:\n{}",
                        format_requires_python_sources(sources)
                    );
                }
                if let Some((RequiresPythonDeclaration::Workspace(group), _)) =
                    sources.iter().next()
                {
                    return write!(
                        f,
                        " (from the workspace root's `tool.uv.dependency-groups.{group}.requires-python`)."
                    );
                }
                if let Some((RequiresPythonDeclaration::Member(package, group), _)) =
                    sources.iter().next()
                {
                    if let Some(group) = group {
                        if *multiple_members {
                            return write!(
                                f,
                                " (from workspace member `{package}`'s `tool.uv.dependency-groups.{group}.requires-python`)."
                            );
                        }
                        return write!(
                            f,
                            " (from `tool.uv.dependency-groups.{group}.requires-python`)."
                        );
                    }
                    if *multiple_members {
                        return write!(
                            f,
                            " (from workspace member `{package}`'s `project.requires-python`)."
                        );
                    }
                    return f.write_str(" (from `project.requires-python`)");
                }
                Ok(())
            }
            Self::Lockfile { locked, groups } => {
                let count = usize::from(locked.is_some()) + groups.len();
                if count > 1 {
                    write!(
                        f,
                        ".\nThe following requirements in `uv.lock` do not permit this version:\n"
                    )?;
                    if let Some(locked) = locked {
                        writeln!(f, "- lockfile: {locked}")?;
                    }
                    return f.write_str(&format_requires_python_sources(groups));
                }
                if locked.is_some() {
                    return f.write_str(" (from `requires-python` in `uv.lock`).");
                }
                if let Some((source, _)) = groups.iter().next() {
                    return write!(f, " (from `{source}` in `uv.lock`).");
                }
                f.write_str(" (from `uv.lock`).")
            }
        }
    }
}

/// The declarations used to compute a project's Python requirement.
#[derive(Debug, Clone)]
pub(super) enum PythonRequirementSource {
    /// The requirement was derived from the workspace manifests.
    Workspace {
        sources: RequiresPythonSources,
        multiple_members: bool,
    },
    /// The lockfile's overall requirement and the selected groups' requirements.
    Lockfile {
        locked: RequiresPython,
        groups: RequiresPythonSources,
    },
}

/// Returns an error if the [`Interpreter`] does not satisfy `requires_python`.
///
/// The requirement source determines which conflicting declarations are included in the diagnostic.
pub(super) fn validate_python_requirement(
    interpreter: &Interpreter,
    requires_python: &RequiresPython,
    source: &PythonRequestSource,
    requirement_source: &PythonRequirementSource,
) -> Result<(), ProjectError> {
    if requires_python.contains(interpreter.python_version()) {
        return Ok(());
    }

    let conflicting_requires = match requirement_source {
        PythonRequirementSource::Workspace {
            sources,
            multiple_members,
        } => {
            let sources = sources
                .iter()
                .filter(|(.., requires)| !requires.contains(interpreter.python_version()))
                .map(|(key, requires)| (key.clone(), requires.clone()))
                .collect();
            PythonRequirementConflicts::Workspace {
                sources,
                multiple_members: *multiple_members,
            }
        }
        PythonRequirementSource::Lockfile { locked, groups } => {
            let version = interpreter.python_version().only_release();
            let groups = groups
                .iter()
                .filter(|(_, requires)| !requires.contains(&version))
                .map(|(key, requires)| (key.clone(), requires.clone()))
                .collect();
            PythonRequirementConflicts::Lockfile {
                locked: (!locked.contains(interpreter.python_version())).then(|| locked.clone()),
                groups,
            }
        }
    };

    match source {
        PythonRequestSource::UserRequest => {
            Err(ProjectError::RequestedPythonProjectIncompatibility(
                interpreter.python_version().clone(),
                requires_python.clone(),
                Box::new(conflicting_requires),
            ))
        }
        PythonRequestSource::DotPythonVersion(file) => {
            Err(ProjectError::DotPythonVersionProjectIncompatibility {
                python_request: file.path().user_display().to_string(),
                version: interpreter.python_version().clone(),
                requires_python: requires_python.clone(),
                requires_python_sources: Box::new(conflicting_requires),
            })
        }
        PythonRequestSource::RequiresPython => {
            Err(ProjectError::RequiresPythonProjectIncompatibility(
                interpreter.python_version().clone(),
                requires_python.clone(),
                Box::new(conflicting_requires),
            ))
        }
    }
}

pub(super) fn format_requires_python_sources(conflicts: &RequiresPythonSources) -> String {
    conflicts
        .iter()
        .map(|(source, specifiers)| format!("- {source}: {specifiers}"))
        .join("\n")
}

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use itertools::Either;

use uv_configuration::SourceStrategy;
use uv_distribution::LoweredRequirement;
use uv_distribution_types::{Index, IndexLocations};
use uv_normalize::{GroupName, PackageName};
use uv_pep508::RequirementOrigin;
use uv_pypi_types::{Conflicts, Requirement, SupportedEnvironments, VerbatimParsedUrl};
use uv_resolver::{Lock, LockVersion, RequiresPython, VERSION};
use uv_scripts::Pep723Script;
use uv_workspace::dependency_groups::DependencyGroupError;
use uv_workspace::{Workspace, WorkspaceMember};

use crate::commands::project::{find_requires_python, ProjectError};

/// A target that can be resolved into a lockfile.
#[derive(Debug, Copy, Clone)]
pub(crate) enum LockTarget<'lock> {
    Workspace(&'lock Workspace),
    Script(&'lock Pep723Script),
}

impl<'lock> From<&'lock Workspace> for LockTarget<'lock> {
    fn from(workspace: &'lock Workspace) -> Self {
        Self::Workspace(workspace)
    }
}

impl<'lock> From<&'lock Pep723Script> for LockTarget<'lock> {
    fn from(script: &'lock Pep723Script) -> Self {
        LockTarget::Script(script)
    }
}

impl<'lock> LockTarget<'lock> {
    /// Return the set of requirements that are attached to the target directly, as opposed to being
    /// attached to any members within the target.
    pub(crate) fn requirements(self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        match self {
            Self::Workspace(workspace) => workspace.requirements(),
            Self::Script(script) => script.metadata.dependencies.clone().unwrap_or_default(),
        }
    }

    /// Returns the set of overrides for the [`LockTarget`].
    pub(crate) fn overrides(self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        match self {
            Self::Workspace(workspace) => workspace.overrides(),
            Self::Script(script) => script
                .metadata
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.override_dependencies.as_ref())
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
        }
    }

    /// Returns the set of constraints for the [`LockTarget`].
    pub(crate) fn constraints(self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        match self {
            Self::Workspace(workspace) => workspace.constraints(),
            Self::Script(script) => script
                .metadata
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.constraint_dependencies.as_ref())
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
        }
    }

    /// Returns the set of build constraints for the [`LockTarget`].
    pub(crate) fn build_constraints(self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        match self {
            Self::Workspace(workspace) => workspace.build_constraints(),
            Self::Script(script) => script
                .metadata
                .tool
                .as_ref()
                .and_then(|tool| tool.uv.as_ref())
                .and_then(|uv| uv.build_constraint_dependencies.as_ref())
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
        }
    }

    /// Return the dependency groups that are attached to the target directly, as opposed to being
    /// attached to any members within the target.
    pub(crate) fn dependency_groups(
        self,
    ) -> Result<
        BTreeMap<GroupName, Vec<uv_pep508::Requirement<VerbatimParsedUrl>>>,
        DependencyGroupError,
    > {
        match self {
            Self::Workspace(workspace) => workspace.dependency_groups(),
            Self::Script(_) => Ok(BTreeMap::new()),
        }
    }

    /// Returns the set of all members within the target.
    pub(crate) fn members_requirements(self) -> impl Iterator<Item = Requirement> + 'lock {
        match self {
            Self::Workspace(workspace) => Either::Left(workspace.members_requirements()),
            Self::Script(_) => Either::Right(std::iter::empty()),
        }
    }

    /// Returns the set of all dependency groups within the target.
    pub(crate) fn group_requirements(self) -> impl Iterator<Item = Requirement> + 'lock {
        match self {
            Self::Workspace(workspace) => Either::Left(workspace.group_requirements()),
            Self::Script(_) => Either::Right(std::iter::empty()),
        }
    }

    /// Return the list of members to include in the [`Lock`].
    pub(crate) fn members(self) -> Vec<PackageName> {
        match self {
            Self::Workspace(workspace) => {
                let mut members = workspace.packages().keys().cloned().collect::<Vec<_>>();
                members.sort();

                // If this is a non-virtual project with a single member, we can omit it from the lockfile.
                // If any members are added or removed, it will inherently mismatch. If the member is
                // renamed, it will also mismatch.
                if members.len() == 1 && !workspace.is_non_project() {
                    members.clear();
                }

                members
            }
            Self::Script(_) => Vec::new(),
        }
    }

    /// Return the list of packages.
    pub(crate) fn packages(self) -> &'lock BTreeMap<PackageName, WorkspaceMember> {
        match self {
            Self::Workspace(workspace) => workspace.packages(),
            Self::Script(_) => {
                static EMPTY: BTreeMap<PackageName, WorkspaceMember> = BTreeMap::new();
                &EMPTY
            }
        }
    }

    /// Returns the set of supported environments for the [`LockTarget`].
    pub(crate) fn environments(self) -> Option<&'lock SupportedEnvironments> {
        match self {
            Self::Workspace(workspace) => workspace.environments(),
            Self::Script(_) => {
                // TODO(charlie): Add support for environments in scripts.
                None
            }
        }
    }

    /// Returns the set of required platforms for the [`LockTarget`].
    pub(crate) fn required_environments(self) -> Option<&'lock SupportedEnvironments> {
        match self {
            Self::Workspace(workspace) => workspace.required_environments(),
            Self::Script(_) => {
                // TODO(charlie): Add support for environments in scripts.
                None
            }
        }
    }

    /// Returns the set of conflicts for the [`LockTarget`].
    pub(crate) fn conflicts(self) -> Conflicts {
        match self {
            Self::Workspace(workspace) => workspace.conflicts(),
            Self::Script(_) => Conflicts::empty(),
        }
    }

    /// Return an iterator over the [`Index`] definitions in the [`LockTarget`].
    pub(crate) fn indexes(self) -> impl Iterator<Item = &'lock Index> {
        match self {
            Self::Workspace(workspace) => Either::Left(workspace.indexes().iter().chain(
                workspace.packages().values().flat_map(|member| {
                    member
                        .pyproject_toml()
                        .tool
                        .as_ref()
                        .and_then(|tool| tool.uv.as_ref())
                        .and_then(|uv| uv.index.as_ref())
                        .into_iter()
                        .flatten()
                }),
            )),
            Self::Script(script) => Either::Right(
                script
                    .metadata
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.top_level.index.as_deref())
                    .into_iter()
                    .flatten(),
            ),
        }
    }

    /// Return the `Requires-Python` bound for the [`LockTarget`].
    #[allow(clippy::result_large_err)]
    pub(crate) fn requires_python(self) -> Result<Option<RequiresPython>, ProjectError> {
        match self {
            Self::Workspace(workspace) => find_requires_python(workspace),
            Self::Script(script) => Ok(script
                .metadata
                .requires_python
                .as_ref()
                .map(RequiresPython::from_specifiers)),
        }
    }

    /// Return the path to the lock root.
    pub(crate) fn install_path(self) -> &'lock Path {
        match self {
            Self::Workspace(workspace) => workspace.install_path(),
            Self::Script(script) => script.path.parent().unwrap(),
        }
    }

    /// Return the path to the lockfile.
    pub(crate) fn lock_path(self) -> PathBuf {
        match self {
            // `uv.lock`
            Self::Workspace(workspace) => workspace.install_path().join("uv.lock"),
            // `script.py.lock`
            Self::Script(script) => {
                let mut file_name = match script.path.file_name() {
                    Some(f) => f.to_os_string(),
                    None => panic!("Script path has no file name"),
                };
                file_name.push(".lock");
                script.path.with_file_name(file_name)
            }
        }
    }

    /// Read the lockfile from the workspace.
    ///
    /// Returns `Ok(None)` if the lockfile does not exist.
    pub(crate) async fn read(self) -> Result<Option<Lock>, ProjectError> {
        match fs_err::tokio::read_to_string(self.lock_path()).await {
            Ok(encoded) => {
                match toml::from_str::<Lock>(&encoded) {
                    Ok(lock) => {
                        // If the lockfile uses an unsupported version, raise an error.
                        if lock.version() != VERSION {
                            return Err(ProjectError::UnsupportedLockVersion(
                                VERSION,
                                lock.version(),
                            ));
                        }
                        Ok(Some(lock))
                    }
                    Err(err) => {
                        // If we failed to parse the lockfile, determine whether it's a supported
                        // version.
                        if let Ok(lock) = toml::from_str::<LockVersion>(&encoded) {
                            if lock.version() != VERSION {
                                return Err(ProjectError::UnparsableLockVersion(
                                    VERSION,
                                    lock.version(),
                                    err,
                                ));
                            }
                        }
                        Err(ProjectError::UvLockParse(err))
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Read the lockfile from the workspace as bytes.
    pub(crate) async fn read_bytes(self) -> Result<Option<Vec<u8>>, std::io::Error> {
        match fs_err::tokio::read(self.lock_path()).await {
            Ok(encoded) => Ok(Some(encoded)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Write the lockfile to disk.
    pub(crate) async fn commit(self, lock: &Lock) -> Result<(), ProjectError> {
        let encoded = lock.to_toml()?;
        fs_err::tokio::write(self.lock_path(), encoded).await?;
        Ok(())
    }

    /// Lower the requirements for the [`LockTarget`], relative to the target root.
    pub(crate) fn lower(
        self,
        requirements: Vec<uv_pep508::Requirement<VerbatimParsedUrl>>,
        locations: &IndexLocations,
        sources: SourceStrategy,
    ) -> Result<Vec<Requirement>, uv_distribution::MetadataError> {
        match self {
            Self::Workspace(workspace) => {
                let name = workspace
                    .pyproject_toml()
                    .project
                    .as_ref()
                    .map(|project| project.name.clone());

                // We model these as `build-requires`, since, like build requirements, it doesn't define extras
                // or dependency groups.
                let metadata = uv_distribution::BuildRequires::from_workspace(
                    uv_pypi_types::BuildRequires {
                        name,
                        requires_dist: requirements,
                    },
                    workspace,
                    locations,
                    sources,
                )?;

                Ok(metadata
                    .requires_dist
                    .into_iter()
                    .map(|requirement| requirement.with_origin(RequirementOrigin::Workspace))
                    .collect::<Vec<_>>())
            }
            Self::Script(script) => {
                // Collect any `tool.uv.index` from the script.
                let empty = Vec::default();
                let indexes = match sources {
                    SourceStrategy::Enabled => script
                        .metadata
                        .tool
                        .as_ref()
                        .and_then(|tool| tool.uv.as_ref())
                        .and_then(|uv| uv.top_level.index.as_deref())
                        .unwrap_or(&empty),
                    SourceStrategy::Disabled => &empty,
                };

                // Collect any `tool.uv.sources` from the script.
                let empty = BTreeMap::default();
                let sources = match sources {
                    SourceStrategy::Enabled => script
                        .metadata
                        .tool
                        .as_ref()
                        .and_then(|tool| tool.uv.as_ref())
                        .and_then(|uv| uv.sources.as_ref())
                        .unwrap_or(&empty),
                    SourceStrategy::Disabled => &empty,
                };

                Ok(requirements
                    .into_iter()
                    .flat_map(|requirement| {
                        let requirement_name = requirement.name.clone();
                        LoweredRequirement::from_non_workspace_requirement(
                            requirement,
                            script.path.parent().unwrap(),
                            sources,
                            indexes,
                            locations,
                        )
                        .map(move |requirement| match requirement {
                            Ok(requirement) => Ok(requirement.into_inner()),
                            Err(err) => Err(uv_distribution::MetadataError::LoweringError(
                                requirement_name.clone(),
                                Box::new(err),
                            )),
                        })
                    })
                    .collect::<Result<_, _>>()?)
            }
        }
    }
}

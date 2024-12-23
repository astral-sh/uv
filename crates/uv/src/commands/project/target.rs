use itertools::{Either, Itertools};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use uv_configuration::{LowerBound, SourceStrategy};
use uv_distribution::LoweredRequirement;
use uv_distribution_types::IndexLocations;
use uv_normalize::PackageName;
use uv_pep508::RequirementOrigin;
use uv_pypi_types::{Conflicts, Requirement, SupportedEnvironments, VerbatimParsedUrl};
use uv_resolver::{Lock, LockVersion, RequiresPython, VERSION};
use uv_scripts::Pep723Script;
use uv_workspace::dependency_groups::DependencyGroupError;
use uv_workspace::{Workspace, WorkspaceMember};

use crate::commands::project::{find_requires_python, ProjectError};

#[derive(Debug, Copy, Clone)]
pub(crate) enum LockTarget<'lock> {
    Workspace(&'lock Workspace),
    Script(&'lock Pep723Script),
}

impl<'lock> From<&'lock Workspace> for LockTarget<'lock> {
    fn from(workspace: &'lock Workspace) -> Self {
        LockTarget::Workspace(workspace)
    }
}

impl<'lock> From<&'lock Pep723Script> for LockTarget<'lock> {
    fn from(script: &'lock Pep723Script) -> Self {
        LockTarget::Script(script)
    }
}

impl<'lock> LockTarget<'lock> {
    /// Return the path to the lockfile.
    pub(crate) fn lock_path(&self) -> PathBuf {
        match self {
            // `uv.lock`
            LockTarget::Workspace(workspace) => workspace.install_path().join("uv.lock"),
            // `script.py.lock`
            LockTarget::Script(script) => {
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
    pub(crate) async fn read(&self) -> Result<Option<Lock>, ProjectError> {
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
    pub(crate) async fn read_bytes(&self) -> Result<Option<Vec<u8>>, ProjectError> {
        match fs_err::tokio::read(self.lock_path()).await {
            Ok(encoded) => Ok(Some(encoded)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Write the lockfile to disk.
    pub(crate) async fn commit(&self, lock: &Lock) -> Result<(), ProjectError> {
        let encoded = lock.to_toml()?;
        fs_err::tokio::write(self.lock_path(), encoded).await?;
        Ok(())
    }

    /// Returns `true` if the lockfile exists.
    pub(crate) fn exists(&self) -> bool {
        self.lock_path().exists()
    }

    pub(crate) fn non_project_requirements(
        &self,
    ) -> Result<Vec<uv_pep508::Requirement<VerbatimParsedUrl>>, DependencyGroupError> {
        match self {
            LockTarget::Workspace(workspace) => workspace.non_project_requirements(),
            LockTarget::Script(script) => {
                Ok(script.metadata.dependencies.clone().unwrap_or_default())
            }
        }
    }

    pub(crate) fn overrides(&self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        match self {
            LockTarget::Workspace(workspace) => workspace.overrides(),
            LockTarget::Script(script) => script
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

    pub(crate) fn constraints(&self) -> Vec<uv_pep508::Requirement<VerbatimParsedUrl>> {
        match self {
            LockTarget::Workspace(workspace) => workspace.constraints(),
            LockTarget::Script(script) => script
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

    pub(crate) fn lower(
        &self,
        requirements: Vec<uv_pep508::Requirement<VerbatimParsedUrl>>,
        index_locations: &IndexLocations,
        sources: SourceStrategy,
    ) -> Result<Vec<Requirement>, uv_distribution::MetadataError> {
        match self {
            LockTarget::Workspace(workspace) => {
                lower(requirements, workspace, index_locations, sources)
            }
            LockTarget::Script(script) => {
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
                            index_locations,
                            LowerBound::Allow,
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

    pub(crate) fn members(&self) -> Vec<PackageName> {
        match self {
            LockTarget::Workspace(workspace) => {
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
            LockTarget::Script(script) => Vec::new(),
        }
    }

    pub(crate) fn packages(&self) -> &BTreeMap<PackageName, WorkspaceMember> {
        match self {
            LockTarget::Workspace(workspace) => workspace.packages(),
            LockTarget::Script(_) => {
                static EMPTY: BTreeMap<PackageName, WorkspaceMember> = BTreeMap::new();
                &EMPTY
            }
        }
    }

    pub(crate) fn environments(&self) -> Option<&SupportedEnvironments> {
        match self {
            LockTarget::Workspace(workspace) => workspace.environments(),
            LockTarget::Script(script) => {
                // TODO(charlie): Add support for environments in scripts.
                None
            }
        }
    }

    pub(crate) fn conflicts(&self) -> Conflicts {
        match self {
            LockTarget::Workspace(workspace) => workspace.conflicts(),
            LockTarget::Script(_) => Conflicts::empty(),
        }
    }

    pub(crate) fn requires_python(&self) -> Option<RequiresPython> {
        match self {
            LockTarget::Workspace(workspace) => find_requires_python(workspace),
            LockTarget::Script(script) => script
                .metadata
                .requires_python
                .as_ref()
                .map(RequiresPython::from_specifiers),
        }
    }

    pub(crate) fn members_requirements(&self) -> impl Iterator<Item = Requirement> + '_ {
        match self {
            LockTarget::Workspace(workspace) => Either::Left(workspace.members_requirements()),
            LockTarget::Script(script) => Either::Right(
                script
                    .metadata
                    .dependencies
                    .iter()
                    .flatten()
                    .cloned()
                    .map(Requirement::from),
            ),
        }
    }

    pub(crate) fn group_requirements(&self) -> impl Iterator<Item = Requirement> + '_ {
        match self {
            LockTarget::Workspace(workspace) => Either::Left(workspace.group_requirements()),
            LockTarget::Script(_) => Either::Right(std::iter::empty()),
        }
    }

    pub(crate) fn install_path(&self) -> &Path {
        match self {
            LockTarget::Workspace(workspace) => workspace.install_path(),
            LockTarget::Script(script) => script.path.parent().unwrap(),
        }
    }
}

/// Lower a set of requirements, relative to the workspace root.
fn lower(
    requirements: Vec<uv_pep508::Requirement<VerbatimParsedUrl>>,
    workspace: &Workspace,
    locations: &IndexLocations,
    sources: SourceStrategy,
) -> Result<Vec<Requirement>, uv_distribution::MetadataError> {
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
        LowerBound::Warn,
    )?;

    Ok(metadata
        .requires_dist
        .into_iter()
        .map(|requirement| requirement.with_origin(RequirementOrigin::Workspace))
        .collect::<Vec<_>>())
}

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::str::FromStr;

use thiserror::Error;
use tracing::error;

use uv_distribution_types::RequiresPython;
use uv_normalize::{DEV_DEPENDENCIES, GroupName};
use uv_pep440::VersionSpecifiers;
use uv_pep508::Pep508Error;
use uv_pypi_types::{DependencyGroupSpecifier, VerbatimParsedUrl};

use crate::pyproject::{DependencyGroupSettings, PyProjectToml, ToolUvDependencyGroups};

/// PEP 735 dependency groups, with any `include-group` entries resolved.
#[derive(Debug, Default, Clone)]
pub struct FlatDependencyGroups(BTreeMap<GroupName, FlatDependencyGroup>);

#[derive(Debug, Default, Clone)]
pub struct FlatDependencyGroup {
    pub requirements: Vec<uv_pep508::Requirement<VerbatimParsedUrl>>,
    pub requires_python: Option<VersionSpecifiers>,
}

impl FlatDependencyGroups {
    pub fn from_pyproject_toml(
        pyproject_toml: &PyProjectToml,
    ) -> Result<Self, DependencyGroupError> {
        // Otherwise, return the dependency groups in the non-project workspace root.
        // First, collect `tool.uv.dev_dependencies`
        let dev_dependencies = pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.dev_dependencies.as_ref());

        // Then, collect `dependency-groups`
        let dependency_groups = pyproject_toml
            .dependency_groups
            .iter()
            .flatten()
            .collect::<BTreeMap<_, _>>();

        // Get additional settings
        let empty_settings = ToolUvDependencyGroups::default();
        let group_settings = pyproject_toml
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.dependency_groups.as_ref())
            .unwrap_or(&empty_settings);

        // Flatten the dependency groups.
        let mut dependency_groups = FlatDependencyGroups::from_dependency_groups(
            &dependency_groups,
            group_settings.inner(),
        )
        .map_err(|err| err.with_dev_dependencies(dev_dependencies))?;

        // Add the `dev` group, if `dev-dependencies` is defined.
        if let Some(dev_dependencies) = dev_dependencies {
            dependency_groups
                .entry(DEV_DEPENDENCIES.clone())
                .or_insert_with(FlatDependencyGroup::default)
                .requirements
                .extend(dev_dependencies.clone());
        }

        Ok(dependency_groups)
    }

    /// Resolve the dependency groups (which may contain references to other groups) into concrete
    /// lists of requirements.
    fn from_dependency_groups(
        groups: &BTreeMap<&GroupName, &Vec<DependencyGroupSpecifier>>,
        settings: &BTreeMap<GroupName, DependencyGroupSettings>,
    ) -> Result<Self, DependencyGroupError> {
        fn resolve_group<'data>(
            resolved: &mut BTreeMap<GroupName, FlatDependencyGroup>,
            groups: &'data BTreeMap<&GroupName, &Vec<DependencyGroupSpecifier>>,
            settings: &BTreeMap<GroupName, DependencyGroupSettings>,
            name: &'data GroupName,
            parents: &mut Vec<&'data GroupName>,
        ) -> Result<(), DependencyGroupError> {
            let Some(specifiers) = groups.get(name) else {
                // Missing group
                let parent_name = parents
                    .iter()
                    .last()
                    .copied()
                    .expect("parent when group is missing");
                return Err(DependencyGroupError::GroupNotFound(
                    name.clone(),
                    parent_name.clone(),
                ));
            };

            // "Dependency Group Includes MUST NOT include cycles, and tools SHOULD report an error if they detect a cycle."
            if parents.contains(&name) {
                return Err(DependencyGroupError::DependencyGroupCycle(Cycle(
                    parents.iter().copied().cloned().collect(),
                )));
            }

            // If we already resolved this group, short-circuit.
            if resolved.contains_key(name) {
                return Ok(());
            }

            parents.push(name);
            let mut requirements = Vec::with_capacity(specifiers.len());
            let mut full_requires_python = None;
            for specifier in *specifiers {
                match specifier {
                    DependencyGroupSpecifier::Requirement(requirement) => {
                        match uv_pep508::Requirement::<VerbatimParsedUrl>::from_str(requirement) {
                            Ok(requirement) => requirements.push(requirement),
                            Err(err) => {
                                return Err(DependencyGroupError::GroupParseError(
                                    name.clone(),
                                    requirement.clone(),
                                    Box::new(err),
                                ));
                            }
                        }
                    }
                    DependencyGroupSpecifier::IncludeGroup { include_group } => {
                        resolve_group(resolved, groups, settings, include_group, parents)?;
                        if let Some(included) = resolved.get(include_group) {
                            requirements.extend(included.requirements.iter().cloned());
                            let versions = full_requires_python
                                .into_iter()
                                .flatten()
                                .chain(included.requires_python.clone().into_iter().flatten())
                                .collect::<Vec<_>>();
                            let new_requires_python = VersionSpecifiers::from_iter(versions);
                            full_requires_python =
                                (!new_requires_python.is_empty()).then_some(new_requires_python);
                        }
                    }
                    DependencyGroupSpecifier::Object(map) => {
                        return Err(DependencyGroupError::DependencyObjectSpecifierNotSupported(
                            name.clone(),
                            map.clone(),
                        ));
                    }
                }
            }

            let empty_settings = DependencyGroupSettings::default();
            let DependencyGroupSettings { requires_python } =
                settings.get(name).unwrap_or(&empty_settings);
            if let Some(requires_python) = requires_python {
                // Merge in this requires-python to the full flattened version
                let versions = full_requires_python
                    .into_iter()
                    .flatten()
                    .chain(requires_python.clone())
                    .collect::<Vec<_>>();
                let new_requires_python = VersionSpecifiers::from_iter(versions);
                full_requires_python =
                    (!new_requires_python.is_empty()).then_some(new_requires_python);
                // Merge in this requires-python to every requirement of this group
                // We don't need to merge the full_requires_python because included
                // groups already applied theirs.
                for requirement in &mut requirements {
                    let extra_markers =
                        RequiresPython::from_specifiers(requires_python).to_marker_tree();
                    requirement.marker.and(extra_markers);
                }
            }

            parents.pop();

            resolved.insert(
                name.clone(),
                FlatDependencyGroup {
                    requirements,
                    requires_python: full_requires_python,
                },
            );
            Ok(())
        }

        let mut resolved = BTreeMap::new();
        for name in groups.keys() {
            let mut parents = Vec::new();
            resolve_group(&mut resolved, groups, settings, name, &mut parents)?;
        }
        Ok(Self(resolved))
    }

    /// Return the requirements for a given group, if any.
    pub fn get(&self, group: &GroupName) -> Option<&FlatDependencyGroup> {
        self.0.get(group)
    }

    /// Return the entry for a given group, if any.
    pub fn entry(&mut self, group: GroupName) -> Entry<GroupName, FlatDependencyGroup> {
        self.0.entry(group)
    }

    /// Consume the [`FlatDependencyGroups`] and return the inner map.
    pub fn into_inner(self) -> BTreeMap<GroupName, FlatDependencyGroup> {
        self.0
    }
}

impl FromIterator<(GroupName, FlatDependencyGroup)> for FlatDependencyGroups {
    fn from_iter<T: IntoIterator<Item = (GroupName, FlatDependencyGroup)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl IntoIterator for FlatDependencyGroups {
    type Item = (GroupName, FlatDependencyGroup);
    type IntoIter = std::collections::btree_map::IntoIter<GroupName, FlatDependencyGroup>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Debug, Error)]
pub enum DependencyGroupError {
    #[error("Failed to parse entry in group `{0}`: `{1}`")]
    GroupParseError(
        GroupName,
        String,
        #[source] Box<Pep508Error<VerbatimParsedUrl>>,
    ),
    #[error("Failed to find group `{0}` included by `{1}`")]
    GroupNotFound(GroupName, GroupName),
    #[error(
        "Group `{0}` includes the `dev` group (`include = \"dev\"`), but only `tool.uv.dev-dependencies` was found. To reference the `dev` group via an `include`, remove the `tool.uv.dev-dependencies` section and add any development dependencies to the `dev` entry in the `[dependency-groups]` table instead."
    )]
    DevGroupInclude(GroupName),
    #[error("Detected a cycle in `dependency-groups`: {0}")]
    DependencyGroupCycle(Cycle),
    #[error("Group `{0}` contains an unknown dependency object specifier: {1:?}")]
    DependencyObjectSpecifierNotSupported(GroupName, BTreeMap<String, String>),
}

impl DependencyGroupError {
    /// Enrich a [`DependencyGroupError`] with the `tool.uv.dev-dependencies` metadata, if applicable.
    #[must_use]
    pub fn with_dev_dependencies(
        self,
        dev_dependencies: Option<&Vec<uv_pep508::Requirement<VerbatimParsedUrl>>>,
    ) -> Self {
        match self {
            DependencyGroupError::GroupNotFound(group, parent)
                if dev_dependencies.is_some() && group == *DEV_DEPENDENCIES =>
            {
                DependencyGroupError::DevGroupInclude(parent)
            }
            _ => self,
        }
    }
}

/// A cycle in the `dependency-groups` table.
#[derive(Debug)]
pub struct Cycle(Vec<GroupName>);

/// Display a cycle, e.g., `a -> b -> c -> a`.
impl std::fmt::Display for Cycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let [first, rest @ ..] = self.0.as_slice() else {
            return Ok(());
        };
        write!(f, "`{first}`")?;
        for group in rest {
            write!(f, " -> `{group}`")?;
        }
        write!(f, " -> `{first}`")?;
        Ok(())
    }
}

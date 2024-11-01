use std::collections::BTreeMap;
use std::str::FromStr;

use thiserror::Error;
use tracing::warn;

use uv_normalize::GroupName;
use uv_pep508::Pep508Error;
use uv_pypi_types::VerbatimParsedUrl;

use crate::pyproject::DependencyGroupSpecifier;

/// PEP 735 dependency groups, with any `include-group` entries resolved.
#[derive(Debug, Clone)]
pub struct FlatDependencyGroups(
    BTreeMap<GroupName, Vec<uv_pep508::Requirement<VerbatimParsedUrl>>>,
);

impl FlatDependencyGroups {
    /// Resolve the dependency groups (which may contain references to other groups) into concrete
    /// lists of requirements.
    pub fn from_dependency_groups(
        groups: &BTreeMap<&GroupName, &Vec<DependencyGroupSpecifier>>,
    ) -> Result<Self, DependencyGroupError> {
        fn resolve_group<'data>(
            resolved: &mut BTreeMap<GroupName, Vec<uv_pep508::Requirement<VerbatimParsedUrl>>>,
            groups: &'data BTreeMap<&GroupName, &Vec<DependencyGroupSpecifier>>,
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
                        resolve_group(resolved, groups, include_group, parents)?;
                        requirements
                            .extend(resolved.get(include_group).into_iter().flatten().cloned());
                    }
                    DependencyGroupSpecifier::Object(map) => {
                        warn!(
                            "Ignoring Dependency Object Specifier referenced by `{name}`: {map:?}"
                        );
                    }
                }
            }
            parents.pop();

            resolved.insert(name.clone(), requirements);
            Ok(())
        }

        let mut resolved = BTreeMap::new();
        for name in groups.keys() {
            let mut parents = Vec::new();
            resolve_group(&mut resolved, groups, name, &mut parents)?;
        }
        Ok(Self(resolved))
    }

    /// Return the requirements for a given group, if any.
    pub fn get(
        &self,
        group: &GroupName,
    ) -> Option<&Vec<uv_pep508::Requirement<VerbatimParsedUrl>>> {
        self.0.get(group)
    }
}

impl IntoIterator for FlatDependencyGroups {
    type Item = (GroupName, Vec<uv_pep508::Requirement<VerbatimParsedUrl>>);
    type IntoIter = std::collections::btree_map::IntoIter<
        GroupName,
        Vec<uv_pep508::Requirement<VerbatimParsedUrl>>,
    >;

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
    #[error("Detected a cycle in `dependency-groups`: {0}")]
    DependencyGroupCycle(Cycle),
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

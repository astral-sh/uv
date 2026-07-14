use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use uv_cache_key::{CacheKey, CacheKeyHasher};
use uv_normalize::PackageName;
use uv_pep508::MarkerTree;

use crate::{Dist, Name, Requirement, RequirementSource, Resolution, ResolvedDist};

#[derive(Debug, thiserror::Error)]
pub enum ExtraBuildRequiresError {
    #[error(
        "`{0}` was declared as an extra build dependency with `match-runtime = true`, but was not found in the resolution"
    )]
    NotFound(PackageName),
    #[error(
        "Dependencies marked with `match-runtime = true` cannot include version specifiers, but found: `{0}{1}`"
    )]
    VersionSpecifiersNotAllowed(PackageName, Box<RequirementSource>),
    #[error(
        "Dependencies marked with `match-runtime = true` cannot include URL constraints, but found: `{0}{1}`"
    )]
    UrlNotAllowed(PackageName, Box<RequirementSource>),
}

/// Lowered extra build dependencies with source resolution applied.
#[derive(Debug, Clone, Default)]
pub struct ExtraBuildRequires(BTreeMap<PackageName, Vec<ExtraBuildRequirement>>);

impl std::ops::Deref for ExtraBuildRequires {
    type Target = BTreeMap<PackageName, Vec<ExtraBuildRequirement>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ExtraBuildRequires {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for ExtraBuildRequires {
    type Item = (PackageName, Vec<ExtraBuildRequirement>);
    type IntoIter = std::collections::btree_map::IntoIter<PackageName, Vec<ExtraBuildRequirement>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl FromIterator<(PackageName, Vec<ExtraBuildRequirement>)> for ExtraBuildRequires {
    fn from_iter<T: IntoIterator<Item = (PackageName, Vec<ExtraBuildRequirement>)>>(
        iter: T,
    ) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtraBuildRequirement {
    /// The underlying [`Requirement`] for the build requirement.
    pub requirement: Requirement,
    /// Whether this build requirement should match the runtime environment.
    pub match_runtime: bool,
}

impl From<ExtraBuildRequirement> for Requirement {
    fn from(value: ExtraBuildRequirement) -> Self {
        value.requirement
    }
}

impl CacheKey for ExtraBuildRequirement {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.requirement.cache_key(state);
        self.match_runtime.cache_key(state);
    }
}

impl ExtraBuildRequires {
    /// Return whether a resolved source distribution has a runtime-matched build requirement.
    pub fn has_match_runtime_source(&self, resolution: &Resolution) -> bool {
        resolution.distributions().any(|distribution| {
            matches!(
                distribution,
                ResolvedDist::Installable { dist, .. } if matches!(dist.as_ref(), Dist::Source(_))
            ) && self.get(distribution.name()).is_some_and(|requirements| {
                requirements
                    .iter()
                    .any(|requirement| requirement.match_runtime)
            })
        })
    }

    /// Apply runtime constraints from a resolution to the extra build requirements.
    pub fn match_runtime(
        mut self,
        resolution: &Resolution,
    ) -> Result<Self, ExtraBuildRequiresError> {
        let source_targets = resolution
            .distributions()
            .filter(|distribution| {
                matches!(
                    distribution,
                    ResolvedDist::Installable { dist, .. } if matches!(dist.as_ref(), Dist::Source(_))
                )
            })
            .map(|distribution| distribution.name().clone())
            .collect::<BTreeSet<_>>();
        self.0.retain(|name, requirements| {
            if !source_targets.contains(name) {
                requirements.retain(|requirement| !requirement.match_runtime);
            }
            !requirements.is_empty()
        });

        let mut sources: BTreeMap<PackageName, Vec<(RequirementSource, MarkerTree)>> =
            BTreeMap::new();
        for dist in resolution.distributions() {
            sources
                .entry(dist.name().clone())
                .or_default()
                .push((RequirementSource::from(dist), MarkerTree::TRUE));
        }
        self.match_runtime_sources(&sources)
    }

    /// Apply runtime constraints from resolved package sources to the extra build requirements.
    pub fn match_runtime_sources(
        self,
        sources: &BTreeMap<PackageName, Vec<(RequirementSource, MarkerTree)>>,
    ) -> Result<Self, ExtraBuildRequiresError> {
        self.into_iter()
            .map(|(name, mut requirements)| {
                if !sources.contains_key(&name) {
                    requirements.retain(|requirement| !requirement.match_runtime);
                }
                (name, requirements)
            })
            .filter(|(_, requirements)| !requirements.is_empty())
            .map(|(name, requirements)| {
                let mut matched_requirements = Vec::new();
                for requirement in requirements {
                    match requirement {
                        ExtraBuildRequirement {
                            requirement,
                            match_runtime: true,
                        } => {
                            // Reject requirements with `match-runtime = true` that include any form
                            // of constraint.
                            if let RequirementSource::Registry { specifier, .. } =
                                &requirement.source
                            {
                                if !specifier.is_empty() {
                                    return Err(
                                        ExtraBuildRequiresError::VersionSpecifiersNotAllowed(
                                            requirement.name.clone(),
                                            Box::new(requirement.source.clone()),
                                        ),
                                    );
                                }
                            } else {
                                return Err(ExtraBuildRequiresError::VersionSpecifiersNotAllowed(
                                    requirement.name.clone(),
                                    Box::new(requirement.source.clone()),
                                ));
                            }

                            let runtime_sources =
                                sources.get(&requirement.name).ok_or_else(|| {
                                    ExtraBuildRequiresError::NotFound(requirement.name.clone())
                                })?;
                            for (source, marker) in runtime_sources {
                                let mut requirement = Requirement {
                                    source: source.clone(),
                                    ..requirement.clone()
                                };
                                requirement.marker.and(*marker);
                                matched_requirements.push(ExtraBuildRequirement {
                                    requirement,
                                    match_runtime: true,
                                });
                            }
                        }
                        requirement => matched_requirements.push(requirement),
                    }
                }
                Ok::<_, ExtraBuildRequiresError>((name, matched_requirements))
            })
            .collect::<Result<Self, _>>()
    }
}

/// A map of extra build variables, from variable name to value.
pub type BuildVariables = BTreeMap<String, String>;

/// Extra environment variables to set during builds, on a per-package basis.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ExtraBuildVariables(BTreeMap<PackageName, BuildVariables>);

impl std::ops::Deref for ExtraBuildVariables {
    type Target = BTreeMap<PackageName, BuildVariables>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ExtraBuildVariables {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for ExtraBuildVariables {
    type Item = (PackageName, BuildVariables);
    type IntoIter = std::collections::btree_map::IntoIter<PackageName, BuildVariables>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl FromIterator<(PackageName, BuildVariables)> for ExtraBuildVariables {
    fn from_iter<T: IntoIterator<Item = (PackageName, BuildVariables)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl CacheKey for ExtraBuildVariables {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        for (package, vars) in &self.0 {
            package.as_str().cache_key(state);
            for (key, value) in vars {
                key.cache_key(state);
                value.cache_key(state);
            }
        }
    }
}

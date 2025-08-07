use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use uv_cache_key::{CacheKey, CacheKeyHasher};
use uv_normalize::PackageName;

use crate::{Name, Requirement, RequirementSource, Resolution};

#[derive(Debug, thiserror::Error)]
pub enum ExtraBuildRequiresError {
    #[error(
        "`{0}` was declared as an extra build dependency with `match-runtime = true`, but was not found in the resolution"
    )]
    NotFound(PackageName),
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

#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Apply runtime constraints from a resolution to the extra build requirements.
    pub fn match_runtime(self, resolution: &Resolution) -> Result<Self, ExtraBuildRequiresError> {
        self.into_iter()
            .map(|(name, requirements)| {
                let requirements = requirements
                    .into_iter()
                    .map(|requirement| match requirement {
                        ExtraBuildRequirement {
                            requirement,
                            match_runtime: true,
                        } => {
                            let dist = resolution
                                .distributions()
                                .find(|dist| dist.name() == &requirement.name)
                                .ok_or_else(|| {
                                    ExtraBuildRequiresError::NotFound(requirement.name.clone())
                                })?;
                            let requirement = Requirement {
                                source: RequirementSource::from(dist),
                                ..requirement
                            };
                            Ok::<_, ExtraBuildRequiresError>(ExtraBuildRequirement {
                                requirement,
                                match_runtime: true,
                            })
                        }
                        requirement => Ok(requirement),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok::<_, ExtraBuildRequiresError>((name, requirements))
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

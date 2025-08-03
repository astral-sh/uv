use std::collections::BTreeMap;

use uv_cache_key::{CacheKey, CacheKeyHasher};
use uv_normalize::PackageName;

use crate::Requirement;

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
pub struct AnnotatedBuildRequirement {
    /// The underlying [`Requirement`] for the build requirement.
    pub requirement: Requirement,
    /// Whether this build requirement should match the runtime environment.
    pub match_runtime: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtraBuildRequirement {
    /// An unannotated build requirement (i.e., a simple requirement).
    Unannotated(Requirement),
    /// An annotated build requirement, which includes a `match-runtime` flag.
    Annotated(AnnotatedBuildRequirement),
}

impl From<ExtraBuildRequirement> for Requirement {
    fn from(value: ExtraBuildRequirement) -> Self {
        match value {
            ExtraBuildRequirement::Unannotated(requirement) => requirement,
            ExtraBuildRequirement::Annotated(annotated) => annotated.requirement,
        }
    }
}

impl CacheKey for ExtraBuildRequirement {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        match self {
            ExtraBuildRequirement::Unannotated(req) => {
                0u8.cache_key(state);
                req.cache_key(state);
            }
            ExtraBuildRequirement::Annotated(annotated) => {
                1u8.cache_key(state);
                annotated.requirement.cache_key(state);
                annotated.match_runtime.cache_key(state);
            }
        }
    }
}

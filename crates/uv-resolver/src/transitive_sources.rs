use rustc_hash::FxHashMap;

use uv_distribution_types::Requirement;
use uv_normalize::PackageName;
use uv_pep508::RequirementOrigin;

/// Context-scoped source overlays for transitive dependencies.
#[derive(Clone, Debug, Default)]
pub struct TransitiveSources(
    FxHashMap<PackageName, FxHashMap<RequirementOrigin, Vec<Requirement>>>,
);

impl TransitiveSources {
    /// Build a [`TransitiveSources`] table from lowered source overlay requirements.
    pub fn from_requirements(requirements: impl IntoIterator<Item = Requirement>) -> Self {
        let mut sources: FxHashMap<PackageName, FxHashMap<RequirementOrigin, Vec<Requirement>>> =
            FxHashMap::default();

        for requirement in requirements {
            let Some(origin) = requirement.origin.clone() else {
                continue;
            };
            sources
                .entry(requirement.name.clone())
                .or_default()
                .entry(origin)
                .or_default()
                .push(requirement);
        }

        Self(sources)
    }

    /// Return the source overlay requirements for a package in a given context.
    pub(crate) fn get(
        &self,
        package_name: &PackageName,
        context: &RequirementOrigin,
    ) -> Option<&[Requirement]> {
        self.0
            .get(package_name)
            .and_then(|contexts| contexts.get(context))
            .map(Vec::as_slice)
    }

    /// Returns `true` if there are any transitive source overlays.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns `true` if there are any source overlays for the given package.
    pub(crate) fn contains_key(&self, package_name: &PackageName) -> bool {
        self.0.contains_key(package_name)
    }
}

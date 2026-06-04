use crate::resolver::ForkMap;
use crate::{DependencyMode, Manifest, ResolverEnvironment};
use uv_distribution_types::{IndexMetadata, RequirementSource};
use uv_normalize::PackageName;

/// A map of package names to their explicit index.
///
/// For example, given:
/// ```toml
/// [[tool.uv.index]]
/// name = "pytorch"
/// url = "https://download.pytorch.org/whl/cu130"
///
/// [tool.uv.sources]
/// torch = { index = "pytorch" }
/// ```
///
/// [`Indexes`] would contain a single entry mapping `torch` to `https://download.pytorch.org/whl/cu130`.
#[derive(Debug, Default, Clone)]
pub(crate) struct Indexes(ForkMap<IndexMetadata>);

impl Indexes {
    /// Determine the set of explicit, pinned indexes in the [`Manifest`].
    pub(crate) fn from_manifest(
        manifest: &Manifest,
        env: &ResolverEnvironment,
        dependencies: DependencyMode,
    ) -> Self {
        let mut indexes = ForkMap::default();
        let project = manifest.project.as_ref().or_else(|| {
            let mut projects = manifest.workspace_members.iter();
            let project = projects.next()?;
            projects.next().is_none().then_some(project)
        });

        for requirement in manifest.requirements(env, dependencies) {
            let RequirementSource::Registry {
                index: Some(index),
                conflict,
                ..
            } = &requirement.source
            else {
                continue;
            };
            let mut has_extra = false;
            requirement.marker.visit_extras(|_, _| has_extra = true);
            // Transitive package metadata does not retain its declaring project in `origin`.
            // During universal resolution, complementary source edges preserve these extra-scoped
            // indexes directly, unless a declared conflict already supplies the project scope.
            // Specific resolution evaluates the active extra immediately, so it must retain the
            // index mapping here.
            if env.marker_environment().is_none()
                && requirement.origin.is_none()
                && conflict.is_none()
                && has_extra
            {
                continue;
            }
            indexes.add_with_project(requirement.as_ref(), index.clone(), project);
        }

        Self(indexes)
    }

    /// Returns `true` if there are no explicit indexes.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns `true` if the map contains any indexes for a package.
    pub(crate) fn contains_key(&self, name: &PackageName) -> bool {
        self.0.contains_key(name)
    }

    /// Return the explicit index used for a package in the given fork.
    pub(crate) fn get(&self, name: &PackageName, env: &ResolverEnvironment) -> Vec<&IndexMetadata> {
        self.0.get(name, env)
    }
}

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{borrow::Cow, collections::BTreeMap};

use anyhow::{Context, Result};
use futures::stream::FuturesOrdered;
use futures::TryStreamExt;
use url::Url;

use uv_configuration::{DependencyGroups, ExtrasSpecification};
use uv_distribution::{DistributionDatabase, FlatRequiresDist, Reporter, RequiresDist};
use uv_distribution_types::{
    BuildableSource, DirectorySourceUrl, HashGeneration, HashPolicy, SourceUrl, VersionId,
};
use uv_fs::Simplified;
use uv_normalize::{ExtraName, PackageName};
use uv_pep508::RequirementOrigin;
use uv_pypi_types::Requirement;
use uv_resolver::{InMemoryIndex, MetadataResponse};
use uv_types::{BuildContext, HashStrategy};

#[derive(Debug, Clone)]
pub struct SourceTreeResolution {
    /// The requirements sourced from the source trees.
    pub requirements: Vec<Requirement>,
    /// The names of the projects that were resolved.
    pub project: PackageName,
    /// The extras used when resolving the requirements.
    pub extras: Vec<ExtraName>,
}

/// A resolver for requirements specified via source trees.
///
/// Used, e.g., to determine the input requirements when a user specifies a `pyproject.toml`
/// file, which may require running PEP 517 build hooks to extract metadata.
pub struct SourceTreeResolver<'a, Context: BuildContext> {
    /// The extras to include when resolving requirements.
    extras: &'a ExtrasSpecification,
    /// The groups to include when resolving requirements.
    groups: &'a BTreeMap<PathBuf, DependencyGroups>,
    /// The hash policy to enforce.
    hasher: &'a HashStrategy,
    /// The in-memory index for resolving dependencies.
    index: &'a InMemoryIndex,
    /// The database for fetching and building distributions.
    database: DistributionDatabase<'a, Context>,
}

impl<'a, Context: BuildContext> SourceTreeResolver<'a, Context> {
    /// Instantiate a new [`SourceTreeResolver`] for a given set of `source_trees`.
    pub fn new(
        extras: &'a ExtrasSpecification,
        groups: &'a BTreeMap<PathBuf, DependencyGroups>,
        hasher: &'a HashStrategy,
        index: &'a InMemoryIndex,
        database: DistributionDatabase<'a, Context>,
    ) -> Self {
        Self {
            extras,
            groups,
            hasher,
            index,
            database,
        }
    }

    /// Set the [`Reporter`] to use for this resolver.
    #[must_use]
    pub fn with_reporter(self, reporter: Arc<dyn Reporter>) -> Self {
        Self {
            database: self.database.with_reporter(reporter),
            ..self
        }
    }

    /// Resolve the requirements from the provided source trees.
    pub async fn resolve(
        self,
        source_trees: impl Iterator<Item = &Path>,
    ) -> Result<Vec<SourceTreeResolution>> {
        let resolutions: Vec<_> = source_trees
            .map(|source_tree| async { self.resolve_source_tree(source_tree).await })
            .collect::<FuturesOrdered<_>>()
            .try_collect()
            .await?;
        Ok(resolutions)
    }

    /// Infer the dependencies for a directory dependency.
    async fn resolve_source_tree(&self, path: &Path) -> Result<SourceTreeResolution> {
        let metadata = self.resolve_requires_dist(path).await?;
        let origin = RequirementOrigin::Project(path.to_path_buf(), metadata.name.clone());

        // Determine the extras to include when resolving the requirements.
        let extras = self
            .extras
            .extra_names(metadata.provides_extras.iter())
            .cloned()
            .collect::<Vec<_>>();

        let mut requirements = Vec::new();

        // Resolve any groups associated with this path
        let default_groups = DependencyGroups::default();
        let groups = self.groups.get(path).unwrap_or(&default_groups);

        // Flatten any transitive extras and include dependencies
        // (unless something like --only-group was passed)
        if groups.prod() {
            requirements.extend(
                FlatRequiresDist::from_requirements(metadata.requires_dist, &metadata.name)
                    .into_iter()
                    .map(|requirement| Requirement {
                        origin: Some(origin.clone()),
                        marker: requirement.marker.simplify_extras(&extras),
                        ..requirement
                    }),
            );
        }

        // Apply dependency-groups
        for (group_name, group) in &metadata.dependency_groups {
            if groups.contains(group_name) {
                requirements.extend(group.iter().cloned().map(|group| Requirement {
                    origin: Some(RequirementOrigin::Group(
                        path.to_path_buf(),
                        metadata.name.clone(),
                        group_name.clone(),
                    )),
                    ..group
                }));
            }
        }
        // Complain if dependency groups are named that don't appear.
        for name in groups.explicit_names() {
            if !metadata.dependency_groups.contains_key(name) {
                return Err(anyhow::anyhow!(
                    "The dependency group '{name}' was not found in the project: {}",
                    path.user_display()
                ));
            }
        }

        let project = metadata.name;
        let extras = metadata.provides_extras;

        Ok(SourceTreeResolution {
            requirements,
            project,
            extras,
        })
    }

    /// Resolve the [`RequiresDist`] metadata for a given source tree. Attempts to resolve the
    /// requirements without building the distribution, even if the project contains (e.g.) a
    /// dynamic version since, critically, we don't need to install the package itself; only its
    /// dependencies.
    async fn resolve_requires_dist(&self, path: &Path) -> Result<RequiresDist> {
        // Convert to a buildable source.
        let source_tree = fs_err::canonicalize(path).with_context(|| {
            format!(
                "Failed to canonicalize path to source tree: {}",
                path.user_display()
            )
        })?;
        let source_tree = source_tree.parent().ok_or_else(|| {
            anyhow::anyhow!(
                "The file `{}` appears to be a `pyproject.toml`, `setup.py`, or `setup.cfg` file, which must be in a directory",
                path.user_display()
            )
        })?;

        // If the path is a `pyproject.toml`, attempt to extract the requirements statically. The
        // distribution database will do this too, but we can be even more aggressive here since we
        // _only_ need the requirements. So, for example, even if the version is dynamic, we can
        // still extract the requirements without performing a build, unlike in the database where
        // we typically construct a "complete" metadata object.
        if let Some(metadata) = self.database.requires_dist(source_tree).await? {
            return Ok(metadata);
        }

        let Ok(url) = Url::from_directory_path(source_tree) else {
            return Err(anyhow::anyhow!("Failed to convert path to URL"));
        };
        let source = SourceUrl::Directory(DirectorySourceUrl {
            url: &url,
            install_path: Cow::Borrowed(source_tree),
            editable: false,
        });

        // Determine the hash policy. Since we don't have a package name, we perform a
        // manual match.
        let hashes = match self.hasher {
            HashStrategy::None => HashPolicy::None,
            HashStrategy::Generate(mode) => HashPolicy::Generate(*mode),
            HashStrategy::Verify(_) => HashPolicy::Generate(HashGeneration::All),
            HashStrategy::Require(_) => {
                return Err(anyhow::anyhow!(
                    "Hash-checking is not supported for local directories: {}",
                    path.user_display()
                ));
            }
        };

        // Fetch the metadata for the distribution.
        let metadata = {
            let id = VersionId::from_url(source.url());
            if self.index.distributions().register(id.clone()) {
                // Run the PEP 517 build process to extract metadata from the source distribution.
                let source = BuildableSource::Url(source);
                let archive = self.database.build_wheel_metadata(&source, hashes).await?;

                let metadata = archive.metadata.clone();

                // Insert the metadata into the index.
                self.index
                    .distributions()
                    .done(id, Arc::new(MetadataResponse::Found(archive)));

                metadata
            } else {
                let response = self
                    .index
                    .distributions()
                    .wait(&id)
                    .await
                    .expect("missing value for registered task");
                let MetadataResponse::Found(archive) = &*response else {
                    panic!("Failed to find metadata for: {}", path.user_display());
                };
                archive.metadata.clone()
            }
        };

        Ok(RequiresDist::from(metadata))
    }
}

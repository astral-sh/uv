use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use futures::TryStreamExt;
use futures::stream::FuturesOrdered;
use url::Url;

use uv_configuration::ExtrasSpecification;
use uv_distribution::{DistributionDatabase, FlatRequiresDist, Reporter, RequiresDist};
use uv_distribution_types::Requirement;
use uv_distribution_types::{
    BuildableSource, DirectorySourceUrl, HashGeneration, HashPolicy, SourceUrl, VersionId,
};
use uv_fs::Simplified;
use uv_normalize::{ExtraName, PackageName};
use uv_pep508::RequirementOrigin;
use uv_pypi_types::PyProjectToml;
use uv_redacted::DisplaySafeUrl;
use uv_resolver::{InMemoryIndex, MetadataResponse};
use uv_types::{BuildContext, HashStrategy};

#[derive(Debug, Clone)]
pub enum SourceTree {
    PyProjectToml(PathBuf, PyProjectToml),
    SetupPy(PathBuf),
    SetupCfg(PathBuf),
}

impl SourceTree {
    /// Return the [`Path`] to the file representing the source tree (e.g., the `pyproject.toml`).
    pub fn path(&self) -> &Path {
        match self {
            Self::PyProjectToml(path, ..) => path,
            Self::SetupPy(path) => path,
            Self::SetupCfg(path) => path,
        }
    }

    /// Return the [`PyProjectToml`] if this is a `pyproject.toml`-based source tree.
    pub fn pyproject_toml(&self) -> Option<&PyProjectToml> {
        match self {
            Self::PyProjectToml(.., toml) => Some(toml),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceTreeResolution {
    /// The requirements sourced from the source trees.
    pub requirements: Box<[Requirement]>,
    /// The names of the projects that were resolved.
    pub project: PackageName,
    /// The extras used when resolving the requirements.
    pub extras: Box<[ExtraName]>,
}

/// A resolver for requirements specified via source trees.
///
/// Used, e.g., to determine the input requirements when a user specifies a `pyproject.toml`
/// file, which may require running PEP 517 build hooks to extract metadata.
pub struct SourceTreeResolver<'a, Context: BuildContext> {
    /// The extras to include when resolving requirements.
    extras: &'a ExtrasSpecification,
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
        hasher: &'a HashStrategy,
        index: &'a InMemoryIndex,
        database: DistributionDatabase<'a, Context>,
    ) -> Self {
        Self {
            extras,
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
        source_trees: impl Iterator<Item = &SourceTree>,
    ) -> Result<Vec<SourceTreeResolution>> {
        let resolutions: Vec<_> = source_trees
            .map(async |source_tree| self.resolve_source_tree(source_tree).await)
            .collect::<FuturesOrdered<_>>()
            .try_collect()
            .await?;
        Ok(resolutions)
    }

    /// Infer the dependencies for a directory dependency.
    async fn resolve_source_tree(&self, source_tree: &SourceTree) -> Result<SourceTreeResolution> {
        let metadata = self.resolve_requires_dist(source_tree).await?;
        let origin =
            RequirementOrigin::Project(source_tree.path().to_path_buf(), metadata.name.clone());

        // Determine the extras to include when resolving the requirements.
        let extras = self
            .extras
            .extra_names(metadata.provides_extra.iter())
            .cloned()
            .collect::<Vec<_>>();

        let mut requirements = Vec::new();

        // Flatten any transitive extras and include dependencies
        // (unless something like --only-group was passed)
        requirements.extend(
            FlatRequiresDist::from_requirements(metadata.requires_dist, &metadata.name)
                .into_iter()
                .map(|requirement| Requirement {
                    origin: Some(origin.clone()),
                    marker: requirement.marker.simplify_extras(&extras),
                    ..requirement
                }),
        );

        let requirements = requirements.into_boxed_slice();
        let project = metadata.name;
        let extras = metadata.provides_extra;

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
    async fn resolve_requires_dist(&self, source_tree: &SourceTree) -> Result<RequiresDist> {
        // Convert to a buildable source.
        let path = fs_err::canonicalize(source_tree.path()).with_context(|| {
            format!(
                "Failed to canonicalize path to source tree: {}",
                source_tree.path().user_display()
            )
        })?;
        let path = path.parent().ok_or_else(|| {
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
        if let Some(pyproject_toml) = source_tree.pyproject_toml() {
            if let Some(metadata) = self.database.requires_dist(path, pyproject_toml).await? {
                return Ok(metadata);
            }
        }

        let Ok(url) = Url::from_directory_path(path).map(DisplaySafeUrl::from_url) else {
            return Err(anyhow::anyhow!("Failed to convert path to URL"));
        };
        let source = SourceUrl::Directory(DirectorySourceUrl {
            url: &url,
            install_path: Cow::Borrowed(path),
            editable: None,
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

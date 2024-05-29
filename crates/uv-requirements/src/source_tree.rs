use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use futures::stream::FuturesOrdered;
use futures::TryStreamExt;
use url::Url;

use distribution_types::{BuildableSource, DirectorySourceUrl, HashPolicy, SourceUrl, VersionId};
use pep508_rs::RequirementOrigin;
use pypi_types::{Requirement, Requirements};
use uv_configuration::{ExtrasSpecification, PreviewMode};
use uv_distribution::{
    lower_requirements, DistributionDatabase, Metadata23Lowered, ProjectWorkspace, Reporter,
};
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_resolver::{InMemoryIndex, MetadataResponse};
use uv_types::{BuildContext, HashStrategy};

/// A resolver for requirements specified via source trees.
///
/// Used, e.g., to determine the input requirements when a user specifies a `pyproject.toml`
/// file, which may require running PEP 517 build hooks to extract metadata.
pub struct SourceTreeResolver<'a, Context: BuildContext> {
    /// The requirements for the project.
    source_trees: Vec<PathBuf>,
    /// The extras to include when resolving requirements.
    extras: &'a ExtrasSpecification,
    /// The hash policy to enforce.
    hasher: &'a HashStrategy,
    /// The in-memory index for resolving dependencies.
    index: &'a InMemoryIndex,
    /// The database for fetching and building distributions.
    database: DistributionDatabase<'a, Context>,
    preview: PreviewMode,
}

impl<'a, Context: BuildContext> SourceTreeResolver<'a, Context> {
    /// Instantiate a new [`SourceTreeResolver`] for a given set of `source_trees`.
    pub fn new(
        source_trees: Vec<PathBuf>,
        extras: &'a ExtrasSpecification,
        hasher: &'a HashStrategy,
        index: &'a InMemoryIndex,
        database: DistributionDatabase<'a, Context>,
        preview: PreviewMode,
    ) -> Self {
        Self {
            source_trees,
            extras,
            hasher,
            index,
            database,
            preview,
        }
    }

    /// Set the [`Reporter`] to use for this resolver.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            database: self.database.with_reporter(reporter),
            ..self
        }
    }

    /// Resolve the requirements from the provided source trees.
    pub async fn resolve(self) -> Result<Vec<Requirement>> {
        let requirements: Vec<_> = self
            .source_trees
            .iter()
            .map(|source_tree| async { self.resolve_source_tree(source_tree).await })
            .collect::<FuturesOrdered<_>>()
            .try_collect()
            .await?;
        Ok(requirements
            .into_iter()
            .flatten()
            .map(Requirement::from)
            .collect())
    }

    async fn read_static_metadata(
        &self,
        path: &Path,
    ) -> Result<Option<(Requirements, PackageName)>> {
        if !path
            .file_name()
            .is_some_and(|file_name| file_name == "pyproject.toml")
        {
            return Ok(None);
        }
        let Some(project_workspace) = ProjectWorkspace::from_maybe_project_root(
            path.parent().expect("`pyproject.toml` must have a parent"),
        )
        .await?
        else {
            return Ok(None);
        };

        let Some(project) = &project_workspace.current_project().pyproject_toml().project else {
            return Ok(None);
        };

        // TODO(konsti): Only check for optional-dependencies if there are extras
        if project.dynamic.as_ref().is_some_and(|dynamic| {
            dynamic.contains(&"dependencies".to_string())
                || dynamic.contains(&"optional-dependencies".to_string())
        }) {
            return Ok(None);
        }

        let sources = project_workspace
            .current_project()
            .pyproject_toml()
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.sources.clone())
            .unwrap_or_default();

        let requirements = lower_requirements(
            project.dependencies.as_deref(),
            project.optional_dependencies.as_ref(),
            path,
            &project.name,
            project_workspace.project_root(),
            &sources,
            project_workspace.workspace(),
            self.preview,
        )?;
        Ok(Some((requirements, project.name.clone())))
    }

    /// Infer the dependencies for a directory dependency.
    ///
    /// The dependencies may be declared statically in a `pyproject.toml` (fast path), or we build
    /// the path distribution and read the metadata from the build.
    ///
    /// TODO(konsti): Shouldn't we also read setup.cfg and such?
    async fn resolve_source_tree(&self, path: &Path) -> Result<Vec<Requirement>> {
        if let Some((requirements, project_name)) = self.read_static_metadata(&path).await? {
            // Extract the origin.
            let origin = RequirementOrigin::Project(path.to_path_buf(), project_name);
            // Collect the mandatory requirements and the optional requirements for the activated
            // extras and add the origin to all requirements
            Ok(requirements
                .dependencies
                .into_iter()
                .chain(
                    requirements
                        .optional_dependencies
                        .into_iter()
                        .filter_map(|(key, optional_dependencies)| match self.extras {
                            ExtrasSpecification::None => None,
                            ExtrasSpecification::All => Some(optional_dependencies),
                            ExtrasSpecification::Some(extras) => {
                                if extras.contains(&key) {
                                    Some(optional_dependencies)
                                } else {
                                    None
                                }
                            }
                        })
                        .flatten(),
                )
                .map(|requirement| requirement.with_origin(origin.clone()))
                .collect())
        } else {
            let metadata = self.resolve_by_build(path).await?;
            let origin = RequirementOrigin::Project(path.to_path_buf(), metadata.name.clone());
            // Determine the appropriate requirements to return based on the extras. This involves
            // evaluating the `extras` expression in any markers, but preserving the remaining marker
            // conditions.
            Ok(metadata
                .requires_dist
                .into_iter()
                .map(|requirement| match self.extras {
                    ExtrasSpecification::None => requirement.with_origin(origin.clone()),
                    ExtrasSpecification::All => Requirement {
                        origin: Some(origin.clone()),
                        marker: requirement
                            .marker
                            .and_then(|marker| marker.simplify_extras(&metadata.provides_extras)),
                        ..requirement
                    },
                    ExtrasSpecification::Some(extras) => Requirement {
                        origin: Some(origin.clone()),
                        marker: requirement
                            .marker
                            .and_then(|marker| marker.simplify_extras(extras)),
                        ..requirement
                    },
                })
                .collect())
        }
    }

    // Extract the origin.
    async fn resolve_by_build(&self, path: &Path) -> Result<Metadata23Lowered> {
        // Convert to a buildable source.
        let source_tree = fs_err::canonicalize(path).with_context(|| {
            format!(
                "Failed to canonicalize path to source tree: {}",
                path.user_display()
            )
        })?;
        let source_tree = source_tree.parent().ok_or_else(|| {
            anyhow::anyhow!(
                "The file `{}` appears to be a `setup.py` or `setup.cfg` file, which must be in a directory",
                path.user_display()
            )
        })?;

        let Ok(url) = Url::from_directory_path(source_tree) else {
            return Err(anyhow::anyhow!("Failed to convert path to URL"));
        };
        let source = SourceUrl::Directory(DirectorySourceUrl {
            url: &url,
            path: Cow::Borrowed(source_tree),
            editable: false,
        });

        // Determine the hash policy. Since we don't have a package name, we perform a
        // manual match.
        let hashes = match self.hasher {
            HashStrategy::None => HashPolicy::None,
            HashStrategy::Generate => HashPolicy::Generate,
            HashStrategy::Validate { .. } => {
                return Err(anyhow::anyhow!(
                    "Hash-checking is not supported for local directories: {}",
                    path.user_display()
                ));
            }
        };

        // Fetch the metadata for the distribution.
        let metadata = {
            let id = VersionId::from_url(source.url());
            if let Some(archive) =
                self.index
                    .distributions()
                    .get(&id)
                    .as_deref()
                    .and_then(|response| {
                        if let MetadataResponse::Found(archive) = response {
                            Some(archive)
                        } else {
                            None
                        }
                    })
            {
                // If the metadata is already in the index, return it.
                archive.metadata.clone()
            } else {
                // Run the PEP 517 build process to extract metadata from the source distribution.
                let source = BuildableSource::Url(source);
                let archive = self.database.build_wheel_metadata(&source, hashes).await?;

                // Insert the metadata into the index.
                self.index
                    .distributions()
                    .done(id, Arc::new(MetadataResponse::Found(archive.clone())));

                archive.metadata
            }
        };

        Ok(metadata)
    }
}

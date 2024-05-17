use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use itertools::{Either, Itertools};
use rustc_hash::FxHashSet;
use tracing::{debug, instrument};

use cache_key::CanonicalUrl;
use distribution_types::{
    FlatIndexLocation, IndexUrl, Requirement, RequirementSource, UnresolvedRequirement,
    UnresolvedRequirementSpecification,
};
use pep508_rs::{UnnamedRequirement, VerbatimUrl};
use requirements_txt::{
    EditableRequirement, FindLink, RequirementEntry, RequirementsTxt, RequirementsTxtRequirement,
};
use uv_client::BaseClientBuilder;
use uv_configuration::{NoBinary, NoBuild, PreviewMode};
use uv_fs::Simplified;
use uv_normalize::{ExtraName, PackageName};

use crate::pyproject::{Pep621Metadata, PyProjectToml};
use crate::{ExtrasSpecification, RequirementsSource};

#[derive(Debug, Default)]
pub struct RequirementsSpecification {
    /// The name of the project specifying requirements.
    pub project: Option<PackageName>,
    /// The requirements for the project.
    pub requirements: Vec<UnresolvedRequirementSpecification>,
    /// The constraints for the project.
    pub constraints: Vec<Requirement>,
    /// The overrides for the project.
    pub overrides: Vec<UnresolvedRequirementSpecification>,
    /// Package to install as editable installs
    pub editables: Vec<EditableRequirement>,
    /// The source trees from which to extract requirements.
    pub source_trees: Vec<PathBuf>,
    /// The extras used to collect requirements.
    pub extras: FxHashSet<ExtraName>,
    /// The index URL to use for fetching packages.
    pub index_url: Option<IndexUrl>,
    /// The extra index URLs to use for fetching packages.
    pub extra_index_urls: Vec<IndexUrl>,
    /// Whether to disallow index usage.
    pub no_index: bool,
    /// The `--find-links` locations to use for fetching packages.
    pub find_links: Vec<FlatIndexLocation>,
    /// The `--no-binary` flags to enforce when selecting distributions.
    pub no_binary: NoBinary,
    /// The `--no-build` flags to enforce when selecting distributions.
    pub no_build: NoBuild,
}

impl RequirementsSpecification {
    /// Read the requirements and constraints from a source.
    #[instrument(skip_all, level = tracing::Level::DEBUG, fields(source = % source))]
    pub async fn from_source(
        source: &RequirementsSource,
        extras: &ExtrasSpecification,
        client_builder: &BaseClientBuilder<'_>,
        preview: PreviewMode,
    ) -> Result<Self> {
        Ok(match source {
            RequirementsSource::Package(name) => {
                let requirement = RequirementsTxtRequirement::parse(name, std::env::current_dir()?)
                    .with_context(|| format!("Failed to parse `{name}`"))?;
                Self {
                    requirements: vec![UnresolvedRequirementSpecification::try_from(
                        RequirementEntry {
                            requirement,
                            hashes: vec![],
                        },
                    )?],
                    ..Self::default()
                }
            }
            RequirementsSource::Editable(name) => {
                let requirement = EditableRequirement::parse(name, None, std::env::current_dir()?)
                    .with_context(|| format!("Failed to parse `{name}`"))?;
                Self {
                    editables: vec![requirement],
                    ..Self::default()
                }
            }
            RequirementsSource::RequirementsTxt(path) => {
                let requirements_txt =
                    RequirementsTxt::parse(path, std::env::current_dir()?, client_builder).await?;
                Self {
                    requirements: requirements_txt
                        .requirements
                        .into_iter()
                        .map(UnresolvedRequirementSpecification::try_from)
                        .collect::<Result<_, _>>()?,
                    constraints: requirements_txt
                        .constraints
                        .into_iter()
                        .map(Requirement::from_pep508)
                        .collect::<Result<_, _>>()?,
                    editables: requirements_txt.editables,
                    index_url: requirements_txt.index_url.map(IndexUrl::from),
                    extra_index_urls: requirements_txt
                        .extra_index_urls
                        .into_iter()
                        .map(IndexUrl::from)
                        .collect(),
                    no_index: requirements_txt.no_index,
                    find_links: requirements_txt
                        .find_links
                        .into_iter()
                        .map(|link| match link {
                            FindLink::Url(url) => FlatIndexLocation::Url(url),
                            FindLink::Path(path) => FlatIndexLocation::Path(path),
                        })
                        .collect(),
                    no_binary: requirements_txt.no_binary,
                    no_build: requirements_txt.only_binary,
                    ..Self::default()
                }
            }
            RequirementsSource::PyprojectToml(path) => {
                let contents = uv_fs::read_to_string(&path).await?;
                Self::parse_direct_pyproject_toml(&contents, extras, path.as_ref(), preview)
                    .with_context(|| format!("Failed to parse `{}`", path.user_display()))?
            }
            RequirementsSource::SetupPy(path) | RequirementsSource::SetupCfg(path) => Self {
                source_trees: vec![path.clone()],
                ..Self::default()
            },
            RequirementsSource::SourceTree(path) => Self {
                project: None,
                requirements: vec![UnresolvedRequirementSpecification {
                    requirement: UnresolvedRequirement::Unnamed(UnnamedRequirement {
                        url: VerbatimUrl::from_path(path),
                        extras: vec![],
                        marker: None,
                        origin: None,
                    }),
                    hashes: vec![],
                }],
                ..Self::default()
            },
        })
    }

    /// Attempt to read metadata from the `pyproject.toml` directly.
    ///
    /// Since we only use this path for directly included pyproject.toml, we are strict about
    /// PEP 621 and don't allow invalid `project.dependencies` (e.g., Hatch's relative path
    /// support).
    pub(crate) fn parse_direct_pyproject_toml(
        contents: &str,
        extras: &ExtrasSpecification,
        pyproject_path: &Path,
        preview: PreviewMode,
    ) -> Result<Self> {
        let pyproject = toml::from_str::<PyProjectToml>(contents)?;

        // We need use this path as base for the relative paths inside pyproject.toml, so
        // we need the absolute path instead of a potentially relative path. E.g. with
        // `foo = { path = "../foo" }`, we will join `../foo` onto this path.
        let absolute_path = uv_fs::absolutize_path(pyproject_path)?;
        let project_dir = absolute_path
            .parent()
            .context("`pyproject.toml` has no parent directory")?;

        let workspace_sources = HashMap::default();
        let workspace_packages = HashMap::default();
        match Pep621Metadata::try_from(
            pyproject,
            extras,
            pyproject_path,
            project_dir,
            &workspace_sources,
            &workspace_packages,
            preview,
        ) {
            Ok(Some(project)) => {
                // Partition into editable and non-editable requirements.
                let (editables, requirements): (Vec<_>, Vec<_>) = project
                    .requirements
                    .into_iter()
                    .partition_map(|requirement| {
                        if let RequirementSource::Path {
                            path,
                            editable: Some(true),
                            url,
                        } = requirement.source
                        {
                            Either::Left(EditableRequirement {
                                url,
                                path,
                                marker: requirement.marker,
                                extras: requirement.extras,
                                origin: requirement.origin,
                            })
                        } else {
                            Either::Right(UnresolvedRequirementSpecification {
                                requirement: UnresolvedRequirement::Named(requirement),
                                hashes: vec![],
                            })
                        }
                    });

                Ok(Self {
                    project: Some(project.name),
                    editables,
                    requirements,
                    extras: project.used_extras,
                    ..Self::default()
                })
            }
            Ok(None) => {
                debug!(
                    "Dynamic pyproject.toml at: `{}`",
                    pyproject_path.user_display()
                );
                Ok(Self {
                    project: None,
                    requirements: vec![],
                    source_trees: vec![pyproject_path.to_path_buf()],
                    ..Self::default()
                })
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Read the combined requirements and constraints from a set of sources.
    pub async fn from_sources(
        requirements: &[RequirementsSource],
        constraints: &[RequirementsSource],
        overrides: &[RequirementsSource],
        extras: &ExtrasSpecification,
        client_builder: &BaseClientBuilder<'_>,
        preview: PreviewMode,
    ) -> Result<Self> {
        let mut spec = Self::default();

        // Read all requirements, and keep track of all requirements _and_ constraints.
        // A `requirements.txt` can contain a `-c constraints.txt` directive within it, so reading
        // a requirements file can also add constraints.
        for source in requirements {
            let source = Self::from_source(source, extras, client_builder, preview).await?;
            spec.requirements.extend(source.requirements);
            spec.constraints.extend(source.constraints);
            spec.overrides.extend(source.overrides);
            spec.extras.extend(source.extras);
            spec.editables.extend(source.editables);
            spec.source_trees.extend(source.source_trees);

            // Use the first project name discovered.
            if spec.project.is_none() {
                spec.project = source.project;
            }

            if let Some(index_url) = source.index_url {
                if let Some(existing) = spec.index_url {
                    if CanonicalUrl::new(index_url.url()) != CanonicalUrl::new(existing.url()) {
                        return Err(anyhow::anyhow!(
                            "Multiple index URLs specified: `{existing}` vs. `{index_url}`",
                        ));
                    }
                }
                spec.index_url = Some(index_url);
            }
            spec.no_index |= source.no_index;
            spec.extra_index_urls.extend(source.extra_index_urls);
            spec.find_links.extend(source.find_links);
            spec.no_binary.extend(source.no_binary);
            spec.no_build.extend(source.no_build);
        }

        // Read all constraints, treating both requirements _and_ constraints as constraints.
        // Overrides are ignored, as are the hashes, as they are not relevant for constraints.
        for source in constraints {
            let source = Self::from_source(source, extras, client_builder, preview).await?;
            for entry in source.requirements {
                match entry.requirement {
                    UnresolvedRequirement::Named(requirement) => {
                        spec.constraints.push(requirement);
                    }
                    UnresolvedRequirement::Unnamed(requirement) => {
                        return Err(anyhow::anyhow!(
                            "Unnamed requirements are not allowed as constraints (found: `{requirement}`)"
                        ));
                    }
                }
            }
            spec.constraints.extend(source.constraints);

            if let Some(index_url) = source.index_url {
                if let Some(existing) = spec.index_url {
                    if CanonicalUrl::new(index_url.url()) != CanonicalUrl::new(existing.url()) {
                        return Err(anyhow::anyhow!(
                            "Multiple index URLs specified: `{existing}` vs. `{index_url}`",
                        ));
                    }
                }
                spec.index_url = Some(index_url);
            }
            spec.no_index |= source.no_index;
            spec.extra_index_urls.extend(source.extra_index_urls);
            spec.find_links.extend(source.find_links);
            spec.no_binary.extend(source.no_binary);
            spec.no_build.extend(source.no_build);
        }

        // Read all overrides, treating both requirements _and_ overrides as overrides.
        // Constraints are ignored.
        for source in overrides {
            let source = Self::from_source(source, extras, client_builder, preview).await?;
            spec.overrides.extend(source.requirements);
            spec.overrides.extend(source.overrides);

            if let Some(index_url) = source.index_url {
                if let Some(existing) = spec.index_url {
                    if CanonicalUrl::new(index_url.url()) != CanonicalUrl::new(existing.url()) {
                        return Err(anyhow::anyhow!(
                            "Multiple index URLs specified: `{existing}` vs. `{index_url}`",
                        ));
                    }
                }
                spec.index_url = Some(index_url);
            }
            spec.no_index |= source.no_index;
            spec.extra_index_urls.extend(source.extra_index_urls);
            spec.find_links.extend(source.find_links);
            spec.no_binary.extend(source.no_binary);
            spec.no_build.extend(source.no_build);
        }

        Ok(spec)
    }

    /// Read the requirements from a set of sources.
    pub async fn from_simple_sources(
        requirements: &[RequirementsSource],
        client_builder: &BaseClientBuilder<'_>,
        preview: PreviewMode,
    ) -> Result<Self> {
        Self::from_sources(
            requirements,
            &[],
            &[],
            &ExtrasSpecification::None,
            client_builder,
            preview,
        )
        .await
    }
}

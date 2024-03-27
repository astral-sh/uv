use std::path::PathBuf;

use anyhow::{Context, Result};
use rustc_hash::FxHashSet;
use tracing::{instrument, Level};

use cache_key::CanonicalUrl;
use distribution_types::{FlatIndexLocation, IndexUrl};
use pep508_rs::{Requirement, RequirementsTxtRequirement};
use requirements_txt::{EditableRequirement, FindLink, RequirementsTxt};
use uv_client::BaseClientBuilder;
use uv_fs::Simplified;
use uv_normalize::{ExtraName, PackageName};
use uv_types::{NoBinary, NoBuild};

use crate::pyproject::{Pep621Metadata, PyProjectToml};
use crate::{ExtrasSpecification, RequirementsSource};

#[derive(Debug, Default)]
pub struct RequirementsSpecification {
    /// The name of the project specifying requirements.
    pub project: Option<PackageName>,
    /// The requirements for the project.
    pub requirements: Vec<RequirementsTxtRequirement>,
    /// The constraints for the project.
    pub constraints: Vec<Requirement>,
    /// The overrides for the project.
    pub overrides: Vec<Requirement>,
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
    #[instrument(skip_all, level = Level::DEBUG, fields(source = % source))]
    pub async fn from_source(
        source: &RequirementsSource,
        extras: &ExtrasSpecification<'_>,
        client_builder: &BaseClientBuilder<'_>,
    ) -> Result<Self> {
        Ok(match source {
            RequirementsSource::Package(name) => {
                let requirement = RequirementsTxtRequirement::parse(name, std::env::current_dir()?)
                    .with_context(|| format!("Failed to parse `{name}`"))?;
                Self {
                    project: None,
                    requirements: vec![requirement],
                    constraints: vec![],
                    overrides: vec![],
                    editables: vec![],
                    source_trees: vec![],
                    extras: FxHashSet::default(),
                    index_url: None,
                    extra_index_urls: vec![],
                    no_index: false,
                    find_links: vec![],
                    no_binary: NoBinary::default(),
                    no_build: NoBuild::default(),
                }
            }
            RequirementsSource::Editable(name) => {
                let requirement = EditableRequirement::parse(name, std::env::current_dir()?)
                    .with_context(|| format!("Failed to parse `{name}`"))?;
                Self {
                    project: None,
                    requirements: vec![],
                    constraints: vec![],
                    overrides: vec![],
                    editables: vec![requirement],
                    source_trees: vec![],
                    extras: FxHashSet::default(),
                    index_url: None,
                    extra_index_urls: vec![],
                    no_index: false,
                    find_links: vec![],
                    no_binary: NoBinary::default(),
                    no_build: NoBuild::default(),
                }
            }
            RequirementsSource::RequirementsTxt(path) => {
                let requirements_txt =
                    RequirementsTxt::parse(path, std::env::current_dir()?, client_builder).await?;
                Self {
                    project: None,
                    requirements: requirements_txt
                        .requirements
                        .into_iter()
                        .map(|entry| entry.requirement)
                        .collect(),
                    constraints: requirements_txt.constraints,
                    overrides: vec![],
                    editables: requirements_txt.editables,
                    source_trees: vec![],
                    extras: FxHashSet::default(),
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
                }
            }
            RequirementsSource::PyprojectToml(path) => {
                let contents = uv_fs::read_to_string(path).await?;
                let pyproject = toml::from_str::<PyProjectToml>(&contents)
                    .with_context(|| format!("Failed to parse `{}`", path.user_display()))?;

                // Attempt to read metadata from the `pyproject.toml` directly.
                //
                // If we fail to extract the PEP 621 metadata, fall back to treating it as a source
                // tree, as there are some cases where the `pyproject.toml` may not be a valid PEP
                // 621 file, but might still resolve under PEP 517. (If the source tree doesn't
                // resolve under PEP 517, we'll catch that later.)
                //
                // For example, Hatch's "Context formatting" API is not compliant with PEP 621, as
                // it expects dynamic processing by the build backend for the static metadata
                // fields. See: https://hatch.pypa.io/latest/config/context/
                if let Some(project) = pyproject
                    .project
                    .and_then(|project| Pep621Metadata::try_from(project, extras).ok().flatten())
                {
                    Self {
                        project: Some(project.name),
                        requirements: project
                            .requirements
                            .into_iter()
                            .map(RequirementsTxtRequirement::Pep508)
                            .collect(),
                        constraints: vec![],
                        overrides: vec![],
                        editables: vec![],
                        source_trees: vec![],
                        extras: project.used_extras,
                        index_url: None,
                        extra_index_urls: vec![],
                        no_index: false,
                        find_links: vec![],
                        no_binary: NoBinary::default(),
                        no_build: NoBuild::default(),
                    }
                } else {
                    let path = fs_err::canonicalize(path)?;
                    let source_tree = path.parent().ok_or_else(|| {
                        anyhow::anyhow!(
                            "The file `{}` appears to be a `pyproject.toml` file, which must be in a directory",
                            path.user_display()
                        )
                    })?;
                    Self {
                        project: None,
                        requirements: vec![],
                        constraints: vec![],
                        overrides: vec![],
                        editables: vec![],
                        source_trees: vec![source_tree.to_path_buf()],
                        extras: FxHashSet::default(),
                        index_url: None,
                        extra_index_urls: vec![],
                        no_index: false,
                        find_links: vec![],
                        no_binary: NoBinary::default(),
                        no_build: NoBuild::default(),
                    }
                }
            }
            RequirementsSource::SetupPy(path) | RequirementsSource::SetupCfg(path) => {
                let path = fs_err::canonicalize(path)?;
                let source_tree = path.parent().ok_or_else(|| {
                    anyhow::anyhow!(
                        "The file `{}` appears to be a `setup.py` or `setup.cfg` file, which must be in a directory",
                        path.user_display()
                    )
                })?;
                Self {
                    project: None,
                    requirements: vec![],
                    constraints: vec![],
                    overrides: vec![],
                    editables: vec![],
                    source_trees: vec![source_tree.to_path_buf()],
                    extras: FxHashSet::default(),
                    index_url: None,
                    extra_index_urls: vec![],
                    no_index: false,
                    find_links: vec![],
                    no_binary: NoBinary::default(),
                    no_build: NoBuild::default(),
                }
            }
        })
    }

    /// Read the combined requirements and constraints from a set of sources.
    pub async fn from_sources(
        requirements: &[RequirementsSource],
        constraints: &[RequirementsSource],
        overrides: &[RequirementsSource],
        extras: &ExtrasSpecification<'_>,
        client_builder: &BaseClientBuilder<'_>,
    ) -> Result<Self> {
        let mut spec = Self::default();

        // Read all requirements, and keep track of all requirements _and_ constraints.
        // A `requirements.txt` can contain a `-c constraints.txt` directive within it, so reading
        // a requirements file can also add constraints.
        for source in requirements {
            let source = Self::from_source(source, extras, client_builder).await?;
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

        // Read all constraints, treating _everything_ as a constraint.
        for source in constraints {
            let source = Self::from_source(source, extras, client_builder).await?;
            for requirement in source.requirements {
                match requirement {
                    RequirementsTxtRequirement::Pep508(requirement) => {
                        spec.constraints.push(requirement);
                    }
                    RequirementsTxtRequirement::Unnamed(requirement) => {
                        return Err(anyhow::anyhow!(
                            "Unnamed requirements are not allowed as constraints (found: `{requirement}`)"
                        ));
                    }
                }
            }
            spec.constraints.extend(source.constraints);
            spec.constraints.extend(source.overrides);

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

        // Read all overrides, treating both requirements _and_ constraints as overrides.
        for source in overrides {
            let source = Self::from_source(source, extras, client_builder).await?;
            for requirement in source.requirements {
                match requirement {
                    RequirementsTxtRequirement::Pep508(requirement) => {
                        spec.overrides.push(requirement);
                    }
                    RequirementsTxtRequirement::Unnamed(requirement) => {
                        return Err(anyhow::anyhow!(
                            "Unnamed requirements are not allowed as overrides (found: `{requirement}`)"
                        ));
                    }
                }
            }
            spec.overrides.extend(source.constraints);
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
    ) -> Result<Self> {
        Self::from_sources(
            requirements,
            &[],
            &[],
            &ExtrasSpecification::None,
            client_builder,
        )
        .await
    }
}

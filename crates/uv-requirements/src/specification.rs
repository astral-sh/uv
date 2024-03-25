use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use pyproject_toml::Project;
use rustc_hash::FxHashSet;
use tracing::{instrument, Level};

use cache_key::CanonicalUrl;
use distribution_types::{FlatIndexLocation, IndexUrl};
use pep508_rs::{Requirement, RequirementsTxtRequirement};
use requirements_txt::{EditableRequirement, FindLink, RequirementsTxt};
use uv_client::BaseClientBuilder;
use uv_fs::Simplified;
use uv_normalize::{ExtraName, PackageName};

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
                }
            }
            RequirementsSource::PyprojectToml(path) => {
                let contents = uv_fs::read_to_string(path).await?;
                let pyproject_toml = toml::from_str::<pyproject_toml::PyProjectToml>(&contents)
                    .with_context(|| format!("Failed to parse `{}`", path.user_display()))?;

                // Attempt to read metadata from the `pyproject.toml` directly.
                if let Some(project) = pyproject_toml
                    .project
                    .map(|project| {
                        StaticProject::try_from(project, extras).with_context(|| {
                            format!(
                                "Failed to extract requirements from `{}`",
                                path.user_display()
                            )
                        })
                    })
                    .transpose()?
                    .flatten()
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

#[derive(Debug)]
pub struct StaticProject {
    /// The name of the project.
    pub name: PackageName,
    /// The requirements extracted from the project.
    pub requirements: Vec<Requirement>,
    /// The extras used to collect requirements.
    pub used_extras: FxHashSet<ExtraName>,
}

impl StaticProject {
    pub fn try_from(project: Project, extras: &ExtrasSpecification) -> Result<Option<Self>> {
        // Parse the project name.
        let name =
            PackageName::new(project.name).with_context(|| "Invalid `project.name`".to_string())?;

        if let Some(dynamic) = project.dynamic.as_ref() {
            // If the project specifies dynamic dependencies, we can't extract the requirements.
            if dynamic.iter().any(|field| field == "dependencies") {
                return Ok(None);
            }
            // If we requested extras, and the project specifies dynamic optional dependencies, we can't
            // extract the requirements.
            if !extras.is_empty() && dynamic.iter().any(|field| field == "optional-dependencies") {
                return Ok(None);
            }
        }

        let mut requirements = Vec::new();
        let mut used_extras = FxHashSet::default();

        // Include the default dependencies.
        requirements.extend(project.dependencies.unwrap_or_default());

        // Include any optional dependencies specified in `extras`.
        if !extras.is_empty() {
            if let Some(optional_dependencies) = project.optional_dependencies {
                for (extra_name, optional_requirements) in &optional_dependencies {
                    let normalized_name = ExtraName::from_str(extra_name)
                        .with_context(|| format!("Invalid extra name `{extra_name}`"))?;
                    if extras.contains(&normalized_name) {
                        used_extras.insert(normalized_name);
                        requirements.extend(flatten_extra(
                            &name,
                            optional_requirements,
                            &optional_dependencies,
                        )?);
                    }
                }
            }
        }

        Ok(Some(Self {
            name,
            requirements,
            used_extras,
        }))
    }
}

/// Given an extra in a project that may contain references to the project
/// itself, flatten it into a list of requirements.
///
/// For example:
/// ```toml
/// [project]
/// name = "my-project"
/// version = "0.0.1"
/// dependencies = [
///     "tomli",
/// ]
///
/// [project.optional-dependencies]
/// test = [
///     "pep517",
/// ]
/// dev = [
///     "my-project[test]",
/// ]
/// ```
fn flatten_extra(
    project_name: &PackageName,
    requirements: &[Requirement],
    extras: &IndexMap<String, Vec<Requirement>>,
) -> Result<Vec<Requirement>> {
    fn inner(
        project_name: &PackageName,
        requirements: &[Requirement],
        extras: &IndexMap<String, Vec<Requirement>>,
        seen: &mut FxHashSet<ExtraName>,
    ) -> Result<Vec<Requirement>> {
        let mut flattened = Vec::with_capacity(requirements.len());
        for requirement in requirements {
            if requirement.name == *project_name {
                for extra in &requirement.extras {
                    // Avoid infinite recursion on mutually recursive extras.
                    if !seen.insert(extra.clone()) {
                        continue;
                    }

                    // Flatten the extra requirements.
                    for (name, extra_requirements) in extras {
                        let normalized_name = ExtraName::from_str(name)?;
                        if normalized_name == *extra {
                            flattened.extend(inner(
                                project_name,
                                extra_requirements,
                                extras,
                                seen,
                            )?);
                        }
                    }
                }
            } else {
                flattened.push(requirement.clone());
            }
        }
        Ok(flattened)
    }

    inner(
        project_name,
        requirements,
        extras,
        &mut FxHashSet::default(),
    )
}

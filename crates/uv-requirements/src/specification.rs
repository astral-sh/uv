//! Collecting the requirements to compile, sync or install.
//!
//! # `requirements.txt` format
//!
//! The `requirements.txt` format (also known as `requirements.in`) is static except for the
//! possibility of making network requests.
//!
//! All entries are stored as `requirements` and `editables` or `constraints`  depending on the kind
//! of inclusion (`uv pip install -r` and `uv pip compile` vs. `uv pip install -c` and
//! `uv pip compile -c`).
//!
//! # `pyproject.toml` and directory source.
//!
//! `pyproject.toml` files come in two forms: PEP 621 compliant with static dependencies and non-PEP 621
//! compliant or PEP 621 compliant with dynamic metadata. There are different ways how the requirements are evaluated:
//! * `uv pip install -r pyproject.toml` or `uv pip compile requirements.in`: The `pyproject.toml`
//!   must be valid (in other circumstances we allow invalid `dependencies` e.g. for hatch's
//!   relative path support), but it can be dynamic. We set the `project` from the `name` entry. If it is static, we add
//!   all `dependencies` from the pyproject.toml as `requirements` (and drop the directory). If it
//!   is dynamic, we add the directory to `source_trees`.
//! * `uv pip install .` in a directory with `pyproject.toml` or `uv pip compile requirements.in`
//!   where the `requirements.in` points to that directory: The directory is listed in
//!   `requirements`. The lookahead resolver reads the static metadata from `pyproject.toml` if
//!   available, otherwise it calls PEP 517 to resolve.
//! * `uv pip install -e`: We add the directory in `editables` instead of `requirements`. The
//!   lookahead resolver resolves it the same.
//! * `setup.py` or `setup.cfg` instead of `pyproject.toml`: Directory is an entry in
//!   `source_trees`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rustc_hash::FxHashSet;
use tracing::instrument;

use cache_key::CanonicalUrl;
use distribution_types::{
    FlatIndexLocation, IndexUrl, UnresolvedRequirement, UnresolvedRequirementSpecification,
};
use pep508_rs::{UnnamedRequirement, UnnamedRequirementUrl};
use pypi_types::Requirement;
use pypi_types::VerbatimParsedUrl;
use requirements_txt::{RequirementsTxt, RequirementsTxtRequirement};
use uv_client::BaseClientBuilder;
use uv_configuration::{NoBinary, NoBuild};
use uv_fs::{Simplified, CWD};
use uv_normalize::{ExtraName, PackageName};
use uv_workspace::pyproject::PyProjectToml;

use crate::RequirementsSource;

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
        client_builder: &BaseClientBuilder<'_>,
    ) -> Result<Self> {
        Ok(match source {
            RequirementsSource::Package(name) => {
                let requirement = RequirementsTxtRequirement::parse(name, &*CWD, false)
                    .with_context(|| format!("Failed to parse: `{name}`"))?;
                Self {
                    requirements: vec![UnresolvedRequirementSpecification::from(requirement)],
                    ..Self::default()
                }
            }
            RequirementsSource::Editable(name) => {
                let requirement = RequirementsTxtRequirement::parse(name, &*CWD, true)
                    .with_context(|| format!("Failed to parse: `{name}`"))?;
                Self {
                    requirements: vec![UnresolvedRequirementSpecification::from(
                        requirement.into_editable()?,
                    )],
                    ..Self::default()
                }
            }
            RequirementsSource::RequirementsTxt(path) => {
                if !(path == Path::new("-")
                    || path.starts_with("http://")
                    || path.starts_with("https://")
                    || path.exists())
                {
                    return Err(anyhow::anyhow!("File not found: `{}`", path.user_display()));
                }

                let requirements_txt = RequirementsTxt::parse(path, &*CWD, client_builder).await?;
                Self {
                    requirements: requirements_txt
                        .requirements
                        .into_iter()
                        .map(UnresolvedRequirementSpecification::from)
                        .chain(
                            requirements_txt
                                .editables
                                .into_iter()
                                .map(UnresolvedRequirementSpecification::from),
                        )
                        .collect(),
                    constraints: requirements_txt
                        .constraints
                        .into_iter()
                        .map(Requirement::from)
                        .collect(),
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
                        .map(FlatIndexLocation::from)
                        .collect(),
                    no_binary: requirements_txt.no_binary,
                    no_build: requirements_txt.only_binary,
                    ..Self::default()
                }
            }
            RequirementsSource::PyprojectToml(path) => {
                let contents = match fs_err::tokio::read_to_string(&path).await {
                    Ok(contents) => contents,
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                        return Err(anyhow::anyhow!("File not found: `{}`", path.user_display()));
                    }
                    Err(err) => {
                        return Err(anyhow::anyhow!(
                            "Failed to read `{}`: {}",
                            path.user_display(),
                            err
                        ));
                    }
                };
                let _ = toml::from_str::<PyProjectToml>(&contents)
                    .with_context(|| format!("Failed to parse: `{}`", path.user_display()))?;

                Self {
                    source_trees: vec![path.clone()],
                    ..Self::default()
                }
            }
            RequirementsSource::SetupPy(path) | RequirementsSource::SetupCfg(path) => {
                if !path.is_file() {
                    return Err(anyhow::anyhow!("File not found: `{}`", path.user_display()));
                }

                Self {
                    source_trees: vec![path.clone()],
                    ..Self::default()
                }
            }
            RequirementsSource::SourceTree(path) => {
                if !path.is_dir() {
                    return Err(anyhow::anyhow!(
                        "Directory not found: `{}`",
                        path.user_display()
                    ));
                }

                Self {
                    project: None,
                    requirements: vec![UnresolvedRequirementSpecification {
                        requirement: UnresolvedRequirement::Unnamed(UnnamedRequirement {
                            url: VerbatimParsedUrl::parse_absolute_path(path)?,
                            extras: vec![],
                            marker: None,
                            origin: None,
                        }),
                        hashes: vec![],
                    }],
                    ..Self::default()
                }
            }
        })
    }

    /// Read the combined requirements and constraints from a set of sources.
    pub async fn from_sources(
        requirements: &[RequirementsSource],
        constraints: &[RequirementsSource],
        overrides: &[RequirementsSource],
        client_builder: &BaseClientBuilder<'_>,
    ) -> Result<Self> {
        let mut spec = Self::default();

        // Read all requirements, and keep track of all requirements _and_ constraints.
        // A `requirements.txt` can contain a `-c constraints.txt` directive within it, so reading
        // a requirements file can also add constraints.
        for source in requirements {
            let source = Self::from_source(source, client_builder).await?;
            spec.requirements.extend(source.requirements);
            spec.constraints.extend(source.constraints);
            spec.overrides.extend(source.overrides);
            spec.extras.extend(source.extras);
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
            let source = Self::from_source(source, client_builder).await?;
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
            let source = Self::from_source(source, client_builder).await?;
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

    /// Parse an individual package requirement.
    pub fn parse_package(name: &str) -> Result<UnresolvedRequirementSpecification> {
        let requirement = RequirementsTxtRequirement::parse(name, &*CWD, false)
            .with_context(|| format!("Failed to parse: `{name}`"))?;
        Ok(UnresolvedRequirementSpecification::from(requirement))
    }

    /// Read the requirements from a set of sources.
    pub async fn from_simple_sources(
        requirements: &[RequirementsSource],
        client_builder: &BaseClientBuilder<'_>,
    ) -> Result<Self> {
        Self::from_sources(requirements, &[], &[], client_builder).await
    }

    /// Initialize a [`RequirementsSpecification`] from a list of [`Requirement`].
    pub fn from_requirements(requirements: Vec<Requirement>) -> Self {
        Self {
            requirements: requirements
                .into_iter()
                .map(UnresolvedRequirementSpecification::from)
                .collect(),
            ..Self::default()
        }
    }

    /// Return true if the specification does not include any requirements to install.
    pub fn is_empty(&self) -> bool {
        self.requirements.is_empty() && self.source_trees.is_empty() && self.overrides.is_empty()
    }
}

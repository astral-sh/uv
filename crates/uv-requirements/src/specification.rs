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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rustc_hash::FxHashSet;
use tracing::instrument;
use uv_cache_key::CanonicalUrl;
use uv_client::BaseClientBuilder;
use uv_configuration::{DependencyGroups, NoBinary, NoBuild};
use uv_distribution_types::{
    IndexUrl, NameRequirementSpecification, UnresolvedRequirement,
    UnresolvedRequirementSpecification,
};
use uv_fs::{Simplified, CWD};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::{MarkerTree, UnnamedRequirement, UnnamedRequirementUrl};
use uv_pypi_types::Requirement;
use uv_pypi_types::VerbatimParsedUrl;
use uv_requirements_txt::{RequirementsTxt, RequirementsTxtRequirement};
use uv_warnings::warn_user;
use uv_workspace::pyproject::PyProjectToml;

use crate::RequirementsSource;

#[derive(Debug, Default, Clone)]
pub struct RequirementsSpecification {
    /// The name of the project specifying requirements.
    pub project: Option<PackageName>,
    /// The requirements for the project.
    pub requirements: Vec<UnresolvedRequirementSpecification>,
    /// The constraints for the project.
    pub constraints: Vec<NameRequirementSpecification>,
    /// The overrides for the project.
    pub overrides: Vec<UnresolvedRequirementSpecification>,
    /// The source trees from which to extract requirements.
    pub source_trees: Vec<PathBuf>,
    /// The groups to use for `source_trees`
    pub groups: BTreeMap<PathBuf, DependencyGroups>,
    /// The extras used to collect requirements.
    pub extras: FxHashSet<ExtraName>,
    /// The index URL to use for fetching packages.
    pub index_url: Option<IndexUrl>,
    /// The extra index URLs to use for fetching packages.
    pub extra_index_urls: Vec<IndexUrl>,
    /// Whether to disallow index usage.
    pub no_index: bool,
    /// The `--find-links` locations to use for fetching packages.
    pub find_links: Vec<IndexUrl>,
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
            RequirementsSource::Package(requirement) => Self {
                requirements: vec![UnresolvedRequirementSpecification::from(
                    requirement.clone(),
                )],
                ..Self::default()
            },
            RequirementsSource::Editable(requirement) => Self {
                requirements: vec![UnresolvedRequirementSpecification::from(
                    requirement.clone().into_editable()?,
                )],
                ..Self::default()
            },
            RequirementsSource::RequirementsTxt(path) => {
                if !(path == Path::new("-")
                    || path.starts_with("http://")
                    || path.starts_with("https://")
                    || path.exists())
                {
                    return Err(anyhow::anyhow!("File not found: `{}`", path.user_display()));
                }

                let requirements_txt = RequirementsTxt::parse(path, &*CWD, client_builder).await?;

                if requirements_txt == RequirementsTxt::default() {
                    if path == Path::new("-") {
                        warn_user!("No dependencies found in stdin");
                    } else {
                        warn_user!(
                            "Requirements file `{}` does not contain any dependencies",
                            path.user_display()
                        );
                    }
                }

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
                        .map(NameRequirementSpecification::from)
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
                        .map(IndexUrl::from)
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
                            marker: MarkerTree::TRUE,
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
        groups: BTreeMap<PathBuf, Vec<GroupName>>,
        client_builder: &BaseClientBuilder<'_>,
    ) -> Result<Self> {
        let mut spec = Self::default();

        // Resolve sources into specifications so we know their `source_tree`s∂
        let mut requirement_sources = Vec::new();
        for source in requirements {
            let source = Self::from_source(source, client_builder).await?;
            requirement_sources.push(source);
        }

        // pip `--group` flags specify their own sources, which we need to process here
        if !groups.is_empty() {
            let mut group_specs = BTreeMap::new();
            for (path, groups) in groups {
                // Conceptually pip `--group` flags just add the group referred to by the file.
                // In uv semantics this would be like `--only-group`, however if you do this:
                //
                //    uv pip install -r pyproject.toml --group pyproject.toml:foo
                //
                // We don't want to discard the package listed by `-r` in the way `--only-group`
                // would. So we check to see if any other source wants to add this path, and use
                // that to determine if we're doing `--group` or `--only-group` semantics.
                //
                // Note that it's fine if a file gets referred to multiple times by
                // different-looking paths (like `./pyproject.toml` vs `pyproject.toml`). We're
                // specifically trying to disambiguate in situations where the `--group` *happens*
                // to match with an unrelated argument, and `--only-group` would be overzealous!
                let source_exists_without_group = requirement_sources
                    .iter()
                    .any(|source| source.source_trees.contains(&path));
                let (group, only_group) = if source_exists_without_group {
                    (groups, Vec::new())
                } else {
                    (Vec::new(), groups)
                };
                let group_spec = DependencyGroups::from_args(
                    false,
                    false,
                    false,
                    group,
                    Vec::new(),
                    false,
                    only_group,
                    false,
                );

                // If we're doing `--only-group` semantics it's because only `--group` flags referred
                // to this file, and so we need to make sure to add it to the list of sources!
                if !source_exists_without_group {
                    let source = Self::from_source(
                        &RequirementsSource::PyprojectToml(path.clone()),
                        client_builder,
                    )
                    .await?;
                    requirement_sources.push(source);
                }

                group_specs.insert(path, group_spec);
            }

            spec.groups = group_specs;
        }

        // Read all requirements, and keep track of all requirements _and_ constraints.
        // A `requirements.txt` can contain a `-c constraints.txt` directive within it, so reading
        // a requirements file can also add constraints.
        for source in requirement_sources {
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
        // Overrides are ignored.
        for source in constraints {
            let source = Self::from_source(source, client_builder).await?;
            for entry in source.requirements {
                match entry.requirement {
                    UnresolvedRequirement::Named(requirement) => {
                        spec.constraints.push(NameRequirementSpecification {
                            requirement,
                            hashes: entry.hashes,
                        });
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
        Self::from_sources(requirements, &[], &[], BTreeMap::default(), client_builder).await
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

    /// Initialize a [`RequirementsSpecification`] from a list of [`Requirement`], including
    /// constraints.
    pub fn from_constraints(requirements: Vec<Requirement>, constraints: Vec<Requirement>) -> Self {
        Self {
            requirements: requirements
                .into_iter()
                .map(UnresolvedRequirementSpecification::from)
                .collect(),
            constraints: constraints
                .into_iter()
                .map(NameRequirementSpecification::from)
                .collect(),
            ..Self::default()
        }
    }

    /// Initialize a [`RequirementsSpecification`] from a list of [`Requirement`], including
    /// constraints and overrides.
    pub fn from_overrides(
        requirements: Vec<Requirement>,
        constraints: Vec<Requirement>,
        overrides: Vec<Requirement>,
    ) -> Self {
        Self {
            requirements: requirements
                .into_iter()
                .map(UnresolvedRequirementSpecification::from)
                .collect(),
            constraints: constraints
                .into_iter()
                .map(NameRequirementSpecification::from)
                .collect(),
            overrides: overrides
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

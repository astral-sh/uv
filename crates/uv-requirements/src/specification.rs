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
use std::str::FromStr;

use anyhow::{Context, Result};
use rustc_hash::FxHashSet;
use serde::Deserialize;
use tracing::instrument;
use url::Url;

use uv_cache_key::CanonicalUrl;
use uv_client::BaseClientBuilder;
use uv_configuration::{DependencyGroups, NoBinary, NoBuild};
use uv_distribution_types::Requirement;
use uv_distribution_types::{
    IndexUrl, NameRequirementSpecification, UnresolvedRequirement,
    UnresolvedRequirementSpecification,
};
use uv_fs::{CWD, Simplified};
use uv_normalize::{ExtraName, PackageName, PipGroupName};
use uv_redacted::DisplaySafeUrl;
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
    /// The `pylock.toml` file from which to extract the resolution.
    pub pylock: Option<PathBuf>,
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
                // Check if file exists or is a remote URL (similar to RequirementsTxt case)
                if !(path == Path::new("-")
                    || path.starts_with("http://")
                    || path.starts_with("https://")
                    || path.exists())
                {
                    return Err(anyhow::anyhow!("File not found: `{}`", path.user_display()));
                }

                // Fetch content from remote URL or read local file
                let contents = if path.starts_with("http://") || path.starts_with("https://") {
                    Self::fetch_remote_pyproject_content(path, client_builder).await?
                } else {
                    match fs_err::tokio::read_to_string(&path).await {
                        Ok(contents) => contents,
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                            return Err(anyhow::anyhow!(
                                "File not found: `{}`",
                                path.user_display()
                            ));
                        }
                        Err(err) => {
                            return Err(anyhow::anyhow!(
                                "Failed to read `{}`: {}",
                                path.user_display(),
                                err
                            ));
                        }
                    }
                };

                // Parse the pyproject.toml content
                let pyproject = toml::from_str::<PyProjectToml>(&contents)
                    .with_context(|| format!("Failed to parse: `{}`", path.user_display()))?;

                // For remote files, check for dynamic metadata and reject if found
                if (path.starts_with("http://") || path.starts_with("https://"))
                    && Self::has_dynamic_metadata(&contents)?
                {
                    return Err(anyhow::anyhow!(
                        "Remote pyproject.toml files with dynamic metadata are not supported. \
                         Consider using a Git dependency instead: `{}`",
                        path.user_display()
                    ));
                }

                // For remote files with static metadata, extract dependencies directly
                // For local files, use source_trees approach (existing behavior)
                if path.starts_with("http://") || path.starts_with("https://") {
                    // Remote file: extract static dependencies and add as requirements
                    let mut requirements = Vec::new();

                    if let Some(project) = pyproject.project {
                        if let Some(dependencies) = project.dependencies {
                            for dep in dependencies {
                                let requirement = RequirementsTxtRequirement::parse(&dep, &*CWD, false)
                                    .with_context(|| format!("Failed to parse dependency `{dep}` from remote pyproject.toml"))?;
                                requirements
                                    .push(UnresolvedRequirementSpecification::from(requirement));
                            }
                        }
                    }

                    Self {
                        requirements,
                        ..Self::default()
                    }
                } else {
                    // Local file: use existing source_trees approach
                    Self {
                        source_trees: vec![path.clone()],
                        ..Self::default()
                    }
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
            RequirementsSource::PylockToml(path) => {
                if !path.is_file() {
                    return Err(anyhow::anyhow!("File not found: `{}`", path.user_display()));
                }

                Self {
                    pylock: Some(path.clone()),
                    ..Self::default()
                }
            }
            RequirementsSource::EnvironmentYml(path) => {
                return Err(anyhow::anyhow!(
                    "Conda environment files (i.e., `{}`) are not supported",
                    path.user_display()
                ));
            }
        })
    }

    /// Read the combined requirements and constraints from a set of sources.
    pub async fn from_sources(
        requirements: &[RequirementsSource],
        constraints: &[RequirementsSource],
        overrides: &[RequirementsSource],
        groups: Option<&GroupsSpecification>,
        client_builder: &BaseClientBuilder<'_>,
    ) -> Result<Self> {
        let mut spec = Self::default();

        // Disallow `pylock.toml` files as constraints.
        if let Some(pylock_toml) = constraints.iter().find_map(|source| {
            if let RequirementsSource::PylockToml(path) = source {
                Some(path)
            } else {
                None
            }
        }) {
            return Err(anyhow::anyhow!(
                "Cannot use `{}` as a constraint file",
                pylock_toml.user_display()
            ));
        }

        // Disallow `pylock.toml` files as overrides.
        if let Some(pylock_toml) = overrides.iter().find_map(|source| {
            if let RequirementsSource::PylockToml(path) = source {
                Some(path)
            } else {
                None
            }
        }) {
            return Err(anyhow::anyhow!(
                "Cannot use `{}` as an override file",
                pylock_toml.user_display()
            ));
        }

        // If we have a `pylock.toml`, don't allow additional requirements, constraints, or
        // overrides.
        if let Some(pylock_toml) = requirements.iter().find_map(|source| {
            if let RequirementsSource::PylockToml(path) = source {
                Some(path)
            } else {
                None
            }
        }) {
            if requirements
                .iter()
                .any(|source| !matches!(source, RequirementsSource::PylockToml(..)))
            {
                return Err(anyhow::anyhow!(
                    "Cannot specify additional requirements alongside a `pylock.toml` file",
                ));
            }
            if !constraints.is_empty() {
                return Err(anyhow::anyhow!(
                    "Cannot specify additional requirements with a `pylock.toml` file"
                ));
            }
            if !overrides.is_empty() {
                return Err(anyhow::anyhow!(
                    "Cannot specify constraints with a `pylock.toml` file"
                ));
            }

            // If we have a `pylock.toml`, disallow specifying paths for groups; instead, require
            // that all groups refer to the `pylock.toml` file.
            if let Some(groups) = groups {
                let mut names = Vec::new();
                for group in &groups.groups {
                    if group.path.is_some() {
                        return Err(anyhow::anyhow!(
                            "Cannot specify paths for groups with a `pylock.toml` file; all groups must refer to the `pylock.toml` file"
                        ));
                    }
                    names.push(group.name.clone());
                }

                if !names.is_empty() {
                    spec.groups.insert(
                        pylock_toml.clone(),
                        DependencyGroups::from_args(
                            false,
                            false,
                            false,
                            Vec::new(),
                            Vec::new(),
                            false,
                            names,
                            false,
                        ),
                    );
                }
            }
        } else if let Some(groups) = groups {
            // pip `--group` flags specify their own sources, which we need to process here.
            // First, we collect all groups by their path.
            let mut groups_by_path = BTreeMap::new();
            for group in &groups.groups {
                // If there's no path provided, expect a pyproject.toml in the project-dir
                // (Which is typically the current working directory, matching pip's behaviour)
                let pyproject_path = group
                    .path
                    .clone()
                    .unwrap_or_else(|| groups.root.join("pyproject.toml"));
                groups_by_path
                    .entry(pyproject_path)
                    .or_insert_with(Vec::new)
                    .push(group.name.clone());
            }

            let mut group_specs = BTreeMap::new();
            for (path, groups) in groups_by_path {
                let group_spec = DependencyGroups::from_args(
                    false,
                    false,
                    false,
                    Vec::new(),
                    Vec::new(),
                    false,
                    groups,
                    false,
                );
                group_specs.insert(path, group_spec);
            }
            spec.groups = group_specs;
        }

        // Resolve sources into specifications so we know their `source_tree`.
        let mut requirement_sources = Vec::new();
        for source in requirements {
            let source = Self::from_source(source, client_builder).await?;
            requirement_sources.push(source);
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

            // Allow at most one `pylock.toml`.
            if let Some(pylock) = source.pylock {
                if let Some(existing) = spec.pylock {
                    return Err(anyhow::anyhow!(
                        "Multiple `pylock.toml` files specified: `{}` vs. `{}`",
                        existing.user_display(),
                        pylock.user_display()
                    ));
                }
                spec.pylock = Some(pylock);
            }

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
        Self::from_sources(requirements, &[], &[], None, client_builder).await
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

    /// Fetch the contents of a remote pyproject.toml file.
    async fn fetch_remote_pyproject_content(
        path: &Path,
        client_builder: &BaseClientBuilder<'_>,
    ) -> Result<String> {
        // Check if network is disabled
        if client_builder.is_offline() {
            return Err(anyhow::anyhow!(
                "Network connectivity is disabled, but a remote pyproject.toml file was requested: {}",
                path.display()
            ));
        }

        // Convert path to UTF-8 string for URL parsing
        let path_utf8 = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Non-Unicode URL: {}", path.display()))?;

        // Parse URL and fetch content
        let url = DisplaySafeUrl::from_str(path_utf8)
            .with_context(|| format!("Invalid URL: {path_utf8}"))?;

        let client = client_builder.build();
        let response = client
            .for_host(&url)
            .get(Url::from(url.clone()))
            .send()
            .await
            .with_context(|| format!("Failed to fetch remote pyproject.toml: {url}"))?;

        let text = response
            .error_for_status()
            .with_context(|| format!("Error while accessing remote pyproject.toml: {url}"))?
            .text()
            .await
            .with_context(|| format!("Failed to read response body from: {url}"))?;

        Ok(text)
    }

    /// Check if a pyproject.toml file contains dynamic metadata.
    fn has_dynamic_metadata(contents: &str) -> Result<bool> {
        // Simple struct to parse just the project.dynamic field
        #[derive(Deserialize)]
        struct ProjectToml {
            project: Option<ProjectSection>,
        }

        #[derive(Deserialize)]
        struct ProjectSection {
            dynamic: Option<Vec<String>>,
        }

        // Parse the TOML content to check for dynamic metadata
        let parsed: ProjectToml = toml::from_str(contents)
            .with_context(|| "Failed to parse pyproject.toml content for dynamic metadata check")?;

        // Check if [project.dynamic] exists and is non-empty
        let has_dynamic = parsed
            .project
            .and_then(|project| project.dynamic)
            .is_some_and(|dynamic_list| !dynamic_list.is_empty());

        Ok(has_dynamic)
    }
}

#[derive(Debug, Default, Clone)]
pub struct GroupsSpecification {
    /// The path to the project root, relative to which the default `pyproject.toml` file is
    /// located.
    pub root: PathBuf,
    /// The enabled groups.
    pub groups: Vec<PipGroupName>,
}

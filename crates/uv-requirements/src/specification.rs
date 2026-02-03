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
use url::Url;

use uv_cache_key::CanonicalUrl;
use uv_client::BaseClientBuilder;
use uv_configuration::{DependencyGroups, NoBinary, NoBuild};
use uv_distribution_types::{Index, Requirement};
use uv_distribution_types::{
    IndexUrl, NameRequirementSpecification, UnresolvedRequirement,
    UnresolvedRequirementSpecification,
};
use uv_fs::{CWD, Simplified};
use uv_normalize::{ExtraName, PackageName, PipGroupName};
use uv_pypi_types::PyProjectToml;
use uv_redacted::DisplaySafeUrl;
use uv_requirements_txt::{RequirementsTxt, RequirementsTxtRequirement, SourceCache};
use uv_scripts::Pep723Metadata;
use uv_warnings::warn_user;

use crate::{RequirementsSource, SourceTree};

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
    /// The excludes for the project.
    pub excludes: Vec<PackageName>,
    /// The `pylock.toml` file from which to extract the resolution.
    pub pylock: Option<PathBuf>,
    /// The source trees from which to extract requirements.
    pub source_trees: Vec<SourceTree>,
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
        Self::from_source_with_cache(source, client_builder, &mut SourceCache::default()).await
    }

    /// Create a [`RequirementsSpecification`] from PEP 723 script metadata.
    fn from_pep723_metadata(metadata: &Pep723Metadata) -> Self {
        let requirements = metadata
            .dependencies
            .as_ref()
            .map(|dependencies| {
                dependencies
                    .iter()
                    .map(|dependency| {
                        UnresolvedRequirementSpecification::from(Requirement::from(
                            dependency.to_owned(),
                        ))
                    })
                    .collect::<Vec<UnresolvedRequirementSpecification>>()
            })
            .unwrap_or_default();

        if let Some(tool_uv) = metadata.tool.as_ref().and_then(|tool| tool.uv.as_ref()) {
            let constraints = tool_uv
                .constraint_dependencies
                .as_ref()
                .map(|dependencies| {
                    dependencies
                        .iter()
                        .map(|dependency| {
                            NameRequirementSpecification::from(Requirement::from(
                                dependency.to_owned(),
                            ))
                        })
                        .collect::<Vec<NameRequirementSpecification>>()
                })
                .unwrap_or_default();

            let overrides = tool_uv
                .override_dependencies
                .as_ref()
                .map(|dependencies| {
                    dependencies
                        .iter()
                        .map(|dependency| {
                            UnresolvedRequirementSpecification::from(Requirement::from(
                                dependency.to_owned(),
                            ))
                        })
                        .collect::<Vec<UnresolvedRequirementSpecification>>()
                })
                .unwrap_or_default();

            Self {
                requirements,
                constraints,
                overrides,
                index_url: tool_uv
                    .top_level
                    .index_url
                    .as_ref()
                    .map(|index| Index::from(index.clone()).url),
                extra_index_urls: tool_uv
                    .top_level
                    .extra_index_url
                    .as_ref()
                    .into_iter()
                    .flat_map(|urls| urls.iter().map(|index| Index::from(index.clone()).url))
                    .collect(),
                no_index: tool_uv.top_level.no_index.unwrap_or_default(),
                find_links: tool_uv
                    .top_level
                    .find_links
                    .as_ref()
                    .into_iter()
                    .flat_map(|urls| urls.iter().map(|index| Index::from(index.clone()).url))
                    .collect(),
                no_binary: NoBinary::from_args(
                    tool_uv.top_level.no_binary,
                    tool_uv
                        .top_level
                        .no_binary_package
                        .clone()
                        .unwrap_or_default(),
                ),
                no_build: NoBuild::from_args(
                    tool_uv.top_level.no_build,
                    tool_uv
                        .top_level
                        .no_build_package
                        .clone()
                        .unwrap_or_default(),
                ),
                ..Self::default()
            }
        } else {
            Self {
                requirements,
                ..Self::default()
            }
        }
    }

    /// Create a [`RequirementsSpecification`] from a parsed `requirements.txt` file.
    fn from_requirements_txt(requirements_txt: RequirementsTxt) -> Self {
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

    /// Read the requirements and constraints from a source, using a cache for file contents.
    #[instrument(skip_all, level = tracing::Level::DEBUG, fields(source = % source))]
    pub async fn from_source_with_cache(
        source: &RequirementsSource,
        client_builder: &BaseClientBuilder<'_>,
        cache: &mut SourceCache,
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
                if !(path.starts_with("http://") || path.starts_with("https://") || path.exists()) {
                    return Err(anyhow::anyhow!("File not found: `{}`", path.user_display()));
                }

                let requirements_txt =
                    RequirementsTxt::parse_with_cache(path, &*CWD, client_builder, cache).await?;

                if requirements_txt == RequirementsTxt::default() {
                    warn_user!(
                        "Requirements file `{}` does not contain any dependencies",
                        path.user_display()
                    );
                }

                Self::from_requirements_txt(requirements_txt)
            }
            RequirementsSource::PyprojectToml(path) => {
                let content = match fs_err::tokio::read_to_string(&path).await {
                    Ok(content) => content,
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
                let pyproject_toml = toml::from_str::<PyProjectToml>(&content)
                    .with_context(|| format!("Failed to parse: `{}`", path.user_display()))?;

                Self {
                    source_trees: vec![SourceTree::PyProjectToml(path.clone(), pyproject_toml)],
                    ..Self::default()
                }
            }
            RequirementsSource::Pep723Script(path) => {
                let content = if let Some(content) = cache.get(path.as_path()) {
                    content.clone()
                } else {
                    let content = read_file(path, client_builder).await?;
                    cache.insert(path.clone(), content.clone());
                    content
                };

                let metadata = match Pep723Metadata::parse(content.as_bytes()) {
                    Ok(Some(script)) => script,
                    Ok(None) => {
                        return Err(anyhow::anyhow!(
                            "`{}` does not contain inline script metadata",
                            path.user_display(),
                        ));
                    }
                    Err(err) => return Err(err.into()),
                };

                Self::from_pep723_metadata(&metadata)
            }
            RequirementsSource::SetupPy(path) => {
                if !path.is_file() {
                    return Err(anyhow::anyhow!("File not found: `{}`", path.user_display()));
                }

                Self {
                    source_trees: vec![SourceTree::SetupPy(path.clone())],
                    ..Self::default()
                }
            }
            RequirementsSource::SetupCfg(path) => {
                if !path.is_file() {
                    return Err(anyhow::anyhow!("File not found: `{}`", path.user_display()));
                }

                Self {
                    source_trees: vec![SourceTree::SetupCfg(path.clone())],
                    ..Self::default()
                }
            }
            RequirementsSource::PylockToml(path) => {
                if !(path.starts_with("http://") || path.starts_with("https://") || path.exists()) {
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
            RequirementsSource::Extensionless(path) => {
                let content = if let Some(content) = cache.get(path.as_path()) {
                    content.clone()
                } else {
                    let content = read_file(path, client_builder).await?;
                    cache.insert(path.clone(), content.clone());
                    content
                };

                // Detect if it's a PEP 723 script.
                if let Some(metadata) = Pep723Metadata::parse(content.as_bytes())? {
                    Self::from_pep723_metadata(&metadata)
                } else {
                    // If it's not a PEP 723 script, assume it's a `requirements.txt` file.
                    let requirements_txt =
                        RequirementsTxt::parse_str(&content, &path, &*CWD, client_builder, cache)
                            .await?;

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

                    Self::from_requirements_txt(requirements_txt)
                }
            }
        })
    }

    /// Read the combined requirements and constraints from a set of sources.
    pub async fn from_sources(
        requirements: &[RequirementsSource],
        constraints: &[RequirementsSource],
        overrides: &[RequirementsSource],
        excludes: &[RequirementsSource],
        groups: Option<&GroupsSpecification>,
        client_builder: &BaseClientBuilder<'_>,
    ) -> Result<Self> {
        let mut spec = Self::default();
        let mut cache = SourceCache::default();

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

        // Disallow `pylock.toml` files as excludes.
        if let Some(pylock_toml) = excludes.iter().find_map(|source| {
            if let RequirementsSource::PylockToml(path) = source {
                Some(path)
            } else {
                None
            }
        }) {
            return Err(anyhow::anyhow!(
                "Cannot use `{}` as an exclude file",
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
                    "Cannot specify constraints with a `pylock.toml` file"
                ));
            }
            if !overrides.is_empty() {
                return Err(anyhow::anyhow!(
                    "Cannot specify overrides with a `pylock.toml` file"
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
            let source = Self::from_source_with_cache(source, client_builder, &mut cache).await?;
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
            let source = Self::from_source_with_cache(source, client_builder, &mut cache).await?;
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
            let source = Self::from_source_with_cache(source, client_builder, &mut cache).await?;
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

        // Collect excludes.
        for source in excludes {
            let source = Self::from_source_with_cache(source, client_builder, &mut cache).await?;
            for req_spec in source.requirements {
                match req_spec.requirement {
                    UnresolvedRequirement::Named(requirement) => {
                        spec.excludes.push(requirement.name);
                    }
                    UnresolvedRequirement::Unnamed(requirement) => {
                        return Err(anyhow::anyhow!(
                            "Unnamed requirements are not allowed as exclusions (found: `{requirement}`)"
                        ));
                    }
                }
            }
            spec.excludes.extend(source.excludes.into_iter());
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
        Self::from_sources(requirements, &[], &[], &[], None, client_builder).await
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

#[derive(Debug, Default, Clone)]
pub struct GroupsSpecification {
    /// The path to the project root, relative to which the default `pyproject.toml` file is
    /// located.
    pub root: PathBuf,
    /// The enabled groups.
    pub groups: Vec<PipGroupName>,
}

/// Read the contents of a path, fetching over HTTP(S) if necessary.
async fn read_file(path: &Path, client_builder: &BaseClientBuilder<'_>) -> Result<String> {
    // If the path is a URL, fetch it over HTTP(S).
    if path.starts_with("http://") || path.starts_with("https://") {
        // Only continue if we are absolutely certain no local file exists.
        //
        // We don't do this check on Windows since the file path would
        // be invalid anyway, and thus couldn't refer to a local file.
        if !cfg!(unix) || matches!(path.try_exists(), Ok(false)) {
            let url = DisplaySafeUrl::parse(&path.to_string_lossy())?;

            let client = client_builder.build();
            let response = client
                .for_host(&url)
                .get(Url::from(url.clone()))
                .send()
                .await?;

            response.error_for_status_ref()?;

            return Ok(response.text().await?);
        }
    }

    // Read the file content.
    let content = uv_fs::read_to_string_transcode(path).await?;
    Ok(content)
}

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

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use path_absolutize::Absolutize;
use rustc_hash::FxHashSet;
use same_file::is_same_file;
use tracing::{debug, instrument, trace};

use cache_key::CanonicalUrl;
use distribution_types::{
    FlatIndexLocation, IndexUrl, Requirement, RequirementSource, UnresolvedRequirement,
    UnresolvedRequirementSpecification,
};
use pep508_rs::{UnnamedRequirement, UnnamedRequirementUrl};
use pypi_types::VerbatimParsedUrl;
use requirements_txt::{
    EditableRequirement, FindLink, RequirementEntry, RequirementsTxt, RequirementsTxtRequirement,
};
use uv_client::BaseClientBuilder;
use uv_configuration::{NoBinary, NoBuild, PreviewMode};
use uv_fs::Simplified;
use uv_normalize::{ExtraName, PackageName};

use crate::pyproject::{Pep621Metadata, PyProjectToml};
use crate::ProjectWorkspace;
use crate::{ExtrasSpecification, RequirementsSource, Workspace, WorkspaceError};

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
        extras: &ExtrasSpecification,
        client_builder: &BaseClientBuilder<'_>,
        workspace: Option<&Workspace>,
        preview: PreviewMode,
    ) -> Result<Self> {
        Ok(match source {
            RequirementsSource::Package(name) => {
                let requirement = RequirementsTxtRequirement::parse(name, std::env::current_dir()?)
                    .with_context(|| format!("Failed to parse: `{name}`"))?;
                Self {
                    requirements: vec![UnresolvedRequirementSpecification::from(
                        RequirementEntry {
                            requirement,
                            hashes: vec![],
                        },
                    )],
                    ..Self::default()
                }
            }
            RequirementsSource::Editable(name) => {
                Self::from_editable_source(name, extras, workspace, preview).await?
            }
            RequirementsSource::RequirementsTxt(path) => {
                let requirements_txt =
                    RequirementsTxt::parse(path, std::env::current_dir()?, client_builder).await?;
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
                Self::from_pyproject_toml_source(path, extras, preview).await?
            }
            RequirementsSource::SetupPy(path) | RequirementsSource::SetupCfg(path) => Self {
                source_trees: vec![path.clone()],
                ..Self::default()
            },
            RequirementsSource::SourceTree(path) => Self {
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
            },
        })
    }

    async fn from_editable_source(
        name: &str,
        extras: &ExtrasSpecification,
        workspace: Option<&Workspace>,
        preview: PreviewMode,
    ) -> Result<RequirementsSpecification> {
        let requirement = EditableRequirement::parse(name, None, std::env::current_dir()?)
            .with_context(|| format!("Failed to parse: `{name}`"))?;

        // If we're not in preview mode, return the editable without searching for a workspace.
        if preview.is_disabled() {
            return Ok(Self {
                requirements: vec![UnresolvedRequirementSpecification::from(requirement)],
                ..Self::default()
            });
        }

        // First try to find the project in the existing workspace (if any), then try workspace
        // discovery.
        let project_in_exiting_workspace = workspace.and_then(|workspace| {
            // We use `is_same_file` instead of indexing by path to support different versions of
            // the same path (e.g. symlinks).
            workspace
                .packages()
                .values()
                .find(|member| is_same_file(member.root(), &requirement.path).unwrap_or(false))
                .map(|member| (member.pyproject_toml(), workspace))
        });

        let editable_spec = if let Some((pyproject_toml, workspace)) = project_in_exiting_workspace
        {
            debug!(
                "Found project in workspace at: `{}`",
                requirement.path.user_display()
            );

            Self::parse_direct_pyproject_toml(
                pyproject_toml,
                workspace,
                extras,
                requirement.path.as_ref(),
                preview,
            )
            .with_context(|| format!("Failed to parse: `{}`", requirement.path.user_display()))?
        } else if let Some(project_workspace) =
            ProjectWorkspace::from_maybe_project_root(&requirement.path).await?
        {
            debug!(
                "Found project at workspace root: `{}`",
                requirement.path.user_display()
            );

            let pyproject_toml = project_workspace.current_project().pyproject_toml();
            let workspace = project_workspace.workspace();
            Self::parse_direct_pyproject_toml(
                pyproject_toml,
                workspace,
                extras,
                requirement.path.as_ref(),
                preview,
            )
            .with_context(|| format!("Failed to parse: `{}`", requirement.path.user_display()))?
        } else {
            // No `pyproject.toml` or no static metadata also means no workspace support (at the
            // moment).
            debug!(
                "pyproject.toml has dynamic metadata at: `{}`",
                requirement.path.user_display()
            );

            return Ok(Self {
                requirements: vec![UnresolvedRequirementSpecification::from(requirement)],
                ..Self::default()
            });
        };

        if let Some(editable_spec) = editable_spec {
            // We only collect the editables here to keep the count of root packages correct.
            // TODO(konsti): Collect all workspace packages, even the non-editable ones.
            Ok(Self {
                requirements: editable_spec
                    .requirements
                    .into_iter()
                    .chain(std::iter::once(UnresolvedRequirementSpecification::from(
                        requirement,
                    )))
                    .filter(|entry| entry.requirement.is_editable())
                    .collect(),
                ..Self::default()
            })
        } else {
            debug!(
                "pyproject.toml has dynamic metadata at: `{}`",
                requirement.path.user_display()
            );
            Ok(Self {
                requirements: vec![UnresolvedRequirementSpecification::from(requirement)],
                ..Self::default()
            })
        }
    }

    async fn from_pyproject_toml_source(
        path: &Path,
        extras: &ExtrasSpecification,
        preview: PreviewMode,
    ) -> Result<RequirementsSpecification> {
        let dir = path.parent().context("pyproject.toml must have a parent")?;
        // We have to handle three cases:
        // * There is a workspace (possibly implicit) with static dependencies.
        // * There are dynamic dependencies, we have to build and don't use workspace information if
        //   present.
        // * There was an error during workspace discovery, such as an IO error or a
        //   `pyproject.toml` in the workspace not matching the (lenient) schema.
        match ProjectWorkspace::from_project_root(dir).await {
            Ok(project_workspace) => {
                let static_pyproject_toml = Self::parse_direct_pyproject_toml(
                    project_workspace.current_project().pyproject_toml(),
                    project_workspace.workspace(),
                    extras,
                    path,
                    preview,
                )
                .with_context(|| format!("Failed to parse: `{}`", path.user_display()))?;

                if let Some(static_pyproject_toml) = static_pyproject_toml {
                    Ok(static_pyproject_toml)
                } else {
                    debug!("Dynamic pyproject.toml at: `{}`", path.user_display());
                    Ok(Self {
                        source_trees: vec![path.to_path_buf()],
                        ..Self::default()
                    })
                }
            }
            Err(WorkspaceError::MissingProject(_)) => {
                debug!(
                    "Missing `project` table from pyproject.toml at: `{}`",
                    path.user_display()
                );
                Ok(Self {
                    source_trees: vec![path.to_path_buf()],
                    ..Self::default()
                })
            }
            Err(err) => Err(anyhow::Error::new(err)),
        }
    }

    /// Parse and lower a `pyproject.toml`, including all editable workspace dependencies.
    ///
    /// When dependency information is dynamic or invalid `project.dependencies` (e.g., Hatch's
    /// relative path support), we return `None` and query the metadata with PEP 517 later.
    pub(crate) fn parse_direct_pyproject_toml(
        pyproject: &PyProjectToml,
        workspace: &Workspace,
        extras: &ExtrasSpecification,
        pyproject_path: &Path,
        preview: PreviewMode,
    ) -> Result<Option<Self>> {
        // We need use this path as base for the relative paths inside pyproject.toml, so
        // we need the absolute path instead of a potentially relative path. E.g. with
        // `foo = { path = "../foo" }`, we will join `../foo` onto this path.
        let absolute_path = uv_fs::absolutize_path(pyproject_path)?;
        let project_dir = absolute_path
            .parent()
            .context("`pyproject.toml` has no parent directory")?;

        let Some(project) = Pep621Metadata::try_from(
            pyproject,
            extras,
            pyproject_path,
            project_dir,
            workspace,
            preview,
        )?
        else {
            debug!(
                "Dynamic pyproject.toml at: `{}`",
                pyproject_path.user_display()
            );
            return Ok(None);
        };

        if preview.is_disabled() {
            Ok(Some(Self {
                project: Some(project.name),
                requirements: project
                    .requirements
                    .into_iter()
                    .map(UnresolvedRequirementSpecification::from)
                    .collect(),
                extras: project.used_extras,
                ..Self::default()
            }))
        } else {
            Ok(Some(Self::collect_transitive_editables(
                workspace, extras, preview, project,
            )?))
        }
    }

    /// Perform a workspace dependency DAG traversal (breadth-first search) to collect all editables
    /// eagerly.
    ///
    /// Consider a requirement on A in a workspace with workspace packages A, B, C where
    /// A -> B and B -> C.
    fn collect_transitive_editables(
        workspace: &Workspace,
        extras: &ExtrasSpecification,
        preview: PreviewMode,
        project: Pep621Metadata,
    ) -> Result<RequirementsSpecification> {
        let mut seen = FxHashSet::from_iter([project.name.clone()]);
        let mut queue = VecDeque::from([project.name.clone()]);
        let mut requirements = Vec::new();
        let mut used_extras = FxHashSet::default();

        while let Some(project_name) = queue.pop_front() {
            let Some(current) = workspace.packages().get(&project_name) else {
                continue;
            };
            trace!("Processing metadata for workspace package {project_name}");

            let project_root_absolute = current.root().absolutize_from(workspace.root())?;
            let pyproject = current.pyproject_toml().clone();
            let project = Pep621Metadata::try_from(
                &pyproject,
                extras,
                &project_root_absolute.join("pyproject.toml"),
                project_root_absolute.as_ref(),
                workspace,
                preview,
            )
            .with_context(|| {
                format!(
                    "Invalid requirements in: `{}`",
                    current.root().join("pyproject.toml").user_display()
                )
            })?
            // TODO(konsti): We should support this by building and using the built PEP 517 metadata
            .with_context(|| {
                format!(
                    "Workspace member doesn't declare static metadata: `{}`",
                    current.root().user_display()
                )
            })?;

            // Recurse into any editables.
            for requirement in &project.requirements {
                if matches!(
                    requirement.source,
                    RequirementSource::Path { editable: true, .. }
                ) {
                    if seen.insert(requirement.name.clone()) {
                        queue.push_back(requirement.name.clone());
                    }
                }
            }

            // Collect the requirements and extras.
            used_extras.extend(project.used_extras);
            requirements.extend(project.requirements);
        }

        let spec = Self {
            project: Some(project.name),
            requirements: requirements
                .into_iter()
                .map(UnresolvedRequirementSpecification::from)
                .collect(),
            extras: used_extras,
            ..Self::default()
        };
        Ok(spec)
    }

    /// Read the combined requirements and constraints from a set of sources.
    pub async fn from_sources(
        requirements: &[RequirementsSource],
        constraints: &[RequirementsSource],
        overrides: &[RequirementsSource],
        // Avoid re-discovering the workspace if we already loaded it.
        workspace: Option<&Workspace>,
        extras: &ExtrasSpecification,
        client_builder: &BaseClientBuilder<'_>,
        preview: PreviewMode,
    ) -> Result<Self> {
        let mut spec = Self::default();

        // Read all requirements, and keep track of all requirements _and_ constraints.
        // A `requirements.txt` can contain a `-c constraints.txt` directive within it, so reading
        // a requirements file can also add constraints.
        for source in requirements {
            let source =
                Self::from_source(source, extras, client_builder, workspace, preview).await?;
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
            let source =
                Self::from_source(source, extras, client_builder, workspace, preview).await?;
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
            let source = Self::from_source(source, extras, client_builder, None, preview).await?;
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
            None,
            &ExtrasSpecification::None,
            client_builder,
            preview,
        )
        .await
    }
}

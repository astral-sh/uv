use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use path_absolutize::Absolutize;
use thiserror::Error;
use url::Url;

use pep440_rs::VersionSpecifiers;
use pep508_rs::{VerbatimUrl, VersionOrUrl};
use pypi_types::{Requirement, RequirementSource, VerbatimParsedUrl};
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_git::GitReference;
use uv_normalize::PackageName;
use uv_warnings::warn_user_once;

use crate::pyproject::Source;
use crate::Workspace;

/// An error parsing and merging `tool.uv.sources` with
/// `project.{dependencies,optional-dependencies}`.
#[derive(Debug, Error)]
pub enum LoweringError {
    #[error("Package is not included as workspace package in `tool.uv.workspace`")]
    UndeclaredWorkspacePackage,
    #[error("Can only specify one of: `rev`, `tag`, or `branch`")]
    MoreThanOneGitRef,
    #[error("Unable to combine options in `tool.uv.sources`")]
    InvalidEntry,
    #[error(transparent)]
    InvalidUrl(#[from] url::ParseError),
    #[error(transparent)]
    InvalidVerbatimUrl(#[from] pep508_rs::VerbatimUrlError),
    #[error("Can't combine URLs from both `project.dependencies` and `tool.uv.sources`")]
    ConflictingUrls,
    #[error("Could not normalize path: `{}`", _0.user_display())]
    Absolutize(PathBuf, #[source] io::Error),
    #[error("Fragments are not allowed in URLs: `{0}`")]
    ForbiddenFragment(Url),
    #[error("`workspace = false` is not yet supported")]
    WorkspaceFalse,
    #[error("`tool.uv.sources` is a preview feature; use `--preview` or set `UV_PREVIEW=1` to enable it")]
    MissingPreview,
}

/// Combine `project.dependencies` or `project.optional-dependencies` with `tool.uv.sources`.
pub(crate) fn lower_requirement(
    requirement: pep508_rs::Requirement<VerbatimParsedUrl>,
    project_name: &PackageName,
    project_dir: &Path,
    project_sources: &BTreeMap<PackageName, Source>,
    workspace: &Workspace,
    preview: PreviewMode,
) -> Result<Requirement, LoweringError> {
    let source = project_sources
        .get(&requirement.name)
        .or(workspace.sources().get(&requirement.name))
        .cloned();

    let workspace_package_declared =
        // We require that when you use a package that's part of the workspace, ...
        !workspace.packages().contains_key(&requirement.name)
        // ... it must be declared as a workspace dependency (`workspace = true`), ...
        || matches!(
            source,
            Some(Source::Workspace {
                // By using toml, we technically support `workspace = false`.
                workspace: true,
                ..
            })
        )
        // ... except for recursive self-inclusion (extras that activate other extras), e.g.
        // `framework[machine_learning]` depends on `framework[cuda]`.
        || &requirement.name == project_name;
    if !workspace_package_declared {
        return Err(LoweringError::UndeclaredWorkspacePackage);
    }

    let Some(source) = source else {
        let has_sources = !project_sources.is_empty() || !workspace.sources().is_empty();
        // Support recursive editable inclusions.
        if has_sources && requirement.version_or_url.is_none() && &requirement.name != project_name
        {
            warn_user_once!(
                "Missing version constraint (e.g., a lower bound) for `{}`",
                requirement.name
            );
        }
        return Ok(Requirement::from(requirement));
    };

    if preview.is_disabled() {
        return Err(LoweringError::MissingPreview);
    }

    let source = match source {
        Source::Git {
            git,
            subdirectory,
            rev,
            tag,
            branch,
        } => {
            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                return Err(LoweringError::ConflictingUrls);
            }
            let reference = match (rev, tag, branch) {
                (None, None, None) => GitReference::DefaultBranch,
                (Some(rev), None, None) => {
                    if rev.starts_with("refs/") {
                        GitReference::NamedRef(rev.clone())
                    } else if rev.len() == 40 {
                        GitReference::FullCommit(rev.clone())
                    } else {
                        GitReference::ShortCommit(rev.clone())
                    }
                }
                (None, Some(tag), None) => GitReference::Tag(tag),
                (None, None, Some(branch)) => GitReference::Branch(branch),
                _ => return Err(LoweringError::MoreThanOneGitRef),
            };

            // Create a PEP 508-compatible URL.
            let mut url = Url::parse(&format!("git+{git}"))?;
            if let Some(rev) = reference.as_str() {
                url.set_path(&format!("{}@{}", url.path(), rev));
            }
            if let Some(subdirectory) = &subdirectory {
                url.set_fragment(Some(&format!("subdirectory={subdirectory}")));
            }
            let url = VerbatimUrl::from_url(url);

            let repository = git.clone();

            RequirementSource::Git {
                url,
                repository,
                reference,
                precise: None,
                subdirectory: subdirectory.map(PathBuf::from),
            }
        }
        Source::Url { url, subdirectory } => {
            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                return Err(LoweringError::ConflictingUrls);
            }

            let mut verbatim_url = url.clone();
            if verbatim_url.fragment().is_some() {
                return Err(LoweringError::ForbiddenFragment(url));
            }
            if let Some(subdirectory) = &subdirectory {
                verbatim_url.set_fragment(Some(subdirectory));
            }

            let verbatim_url = VerbatimUrl::from_url(verbatim_url);
            RequirementSource::Url {
                location: url,
                subdirectory: subdirectory.map(PathBuf::from),
                url: verbatim_url,
            }
        }
        Source::Path { path, editable } => {
            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                return Err(LoweringError::ConflictingUrls);
            }
            path_source(path, project_dir, editable.unwrap_or(false))?
        }
        Source::Registry { index } => match requirement.version_or_url {
            None => {
                warn_user_once!(
                    "Missing version constraint (e.g., a lower bound) for `{}`",
                    requirement.name
                );
                RequirementSource::Registry {
                    specifier: VersionSpecifiers::empty(),
                    index: Some(index),
                }
            }
            Some(VersionOrUrl::VersionSpecifier(version)) => RequirementSource::Registry {
                specifier: version,
                index: Some(index),
            },
            Some(VersionOrUrl::Url(_)) => return Err(LoweringError::ConflictingUrls),
        },
        Source::Workspace {
            workspace: is_workspace,
            editable,
        } => {
            if !is_workspace {
                return Err(LoweringError::WorkspaceFalse);
            }
            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                return Err(LoweringError::ConflictingUrls);
            }
            let path = workspace
                .packages()
                .get(&requirement.name)
                .ok_or(LoweringError::UndeclaredWorkspacePackage)?
                .clone();
            path_source(path.root(), workspace.root(), editable.unwrap_or(true))?
        }
        Source::CatchAll { .. } => {
            // Emit a dedicated error message, which is an improvement over Serde's default error.
            return Err(LoweringError::InvalidEntry);
        }
    };
    Ok(Requirement {
        name: requirement.name,
        extras: requirement.extras,
        marker: requirement.marker,
        source,
        origin: requirement.origin,
    })
}

/// Convert a path string to a path section.
fn path_source(
    path: impl AsRef<Path>,
    project_dir: &Path,
    editable: bool,
) -> Result<RequirementSource, LoweringError> {
    let url = VerbatimUrl::parse_path(path.as_ref(), project_dir)?
        .with_given(path.as_ref().to_string_lossy());
    let path_buf = path.as_ref().to_path_buf();
    let path_buf = path_buf
        .absolutize_from(project_dir)
        .map_err(|err| LoweringError::Absolutize(path.as_ref().to_path_buf(), err))?
        .to_path_buf();
    Ok(RequirementSource::Path {
        path: path_buf,
        url,
        editable,
    })
}

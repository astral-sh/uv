use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use distribution_filename::DistExtension;
use path_absolutize::Absolutize;
use pep440_rs::VersionSpecifiers;
use pep508_rs::{VerbatimUrl, VersionOrUrl};
use pypi_types::{ParsedUrlError, Requirement, RequirementSource, VerbatimParsedUrl};
use thiserror::Error;
use url::Url;
use uv_fs::{relative_to, Simplified};
use uv_git::GitReference;
use uv_normalize::PackageName;
use uv_warnings::warn_user_once;
use uv_workspace::pyproject::Source;
use uv_workspace::Workspace;

#[derive(Debug, Clone)]
pub struct LoweredRequirement(Requirement);

#[derive(Debug, Clone, Copy)]
enum Origin {
    /// The `tool.uv.sources` were read from the project.
    Project,
    /// The `tool.uv.sources` were read from the workspace root.
    Workspace,
}

impl LoweredRequirement {
    /// Combine `project.dependencies` or `project.optional-dependencies` with `tool.uv.sources`.
    pub(crate) fn from_requirement(
        requirement: pep508_rs::Requirement<VerbatimParsedUrl>,
        project_name: &PackageName,
        project_dir: &Path,
        project_sources: &BTreeMap<PackageName, Source>,
        workspace: &Workspace,
    ) -> Result<Self, LoweringError> {
        let (source, origin) = if let Some(source) = project_sources.get(&requirement.name) {
            (Some(source), Origin::Project)
        } else if let Some(source) = workspace.sources().get(&requirement.name) {
            (Some(source), Origin::Workspace)
        } else {
            (None, Origin::Project)
        };
        let source = source.cloned();

        let workspace_package_declared =
            // We require that when you use a package that's part of the workspace, ...
            !workspace.packages().contains_key(&requirement.name)
                // ... it must be declared as a workspace dependency (`workspace = true`), ...
                || matches!(
                    source,
                    Some(Source::Workspace {
                        // By using toml, we technically support `workspace = false`.
                        workspace: true
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
            if has_sources
                && requirement.version_or_url.is_none()
                && &requirement.name != project_name
            {
                warn_user_once!(
                    "Missing version constraint (e.g., a lower bound) for `{}`",
                    requirement.name
                );
            }
            return Ok(Self(Requirement::from(requirement)));
        };

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
                git_source(&git, subdirectory, rev, tag, branch)?
            }
            Source::Url { url, subdirectory } => {
                if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                    return Err(LoweringError::ConflictingUrls);
                }
                url_source(url, subdirectory)?
            }
            Source::Path { path, editable } => {
                if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                    return Err(LoweringError::ConflictingUrls);
                }
                path_source(
                    path,
                    origin,
                    project_dir,
                    workspace.install_path(),
                    editable.unwrap_or(false),
                )?
            }
            Source::Registry { index } => registry_source(&requirement, index)?,
            Source::Workspace {
                workspace: is_workspace,
            } => {
                if !is_workspace {
                    return Err(LoweringError::WorkspaceFalse);
                }
                if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                    return Err(LoweringError::ConflictingUrls);
                }
                let member = workspace
                    .packages()
                    .get(&requirement.name)
                    .ok_or(LoweringError::UndeclaredWorkspacePackage)?
                    .clone();

                // Say we have:
                // ```
                // root
                // ├── main_workspace  <- We want to the path from here ...
                // │   ├── pyproject.toml
                // │   └── uv.lock
                // └──current_workspace
                //    └── packages
                //        └── current_package  <- ... to here.
                //            └── pyproject.toml
                // ```
                // The path we need in the lockfile: `../current_workspace/packages/current_project`
                // member root: `/root/current_workspace/packages/current_project`
                // workspace install root: `/root/current_workspace`
                // relative to workspace: `packages/current_project`
                // workspace lock root: `../current_workspace`
                // relative to main workspace: `../current_workspace/packages/current_project`
                let relative_to_workspace = relative_to(member.root(), workspace.install_path())
                    .map_err(LoweringError::RelativeTo)?;
                let relative_to_main_workspace = workspace.lock_path().join(relative_to_workspace);
                let url = VerbatimUrl::parse_absolute_path(member.root())?
                    .with_given(relative_to_main_workspace.to_string_lossy());
                RequirementSource::Directory {
                    install_path: member.root().clone(),
                    lock_path: relative_to_main_workspace,
                    url,
                    editable: true,
                }
            }
            Source::CatchAll { .. } => {
                // Emit a dedicated error message, which is an improvement over Serde's default error.
                return Err(LoweringError::InvalidEntry);
            }
        };
        Ok(Self(Requirement {
            name: requirement.name,
            extras: requirement.extras,
            marker: requirement.marker,
            source,
            origin: requirement.origin,
        }))
    }

    /// Lower a [`pep508_rs::Requirement`] in a non-workspace setting (for example, in a PEP 723
    /// script, which runs in an isolated context).
    pub fn from_non_workspace_requirement(
        requirement: pep508_rs::Requirement<VerbatimParsedUrl>,
        dir: &Path,
        sources: &BTreeMap<PackageName, Source>,
    ) -> Result<Self, LoweringError> {
        let source = sources.get(&requirement.name).cloned();

        let Some(source) = source else {
            return Ok(Self(Requirement::from(requirement)));
        };

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
                git_source(&git, subdirectory, rev, tag, branch)?
            }
            Source::Url { url, subdirectory } => {
                if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                    return Err(LoweringError::ConflictingUrls);
                }
                url_source(url, subdirectory)?
            }
            Source::Path { path, editable } => {
                if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                    return Err(LoweringError::ConflictingUrls);
                }
                path_source(path, Origin::Project, dir, dir, editable.unwrap_or(false))?
            }
            Source::Registry { index } => registry_source(&requirement, index)?,
            Source::Workspace { .. } => {
                return Err(LoweringError::WorkspaceMember);
            }
            Source::CatchAll { .. } => {
                // Emit a dedicated error message, which is an improvement over Serde's default
                // error.
                return Err(LoweringError::InvalidEntry);
            }
        };
        Ok(Self(Requirement {
            name: requirement.name,
            extras: requirement.extras,
            marker: requirement.marker,
            source,
            origin: requirement.origin,
        }))
    }

    /// Convert back into a [`Requirement`].
    pub fn into_inner(self) -> Requirement {
        self.0
    }
}

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
    #[error("Workspace members are not allowed in non-workspace contexts")]
    WorkspaceMember,
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
    #[error("Editable must refer to a local directory, not a file: `{0}`")]
    EditableFile(String),
    #[error(transparent)]
    ParsedUrl(#[from] ParsedUrlError),
    #[error(transparent)] // Function attaches the context
    RelativeTo(io::Error),
}

/// Convert a Git source into a [`RequirementSource`].
fn git_source(
    git: &Url,
    subdirectory: Option<String>,
    rev: Option<String>,
    tag: Option<String>,
    branch: Option<String>,
) -> Result<RequirementSource, LoweringError> {
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

    Ok(RequirementSource::Git {
        url,
        repository,
        reference,
        precise: None,
        subdirectory: subdirectory.map(PathBuf::from),
    })
}

/// Convert a URL source into a [`RequirementSource`].
fn url_source(url: Url, subdirectory: Option<String>) -> Result<RequirementSource, LoweringError> {
    let mut verbatim_url = url.clone();
    if verbatim_url.fragment().is_some() {
        return Err(LoweringError::ForbiddenFragment(url));
    }
    if let Some(subdirectory) = &subdirectory {
        verbatim_url.set_fragment(Some(subdirectory));
    }

    let ext = DistExtension::from_path(url.path())
        .map_err(|err| ParsedUrlError::MissingExtensionUrl(url.to_string(), err))?;

    let verbatim_url = VerbatimUrl::from_url(verbatim_url);
    Ok(RequirementSource::Url {
        location: url,
        subdirectory: subdirectory.map(PathBuf::from),
        ext,
        url: verbatim_url,
    })
}

/// Convert a registry source into a [`RequirementSource`].
fn registry_source(
    requirement: &pep508_rs::Requirement<VerbatimParsedUrl>,
    index: String,
) -> Result<RequirementSource, LoweringError> {
    match &requirement.version_or_url {
        None => {
            warn_user_once!(
                "Missing version constraint (e.g., a lower bound) for `{}`",
                requirement.name
            );
            Ok(RequirementSource::Registry {
                specifier: VersionSpecifiers::empty(),
                index: Some(index),
            })
        }
        Some(VersionOrUrl::VersionSpecifier(version)) => Ok(RequirementSource::Registry {
            specifier: version.clone(),
            index: Some(index),
        }),
        Some(VersionOrUrl::Url(_)) => Err(LoweringError::ConflictingUrls),
    }
}

/// Convert a path string to a file or directory source.
fn path_source(
    path: impl AsRef<Path>,
    origin: Origin,
    project_dir: &Path,
    workspace_root: &Path,
    editable: bool,
) -> Result<RequirementSource, LoweringError> {
    let path = path.as_ref();
    let base = match origin {
        Origin::Project => project_dir,
        Origin::Workspace => workspace_root,
    };
    let url = VerbatimUrl::parse_path(path, base)?.with_given(path.to_string_lossy());
    let absolute_path = path
        .to_path_buf()
        .absolutize_from(base)
        .map_err(|err| LoweringError::Absolutize(path.to_path_buf(), err))?
        .to_path_buf();
    let relative_to_workspace = if path.is_relative() {
        // Relative paths in a project are relative to the project root, but the lockfile is
        // relative to the workspace root.
        relative_to(&absolute_path, workspace_root).map_err(LoweringError::RelativeTo)?
    } else {
        // If the user gave us an absolute path, we respect that.
        path.to_path_buf()
    };
    let is_dir = if let Ok(metadata) = absolute_path.metadata() {
        metadata.is_dir()
    } else {
        absolute_path.extension().is_none()
    };
    if is_dir {
        Ok(RequirementSource::Directory {
            install_path: absolute_path,
            lock_path: relative_to_workspace,
            url,
            editable,
        })
    } else {
        if editable {
            return Err(LoweringError::EditableFile(url.to_string()));
        }
        Ok(RequirementSource::Path {
            install_path: absolute_path,
            lock_path: relative_to_workspace,
            ext: DistExtension::from_path(path)
                .map_err(|err| ParsedUrlError::MissingExtensionPath(path.to_path_buf(), err))?,
            url,
        })
    }
}

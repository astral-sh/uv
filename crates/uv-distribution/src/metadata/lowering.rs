use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use either::Either;
use thiserror::Error;
use url::Url;

use uv_configuration::LowerBound;
use uv_distribution_filename::DistExtension;
use uv_distribution_types::{Index, IndexLocations, IndexName, Origin};
use uv_git::GitReference;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::VersionSpecifiers;
use uv_pep508::{MarkerTree, VerbatimUrl, VersionOrUrl};
use uv_pypi_types::{
    ConflictItem, ParsedUrlError, Requirement, RequirementSource, VerbatimParsedUrl,
};
use uv_warnings::warn_user_once;
use uv_workspace::pyproject::{PyProjectToml, Source, Sources};
use uv_workspace::Workspace;

use crate::metadata::GitWorkspaceMember;

#[derive(Debug, Clone)]
pub struct LoweredRequirement(Requirement);

#[derive(Debug, Clone, Copy)]
enum RequirementOrigin {
    /// The `tool.uv.sources` were read from the project.
    Project,
    /// The `tool.uv.sources` were read from the workspace root.
    Workspace,
}

impl LoweredRequirement {
    /// Combine `project.dependencies` or `project.optional-dependencies` with `tool.uv.sources`.
    pub(crate) fn from_requirement<'data>(
        requirement: uv_pep508::Requirement<VerbatimParsedUrl>,
        project_name: &'data PackageName,
        project_dir: &'data Path,
        project_sources: &'data BTreeMap<PackageName, Sources>,
        project_indexes: &'data [Index],
        extra: Option<&ExtraName>,
        group: Option<&GroupName>,
        locations: &'data IndexLocations,
        workspace: &'data Workspace,
        lower_bound: LowerBound,
        git_member: Option<&'data GitWorkspaceMember<'data>>,
    ) -> impl Iterator<Item = Result<Self, LoweringError>> + 'data + use<'data> {
        // Identify the source from the `tool.uv.sources` table.
        let (source, origin) = if let Some(source) = project_sources.get(&requirement.name) {
            (Some(source), RequirementOrigin::Project)
        } else if let Some(source) = workspace.sources().get(&requirement.name) {
            (Some(source), RequirementOrigin::Workspace)
        } else {
            (None, RequirementOrigin::Project)
        };

        // If the source only applies to a given extra or dependency group, filter it out.
        let source = source.map(|source| {
            source
                .iter()
                .filter(|source| {
                    if let Some(target) = source.extra() {
                        if extra != Some(target) {
                            return false;
                        }
                    }

                    if let Some(target) = source.group() {
                        if group != Some(target) {
                            return false;
                        }
                    }

                    true
                })
                .cloned()
                .collect::<Sources>()
        });

        let workspace_package_declared =
            // We require that when you use a package that's part of the workspace, ...
            !workspace.packages().contains_key(&requirement.name)
                // ... it must be declared as a workspace dependency (`workspace = true`), ...
                || source.as_ref().filter(|sources| !sources.is_empty()).is_some_and(|source| source.iter().all(|source| {
                    matches!(source, Source::Workspace { workspace: true, .. })
                }))
                // ... except for recursive self-inclusion (extras that activate other extras), e.g.
                // `framework[machine_learning]` depends on `framework[cuda]`.
                || &requirement.name == project_name;
        if !workspace_package_declared {
            return Either::Left(std::iter::once(Err(
                LoweringError::UndeclaredWorkspacePackage,
            )));
        }

        let Some(source) = source else {
            let has_sources = !project_sources.is_empty() || !workspace.sources().is_empty();
            if matches!(lower_bound, LowerBound::Warn) {
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
            }
            return Either::Left(std::iter::once(Ok(Self(Requirement::from(requirement)))));
        };

        // Determine whether the markers cover the full space for the requirement. If not, fill the
        // remaining space with the negation of the sources.
        let remaining = {
            // Determine the space covered by the sources.
            let mut total = MarkerTree::FALSE;
            for source in source.iter() {
                total.or(source.marker().clone());
            }

            // Determine the space covered by the requirement.
            let mut remaining = total.negate();
            remaining.and(requirement.marker.clone());

            LoweredRequirement(Requirement {
                marker: remaining,
                ..Requirement::from(requirement.clone())
            })
        };

        Either::Right(
            source
                .into_iter()
                .map(move |source| {
                    let (source, mut marker) = match source {
                        Source::Git {
                            git,
                            subdirectory,
                            rev,
                            tag,
                            branch,
                            marker,
                            ..
                        } => {
                            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                                return Err(LoweringError::ConflictingUrls);
                            }
                            let source = git_source(
                                &git,
                                subdirectory.map(PathBuf::from),
                                rev,
                                tag,
                                branch,
                            )?;
                            (source, marker)
                        }
                        Source::Url {
                            url,
                            subdirectory,
                            marker,
                            ..
                        } => {
                            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                                return Err(LoweringError::ConflictingUrls);
                            }
                            let source = url_source(url, subdirectory.map(PathBuf::from))?;
                            (source, marker)
                        }
                        Source::Path {
                            path,
                            editable,
                            marker,
                            ..
                        } => {
                            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                                return Err(LoweringError::ConflictingUrls);
                            }
                            let source = path_source(
                                PathBuf::from(path),
                                origin,
                                project_dir,
                                workspace.install_path(),
                                editable.unwrap_or(false),
                            )?;
                            (source, marker)
                        }
                        Source::Registry {
                            index,
                            marker,
                            extra,
                            group,
                        } => {
                            // Identify the named index from either the project indexes or the workspace indexes,
                            // in that order.
                            let Some(index) = locations
                                .indexes()
                                .filter(|index| matches!(index.origin, Some(Origin::Cli)))
                                .chain(project_indexes.iter())
                                .chain(workspace.indexes().iter())
                                .find(|Index { name, .. }| {
                                    name.as_ref().is_some_and(|name| *name == index)
                                })
                                .map(|Index { url: index, .. }| index.clone())
                            else {
                                return Err(LoweringError::MissingIndex(
                                    requirement.name.clone(),
                                    index,
                                ));
                            };
                            let conflict = if let Some(extra) = extra {
                                Some(ConflictItem::from((project_name.clone(), extra)))
                            } else {
                                group.map(|group| ConflictItem::from((project_name.clone(), group)))
                            };
                            let source = registry_source(
                                &requirement,
                                index.into_url(),
                                conflict,
                                lower_bound,
                            )?;
                            (source, marker)
                        }
                        Source::Workspace {
                            workspace: is_workspace,
                            marker,
                            ..
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
                            let url = VerbatimUrl::from_absolute_path(member.root())?;
                            let install_path = url.to_file_path().map_err(|()| {
                                LoweringError::RelativeTo(io::Error::new(
                                    io::ErrorKind::Other,
                                    "Invalid path in file URL",
                                ))
                            })?;

                            let source = if let Some(git_member) = &git_member {
                                // If the workspace comes from a git dependency, all workspace
                                // members need to be git deps, too.
                                let subdirectory =
                                    uv_fs::relative_to(member.root(), git_member.fetch_root)
                                        .expect("Workspace member must be relative");
                                RequirementSource::Git {
                                    repository: git_member.git_source.git.repository().clone(),
                                    reference: git_member.git_source.git.reference().clone(),
                                    precise: git_member.git_source.git.precise(),
                                    subdirectory: if subdirectory == PathBuf::new() {
                                        None
                                    } else {
                                        Some(subdirectory)
                                    },
                                    url,
                                }
                            } else if member.pyproject_toml().is_package() {
                                RequirementSource::Directory {
                                    install_path,
                                    url,
                                    editable: true,
                                    r#virtual: false,
                                }
                            } else {
                                RequirementSource::Directory {
                                    install_path,
                                    url,
                                    editable: false,
                                    r#virtual: true,
                                }
                            };
                            (source, marker)
                        }
                    };

                    marker.and(requirement.marker.clone());

                    Ok(Self(Requirement {
                        name: requirement.name.clone(),
                        extras: requirement.extras.clone(),
                        marker,
                        source,
                        origin: requirement.origin.clone(),
                    }))
                })
                .chain(std::iter::once(Ok(remaining)))
                .filter(|requirement| match requirement {
                    Ok(requirement) => !requirement.0.marker.is_false(),
                    Err(_) => true,
                }),
        )
    }

    /// Lower a [`uv_pep508::Requirement`] in a non-workspace setting (for example, in a PEP 723
    /// script, which runs in an isolated context).
    pub fn from_non_workspace_requirement<'data>(
        requirement: uv_pep508::Requirement<VerbatimParsedUrl>,
        dir: &'data Path,
        sources: &'data BTreeMap<PackageName, Sources>,
        indexes: &'data [Index],
        locations: &'data IndexLocations,
        lower_bound: LowerBound,
    ) -> impl Iterator<Item = Result<Self, LoweringError>> + 'data {
        let source = sources.get(&requirement.name).cloned();

        let Some(source) = source else {
            return Either::Left(std::iter::once(Ok(Self(Requirement::from(requirement)))));
        };

        // If the source only applies to a given extra, filter it out.
        let source = source
            .iter()
            .filter(|source| {
                source.extra().map_or(true, |target| {
                    requirement
                        .marker
                        .top_level_extra_name()
                        .is_some_and(|extra| extra == *target)
                })
            })
            .cloned()
            .collect::<Sources>();

        // Determine whether the markers cover the full space for the requirement. If not, fill the
        // remaining space with the negation of the sources.
        let remaining = {
            // Determine the space covered by the sources.
            let mut total = MarkerTree::FALSE;
            for source in source.iter() {
                total.or(source.marker().clone());
            }

            // Determine the space covered by the requirement.
            let mut remaining = total.negate();
            remaining.and(requirement.marker.clone());

            LoweredRequirement(Requirement {
                marker: remaining,
                ..Requirement::from(requirement.clone())
            })
        };

        Either::Right(
            source
                .into_iter()
                .map(move |source| {
                    let (source, mut marker) = match source {
                        Source::Git {
                            git,
                            subdirectory,
                            rev,
                            tag,
                            branch,
                            marker,
                            ..
                        } => {
                            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                                return Err(LoweringError::ConflictingUrls);
                            }
                            let source = git_source(
                                &git,
                                subdirectory.map(PathBuf::from),
                                rev,
                                tag,
                                branch,
                            )?;
                            (source, marker)
                        }
                        Source::Url {
                            url,
                            subdirectory,
                            marker,
                            ..
                        } => {
                            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                                return Err(LoweringError::ConflictingUrls);
                            }
                            let source = url_source(url, subdirectory.map(PathBuf::from))?;
                            (source, marker)
                        }
                        Source::Path {
                            path,
                            editable,
                            marker,
                            ..
                        } => {
                            if matches!(requirement.version_or_url, Some(VersionOrUrl::Url(_))) {
                                return Err(LoweringError::ConflictingUrls);
                            }
                            let source = path_source(
                                PathBuf::from(path),
                                RequirementOrigin::Project,
                                dir,
                                dir,
                                editable.unwrap_or(false),
                            )?;
                            (source, marker)
                        }
                        Source::Registry { index, marker, .. } => {
                            let Some(index) = locations
                                .indexes()
                                .filter(|index| matches!(index.origin, Some(Origin::Cli)))
                                .chain(indexes.iter())
                                .find(|Index { name, .. }| {
                                    name.as_ref().is_some_and(|name| *name == index)
                                })
                                .map(|Index { url: index, .. }| index.clone())
                            else {
                                return Err(LoweringError::MissingIndex(
                                    requirement.name.clone(),
                                    index,
                                ));
                            };
                            let conflict = None;
                            let source = registry_source(
                                &requirement,
                                index.into_url(),
                                conflict,
                                lower_bound,
                            )?;
                            (source, marker)
                        }
                        Source::Workspace { .. } => {
                            return Err(LoweringError::WorkspaceMember);
                        }
                    };

                    marker.and(requirement.marker.clone());

                    Ok(Self(Requirement {
                        name: requirement.name.clone(),
                        extras: requirement.extras.clone(),
                        marker,
                        source,
                        origin: requirement.origin.clone(),
                    }))
                })
                .chain(std::iter::once(Ok(remaining)))
                .filter(|requirement| match requirement {
                    Ok(requirement) => !requirement.0.marker.is_false(),
                    Err(_) => true,
                }),
        )
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
    #[error("Package `{0}` references an undeclared index: `{1}`")]
    MissingIndex(PackageName, IndexName),
    #[error("Workspace members are not allowed in non-workspace contexts")]
    WorkspaceMember,
    #[error(transparent)]
    InvalidUrl(#[from] url::ParseError),
    #[error(transparent)]
    InvalidVerbatimUrl(#[from] uv_pep508::VerbatimUrlError),
    #[error("Can't combine URLs from both `project.dependencies` and `tool.uv.sources`")]
    ConflictingUrls,
    #[error("Fragments are not allowed in URLs: `{0}`")]
    ForbiddenFragment(Url),
    #[error("`workspace = false` is not yet supported")]
    WorkspaceFalse,
    #[error("Editable must refer to a local directory, not a file: `{0}`")]
    EditableFile(String),
    #[error(transparent)]
    ParsedUrl(#[from] ParsedUrlError),
    #[error("Path must be UTF-8: `{0}`")]
    NonUtf8Path(PathBuf),
    #[error(transparent)] // Function attaches the context
    RelativeTo(io::Error),
}

/// Convert a Git source into a [`RequirementSource`].
fn git_source(
    git: &Url,
    subdirectory: Option<PathBuf>,
    rev: Option<String>,
    tag: Option<String>,
    branch: Option<String>,
) -> Result<RequirementSource, LoweringError> {
    let reference = match (rev, tag, branch) {
        (None, None, None) => GitReference::DefaultBranch,
        (Some(rev), None, None) => GitReference::from_rev(rev),
        (None, Some(tag), None) => GitReference::Tag(tag),
        (None, None, Some(branch)) => GitReference::Branch(branch),
        _ => return Err(LoweringError::MoreThanOneGitRef),
    };

    // Create a PEP 508-compatible URL.
    let mut url = Url::parse(&format!("git+{git}"))?;
    if let Some(rev) = reference.as_str() {
        url.set_path(&format!("{}@{}", url.path(), rev));
    }
    if let Some(subdirectory) = subdirectory.as_ref() {
        let subdirectory = subdirectory
            .to_str()
            .ok_or_else(|| LoweringError::NonUtf8Path(subdirectory.clone()))?;
        url.set_fragment(Some(&format!("subdirectory={subdirectory}")));
    }
    let url = VerbatimUrl::from_url(url);

    let repository = git.clone();

    Ok(RequirementSource::Git {
        url,
        repository,
        reference,
        precise: None,
        subdirectory,
    })
}

/// Convert a URL source into a [`RequirementSource`].
fn url_source(url: Url, subdirectory: Option<PathBuf>) -> Result<RequirementSource, LoweringError> {
    let mut verbatim_url = url.clone();
    if verbatim_url.fragment().is_some() {
        return Err(LoweringError::ForbiddenFragment(url));
    }
    if let Some(subdirectory) = subdirectory.as_ref() {
        let subdirectory = subdirectory
            .to_str()
            .ok_or_else(|| LoweringError::NonUtf8Path(subdirectory.clone()))?;
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
    requirement: &uv_pep508::Requirement<VerbatimParsedUrl>,
    index: Url,
    conflict: Option<ConflictItem>,
    bounds: LowerBound,
) -> Result<RequirementSource, LoweringError> {
    match &requirement.version_or_url {
        None => {
            if matches!(bounds, LowerBound::Warn) {
                warn_user_once!(
                    "Missing version constraint (e.g., a lower bound) for `{}`",
                    requirement.name
                );
            }
            Ok(RequirementSource::Registry {
                specifier: VersionSpecifiers::empty(),
                index: Some(index),
                conflict,
            })
        }
        Some(VersionOrUrl::VersionSpecifier(version)) => Ok(RequirementSource::Registry {
            specifier: version.clone(),
            index: Some(index),
            conflict,
        }),
        Some(VersionOrUrl::Url(_)) => Err(LoweringError::ConflictingUrls),
    }
}

/// Convert a path string to a file or directory source.
fn path_source(
    path: impl AsRef<Path>,
    origin: RequirementOrigin,
    project_dir: &Path,
    workspace_root: &Path,
    editable: bool,
) -> Result<RequirementSource, LoweringError> {
    let path = path.as_ref();
    let base = match origin {
        RequirementOrigin::Project => project_dir,
        RequirementOrigin::Workspace => workspace_root,
    };
    let url = VerbatimUrl::from_path(path, base)?.with_given(path.to_string_lossy());
    let install_path = url.to_file_path().map_err(|()| {
        LoweringError::RelativeTo(io::Error::new(
            io::ErrorKind::Other,
            "Invalid path in file URL",
        ))
    })?;

    let is_dir = if let Ok(metadata) = install_path.metadata() {
        metadata.is_dir()
    } else {
        install_path.extension().is_none()
    };
    if is_dir {
        if editable {
            Ok(RequirementSource::Directory {
                install_path,
                url,
                editable,
                r#virtual: false,
            })
        } else {
            // Determine whether the project is a package or virtual.
            let is_package = {
                let pyproject_path = install_path.join("pyproject.toml");
                fs_err::read_to_string(&pyproject_path)
                    .ok()
                    .and_then(|contents| PyProjectToml::from_string(contents).ok())
                    .map(|pyproject_toml| pyproject_toml.is_package())
                    .unwrap_or(true)
            };

            // If a project is not a package, treat it as a virtual dependency.
            let r#virtual = !is_package;

            Ok(RequirementSource::Directory {
                install_path,
                url,
                editable,
                r#virtual,
            })
        }
    } else {
        if editable {
            return Err(LoweringError::EditableFile(url.to_string()));
        }
        Ok(RequirementSource::Path {
            ext: DistExtension::from_path(&install_path)
                .map_err(|err| ParsedUrlError::MissingExtensionPath(path.to_path_buf(), err))?,
            install_path,
            url,
        })
    }
}

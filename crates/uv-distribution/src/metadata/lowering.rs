use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use either::Either;
use thiserror::Error;
use url::Url;

use uv_distribution_filename::DistExtension;
use uv_distribution_types::{Index, IndexLocations, IndexName, Origin};
use uv_git_types::{GitReference, GitUrl, GitUrlParseError};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::VersionSpecifiers;
use uv_pep508::{looks_like_git_repository, MarkerTree, VerbatimUrl, VersionOrUrl};
use uv_pypi_types::{
    ConflictItem, ParsedUrlError, Requirement, RequirementSource, VerbatimParsedUrl,
};
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
        project_name: Option<&'data PackageName>,
        project_dir: &'data Path,
        project_sources: &'data BTreeMap<PackageName, Sources>,
        project_indexes: &'data [Index],
        extra: Option<&ExtraName>,
        group: Option<&GroupName>,
        locations: &'data IndexLocations,
        workspace: &'data Workspace,
        git_member: Option<&'data GitWorkspaceMember<'data>>,
    ) -> impl Iterator<Item = Result<Self, LoweringError>> + 'data {
        // Identify the source from the `tool.uv.sources` table.
        let (sources, origin) = if let Some(source) = project_sources.get(&requirement.name) {
            (Some(source), RequirementOrigin::Project)
        } else if let Some(source) = workspace.sources().get(&requirement.name) {
            (Some(source), RequirementOrigin::Workspace)
        } else {
            (None, RequirementOrigin::Project)
        };

        // If the source only applies to a given extra or dependency group, filter it out.
        let sources = sources.map(|sources| {
            sources
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

        // If you use a package that's part of the workspace...
        if workspace.packages().contains_key(&requirement.name) {
            // And it's not a recursive self-inclusion (extras that activate other extras), e.g.
            // `framework[machine_learning]` depends on `framework[cuda]`.
            if project_name.is_none_or(|project_name| *project_name != requirement.name) {
                // It must be declared as a workspace source.
                let Some(sources) = sources.as_ref() else {
                    // No sources were declared for the workspace package.
                    return Either::Left(std::iter::once(Err(
                        LoweringError::MissingWorkspaceSource(requirement.name.clone()),
                    )));
                };

                for source in sources.iter() {
                    match source {
                        Source::Git { .. } => {
                            return Either::Left(std::iter::once(Err(
                                LoweringError::NonWorkspaceSource(
                                    requirement.name.clone(),
                                    SourceKind::Git,
                                ),
                            )));
                        }
                        Source::Url { .. } => {
                            return Either::Left(std::iter::once(Err(
                                LoweringError::NonWorkspaceSource(
                                    requirement.name.clone(),
                                    SourceKind::Url,
                                ),
                            )));
                        }
                        Source::Path { .. } => {
                            return Either::Left(std::iter::once(Err(
                                LoweringError::NonWorkspaceSource(
                                    requirement.name.clone(),
                                    SourceKind::Path,
                                ),
                            )));
                        }
                        Source::Registry { .. } => {
                            return Either::Left(std::iter::once(Err(
                                LoweringError::NonWorkspaceSource(
                                    requirement.name.clone(),
                                    SourceKind::Registry,
                                ),
                            )));
                        }
                        Source::Workspace { .. } => {
                            // OK
                        }
                    }
                }
            }
        }

        let Some(sources) = sources else {
            return Either::Left(std::iter::once(Ok(Self(Requirement::from(requirement)))));
        };

        // Determine whether the markers cover the full space for the requirement. If not, fill the
        // remaining space with the negation of the sources.
        let remaining = {
            // Determine the space covered by the sources.
            let mut total = MarkerTree::FALSE;
            for source in sources.iter() {
                total.or(source.marker());
            }

            // Determine the space covered by the requirement.
            let mut remaining = total.negate();
            remaining.and(requirement.marker);

            LoweredRequirement(Requirement {
                marker: remaining,
                ..Requirement::from(requirement.clone())
            })
        };

        Either::Right(
            sources
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
                            let source =
                                url_source(&requirement, url, subdirectory.map(PathBuf::from))?;
                            (source, marker)
                        }
                        Source::Path {
                            path,
                            editable,
                            package,
                            marker,
                            ..
                        } => {
                            let source = path_source(
                                PathBuf::from(path),
                                git_member,
                                origin,
                                project_dir,
                                workspace.install_path(),
                                editable,
                                package,
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
                            let conflict = project_name.and_then(|project_name| {
                                if let Some(extra) = extra {
                                    Some(ConflictItem::from((project_name.clone(), extra)))
                                } else {
                                    group.map(|group| {
                                        ConflictItem::from((project_name.clone(), group))
                                    })
                                }
                            });
                            let source = registry_source(&requirement, index.into_url(), conflict);
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
                            let member = workspace
                                .packages()
                                .get(&requirement.name)
                                .ok_or_else(|| {
                                    LoweringError::UndeclaredWorkspacePackage(
                                        requirement.name.clone(),
                                    )
                                })?
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
                                // If the workspace comes from a Git dependency, all workspace
                                // members need to be Git dependencies, too.
                                let subdirectory =
                                    uv_fs::relative_to(member.root(), git_member.fetch_root)
                                        .expect("Workspace member must be relative");
                                let subdirectory = uv_fs::normalize_path_buf(subdirectory);
                                RequirementSource::Git {
                                    git: git_member.git_source.git.clone(),
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

                    marker.and(requirement.marker);

                    Ok(Self(Requirement {
                        name: requirement.name.clone(),
                        extras: requirement.extras.clone(),
                        groups: vec![],
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
    ) -> impl Iterator<Item = Result<Self, LoweringError>> + 'data {
        let source = sources.get(&requirement.name).cloned();

        let Some(source) = source else {
            return Either::Left(std::iter::once(Ok(Self(Requirement::from(requirement)))));
        };

        // If the source only applies to a given extra, filter it out.
        let source = source
            .iter()
            .filter(|source| {
                source.extra().is_none_or(|target| {
                    requirement
                        .marker
                        .top_level_extra_name()
                        .is_some_and(|extra| &*extra == target)
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
                total.or(source.marker());
            }

            // Determine the space covered by the requirement.
            let mut remaining = total.negate();
            remaining.and(requirement.marker);

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
                            let source =
                                url_source(&requirement, url, subdirectory.map(PathBuf::from))?;
                            (source, marker)
                        }
                        Source::Path {
                            path,
                            editable,
                            package,
                            marker,
                            ..
                        } => {
                            let source = path_source(
                                PathBuf::from(path),
                                None,
                                RequirementOrigin::Project,
                                dir,
                                dir,
                                editable,
                                package,
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
                            let source = registry_source(&requirement, index.into_url(), conflict);
                            (source, marker)
                        }
                        Source::Workspace { .. } => {
                            return Err(LoweringError::WorkspaceMember);
                        }
                    };

                    marker.and(requirement.marker);

                    Ok(Self(Requirement {
                        name: requirement.name.clone(),
                        extras: requirement.extras.clone(),
                        groups: vec![],
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
    #[error("`{0}` is included as a workspace member, but is missing an entry in `tool.uv.sources` (e.g., `{0} = {{ workspace = true }}`)")]
    MissingWorkspaceSource(PackageName),
    #[error("`{0}` is included as a workspace member, but references a {1} in `tool.uv.sources`. Workspace members must be declared as workspace sources (e.g., `{0} = {{ workspace = true }}`).")]
    NonWorkspaceSource(PackageName, SourceKind),
    #[error("`{0}` references a workspace in `tool.uv.sources` (e.g., `{0} = {{ workspace = true }}`), but is not a workspace member")]
    UndeclaredWorkspacePackage(PackageName),
    #[error("Can only specify one of: `rev`, `tag`, or `branch`")]
    MoreThanOneGitRef,
    #[error(transparent)]
    GitUrlParse(#[from] GitUrlParseError),
    #[error("Package `{0}` references an undeclared index: `{1}`")]
    MissingIndex(PackageName, IndexName),
    #[error("Workspace members are not allowed in non-workspace contexts")]
    WorkspaceMember,
    #[error(transparent)]
    InvalidUrl(#[from] url::ParseError),
    #[error(transparent)]
    InvalidVerbatimUrl(#[from] uv_pep508::VerbatimUrlError),
    #[error("Fragments are not allowed in URLs: `{0}`")]
    ForbiddenFragment(Url),
    #[error("`{0}` is associated with a URL source, but references a Git repository. Consider using a Git source instead (e.g., `{0} = {{ git = \"{1}\" }}`)")]
    MissingGitSource(PackageName, Url),
    #[error("`workspace = false` is not yet supported")]
    WorkspaceFalse,
    #[error("Source with `editable = true` must refer to a local directory, not a file: `{0}`")]
    EditableFile(String),
    #[error("Source with `package = true` must refer to a local directory, not a file: `{0}`")]
    PackagedFile(String),
    #[error("Git repository references local file source, but only directories are supported as transitive Git dependencies: `{0}`")]
    GitFile(String),
    #[error(transparent)]
    ParsedUrl(#[from] ParsedUrlError),
    #[error("Path must be UTF-8: `{0}`")]
    NonUtf8Path(PathBuf),
    #[error(transparent)] // Function attaches the context
    RelativeTo(io::Error),
}

#[derive(Debug, Copy, Clone)]
pub enum SourceKind {
    Path,
    Url,
    Git,
    Registry,
}

impl std::fmt::Display for SourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceKind::Path => write!(f, "path"),
            SourceKind::Url => write!(f, "URL"),
            SourceKind::Git => write!(f, "Git"),
            SourceKind::Registry => write!(f, "registry"),
        }
    }
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
        git: GitUrl::from_reference(repository, reference)?,
        subdirectory,
    })
}

/// Convert a URL source into a [`RequirementSource`].
fn url_source(
    requirement: &uv_pep508::Requirement<VerbatimParsedUrl>,
    url: Url,
    subdirectory: Option<PathBuf>,
) -> Result<RequirementSource, LoweringError> {
    let mut verbatim_url = url.clone();
    if verbatim_url.fragment().is_some() {
        return Err(LoweringError::ForbiddenFragment(url));
    }
    if let Some(subdirectory) = subdirectory.as_ref() {
        let subdirectory = subdirectory
            .to_str()
            .ok_or_else(|| LoweringError::NonUtf8Path(subdirectory.clone()))?;
        verbatim_url.set_fragment(Some(&format!("subdirectory={subdirectory}")));
    }

    let ext = match DistExtension::from_path(url.path()) {
        Ok(ext) => ext,
        Err(..) if looks_like_git_repository(&url) => {
            return Err(LoweringError::MissingGitSource(
                requirement.name.clone(),
                url.clone(),
            ))
        }
        Err(err) => {
            return Err(ParsedUrlError::MissingExtensionUrl(url.to_string(), err).into());
        }
    };

    let verbatim_url = VerbatimUrl::from_url(verbatim_url);
    Ok(RequirementSource::Url {
        location: url,
        subdirectory,
        ext,
        url: verbatim_url,
    })
}

/// Convert a registry source into a [`RequirementSource`].
fn registry_source(
    requirement: &uv_pep508::Requirement<VerbatimParsedUrl>,
    index: Url,
    conflict: Option<ConflictItem>,
) -> RequirementSource {
    match &requirement.version_or_url {
        None => RequirementSource::Registry {
            specifier: VersionSpecifiers::empty(),
            index: Some(index),
            conflict,
        },
        Some(VersionOrUrl::VersionSpecifier(version)) => RequirementSource::Registry {
            specifier: version.clone(),
            index: Some(index),
            conflict,
        },
        Some(VersionOrUrl::Url(_)) => RequirementSource::Registry {
            specifier: VersionSpecifiers::empty(),
            index: Some(index),
            conflict,
        },
    }
}

/// Convert a path string to a file or directory source.
fn path_source(
    path: impl AsRef<Path>,
    git_member: Option<&GitWorkspaceMember>,
    origin: RequirementOrigin,
    project_dir: &Path,
    workspace_root: &Path,
    editable: Option<bool>,
    package: Option<bool>,
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
        if let Some(git_member) = git_member {
            let subdirectory = uv_fs::relative_to(install_path, git_member.fetch_root)
                .expect("Workspace member must be relative");
            let subdirectory = uv_fs::normalize_path_buf(subdirectory);
            return Ok(RequirementSource::Git {
                git: git_member.git_source.git.clone(),
                subdirectory: if subdirectory == PathBuf::new() {
                    None
                } else {
                    Some(subdirectory)
                },
                url,
            });
        }

        if editable == Some(true) {
            Ok(RequirementSource::Directory {
                install_path,
                url,
                editable: true,
                r#virtual: false,
            })
        } else {
            // Determine whether the project is a package or virtual.
            let is_package = package.unwrap_or_else(|| {
                let pyproject_path = install_path.join("pyproject.toml");
                fs_err::read_to_string(&pyproject_path)
                    .ok()
                    .and_then(|contents| PyProjectToml::from_string(contents).ok())
                    .map(|pyproject_toml| pyproject_toml.is_package())
                    .unwrap_or(true)
            });

            Ok(RequirementSource::Directory {
                install_path,
                url,
                editable: false,
                // If a project is not a package, treat it as a virtual dependency.
                r#virtual: !is_package,
            })
        }
    } else {
        // TODO(charlie): If a Git repo contains a source that points to a file, what should we do?
        if git_member.is_some() {
            return Err(LoweringError::GitFile(url.to_string()));
        }
        if editable == Some(true) {
            return Err(LoweringError::EditableFile(url.to_string()));
        }
        if package == Some(true) {
            return Err(LoweringError::PackagedFile(url.to_string()));
        }
        Ok(RequirementSource::Path {
            ext: DistExtension::from_path(&install_path)
                .map_err(|err| ParsedUrlError::MissingExtensionPath(path.to_path_buf(), err))?,
            install_path,
            url,
        })
    }
}

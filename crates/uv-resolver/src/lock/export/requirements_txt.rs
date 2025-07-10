use std::borrow::Cow;
use std::fmt::Formatter;
use std::path::{Component, Path, PathBuf};

use owo_colors::OwoColorize;
use url::Url;

use uv_configuration::{
    DependencyGroupsWithDefaults, EditableMode, ExtrasSpecificationWithDefaults, InstallOptions,
};
use uv_distribution_filename::{DistExtension, SourceDistExtension};
use uv_fs::Simplified;
use uv_git_types::GitReference;
use uv_normalize::PackageName;
use uv_pypi_types::{ParsedArchiveUrl, ParsedGitUrl};
use uv_redacted::DisplaySafeUrl;

use crate::lock::export::{ExportableRequirement, ExportableRequirements};
use crate::lock::{Package, PackageId, Source};
use crate::{Installable, LockError};

/// An export of a [`Lock`] that renders in `requirements.txt` format.
#[derive(Debug)]
pub struct RequirementsTxtExport<'lock> {
    nodes: Vec<ExportableRequirement<'lock>>,
    hashes: bool,
    editable: EditableMode,
}

impl<'lock> RequirementsTxtExport<'lock> {
    pub fn from_lock(
        target: &impl Installable<'lock>,
        prune: &[PackageName],
        extras: &ExtrasSpecificationWithDefaults,
        dev: &DependencyGroupsWithDefaults,
        annotate: bool,
        editable: EditableMode,
        hashes: bool,
        install_options: &'lock InstallOptions,
    ) -> Result<Self, LockError> {
        // Extract the packages from the lock file.
        let ExportableRequirements(mut nodes) = ExportableRequirements::from_lock(
            target,
            prune,
            extras,
            dev,
            annotate,
            install_options,
        );

        // Sort the nodes, such that unnamed URLs (editables) appear at the top.
        nodes.sort_unstable_by(|a, b| {
            RequirementComparator::from(a.package).cmp(&RequirementComparator::from(b.package))
        });

        Ok(Self {
            nodes,
            hashes,
            editable,
        })
    }
}

impl std::fmt::Display for RequirementsTxtExport<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Write out each package.
        for ExportableRequirement {
            package,
            marker,
            dependents,
        } in &self.nodes
        {
            match &package.id.source {
                Source::Registry(_) => {
                    let version = package
                        .id
                        .version
                        .as_ref()
                        .expect("registry package without version");
                    write!(f, "{}=={}", package.id.name, version)?;
                }
                Source::Git(url, git) => {
                    // Remove the fragment and query from the URL; they're already present in the
                    // `GitSource`.
                    let mut url = url.to_url().map_err(|_| std::fmt::Error)?;
                    url.set_fragment(None);
                    url.set_query(None);

                    // Reconstruct the `GitUrl` from the `GitSource`.
                    let git_url = uv_git_types::GitUrl::from_commit(
                        url,
                        GitReference::from(git.kind.clone()),
                        git.precise,
                    )
                    .expect("Internal Git URLs must have supported schemes");

                    // Reconstruct the PEP 508-compatible URL from the `GitSource`.
                    let url = DisplaySafeUrl::from(ParsedGitUrl {
                        url: git_url.clone(),
                        subdirectory: git.subdirectory.clone(),
                    });

                    write!(f, "{} @ {}", package.id.name, url)?;
                }
                Source::Direct(url, direct) => {
                    let url = DisplaySafeUrl::from(ParsedArchiveUrl {
                        url: url.to_url().map_err(|_| std::fmt::Error)?,
                        subdirectory: direct.subdirectory.clone(),
                        ext: DistExtension::Source(SourceDistExtension::TarGz),
                    });
                    write!(
                        f,
                        "{} @ {}",
                        package.id.name,
                        // TODO(zanieb): We should probably omit passwords here by default, but we
                        // should change it in a breaking release and allow opt-in to include them.
                        url.displayable_with_credentials()
                    )?;
                }
                Source::Path(path) | Source::Directory(path) => {
                    if path.is_absolute() {
                        write!(
                            f,
                            "{}",
                            Url::from_file_path(path).map_err(|()| std::fmt::Error)?
                        )?;
                    } else {
                        write!(f, "{}", anchor(path).portable_display())?;
                    }
                }
                Source::Editable(path) => match self.editable {
                    EditableMode::Editable => {
                        write!(f, "-e {}", anchor(path).portable_display())?;
                    }
                    EditableMode::NonEditable => {
                        if path.is_absolute() {
                            write!(
                                f,
                                "{}",
                                Url::from_file_path(path).map_err(|()| std::fmt::Error)?
                            )?;
                        } else {
                            write!(f, "{}", anchor(path).portable_display())?;
                        }
                    }
                },
                Source::Virtual(_) => {
                    continue;
                }
            }

            if let Some(contents) = marker.contents() {
                write!(f, " ; {contents}")?;
            }

            if self.hashes {
                let mut hashes = package.hashes();
                hashes.sort_unstable();
                if !hashes.is_empty() {
                    for hash in hashes.iter() {
                        writeln!(f, " \\")?;
                        write!(f, "    --hash=")?;
                        write!(f, "{hash}")?;
                    }
                }
            }

            writeln!(f)?;

            // Add "via ..." comments for all dependents.
            match dependents.as_slice() {
                [] => {}
                [dependent] => {
                    writeln!(f, "{}", format!("    # via {}", dependent.id.name).green())?;
                }
                _ => {
                    writeln!(f, "{}", "    # via".green())?;
                    for &dependent in dependents {
                        writeln!(f, "{}", format!("    #   {}", dependent.id.name).green())?;
                    }
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum RequirementComparator<'lock> {
    Editable(&'lock Path),
    Path(&'lock Path),
    Package(&'lock PackageId),
}

impl<'lock> From<&'lock Package> for RequirementComparator<'lock> {
    fn from(value: &'lock Package) -> Self {
        match &value.id.source {
            Source::Path(path) | Source::Directory(path) => Self::Path(path),
            Source::Editable(path) => Self::Editable(path),
            _ => Self::Package(&value.id),
        }
    }
}

/// Modify a relative [`Path`] to anchor it at the current working directory.
///
/// For example, given `foo/bar`, returns `./foo/bar`.
fn anchor(path: &Path) -> Cow<'_, Path> {
    match path.components().next() {
        None => Cow::Owned(PathBuf::from(".")),
        Some(Component::CurDir | Component::ParentDir) => Cow::Borrowed(path),
        _ => Cow::Owned(PathBuf::from("./").join(path)),
    }
}

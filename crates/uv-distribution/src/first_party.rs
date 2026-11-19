use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use uv_distribution_types::{BuildableSource, RequirementSource, SourceDist};
use uv_fs::normalize_path;
use uv_normalize::PackageName;
use uv_workspace::Workspace;

/// Workspace packages eligible for first-party metadata builds.
///
/// A package must match both the name and source path of a non-virtual workspace member. Members
/// explicitly excluded by installation filters are not eligible.
#[derive(Debug, Default)]
pub struct FirstPartyPackages {
    members: BTreeMap<PackageName, PathBuf>,
}

impl FirstPartyPackages {
    /// Collect eligible members of a workspace, excluding the given package names.
    pub fn from_workspace(workspace: &Workspace, exclusions: &BTreeSet<PackageName>) -> Self {
        let members = workspace
            .members_requirements()
            .filter(|requirement| !exclusions.contains(&requirement.name))
            .filter_map(|requirement| match requirement.source {
                RequirementSource::Directory {
                    install_path,
                    r#virtual: Some(false),
                    ..
                } => Some((requirement.name, install_path.into_path_buf())),
                RequirementSource::Directory { .. }
                | RequirementSource::Registry { .. }
                | RequirementSource::Url { .. }
                | RequirementSource::GitDirectory { .. }
                | RequirementSource::GitPath { .. }
                | RequirementSource::Path { .. } => None,
            })
            .collect();
        Self { members }
    }

    /// Return whether the name and path identify an eligible member.
    pub(crate) fn contains(&self, name: &PackageName, path: &Path) -> bool {
        self.members
            .get(name)
            .is_some_and(|member| normalize_path(member.as_path()) == normalize_path(path))
    }

    /// Return whether the source identifies an eligible member.
    pub(crate) fn contains_source(&self, source: &BuildableSource<'_>) -> bool {
        let BuildableSource::Dist(SourceDist::Directory(directory)) = source else {
            return false;
        };
        !directory.r#virtual.unwrap_or(false)
            && self.contains(&directory.name, &directory.install_path)
    }
}

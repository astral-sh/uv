use std::path::Path;

use uv_distribution_types::{Requirement, RequirementSource};

use super::requirements::normalize_requirement;
use super::{Lock, LockError, Package, Source};

impl Lock {
    /// Return the workspace root when its declarations can be checked without marker forks.
    ///
    /// Restrict the check to unconditional edges from one workspace root. A more general graph
    /// can have declarations that apply only in particular extras, groups, or conflict contexts
    /// and needs full marker coverage.
    pub(super) fn simple_root(&self) -> Option<&Package> {
        let root = self.root()?;
        if self.supports_missing_package_metadata()
            || root.is_dynamic()
            || !root.has_metadata()
            || !self.fork_markers.is_empty()
            || !self.conflicts.is_empty()
            || !self.manifest.requirements.is_empty()
            || !self.manifest.dependency_groups.is_empty()
            || !self.manifest.overrides.is_empty()
            || !self.manifest.excludes.is_empty()
            || !self.manifest.dependency_metadata.is_empty()
        {
            return None;
        }
        if self.packages.iter().any(|package| {
            !package.fork_markers.is_empty()
                || !package.optional_dependencies.is_empty()
                || !package.dependency_groups.is_empty()
                || self.is_workspace_package(package) && package.id != root.id
                || package.dependencies.iter().any(|dependency| {
                    !dependency.extra.is_empty()
                        || !dependency
                            .simplified_marker
                            .as_simplified_marker_tree()
                            .is_true()
                })
        }) {
            return None;
        }
        if !root.metadata.provides_extra.is_empty()
            || !root.metadata.dependency_groups.is_empty()
            || root
                .metadata
                .requires_dist
                .iter()
                .any(|requirement| !requirement.marker.is_true())
        {
            return None;
        }
        Some(root)
    }

    /// Return the only external source when its authorization can be checked from the root.
    pub(super) fn simple_external_source(&self) -> Option<&Package> {
        let mut external = self.packages.iter().filter(|package| {
            !matches!(package.id.source, Source::Registry(_)) && !self.is_workspace_package(package)
        });
        let package = external.next()?;
        if external.next().is_some() {
            return None;
        }
        self.simple_root()?;
        Some(package)
    }

    /// Return whether a current declaration selects this exact source without a marker.
    pub(super) fn source_selected_by(
        package: &Package,
        requirement: &Requirement,
        root: &Path,
    ) -> Result<bool, LockError> {
        if requirement.name != package.id.name || !requirement.marker.is_true() {
            return Ok(false);
        }
        package
            .id
            .source
            .satisfies_requirement_source(&requirement.source, root)
    }

    /// Omit an unconditional source constraint when its selected source can be revalidated.
    pub(super) fn can_omit_source_constraint(&self, constraint: &Requirement, root: &Path) -> bool {
        if matches!(constraint.source, RequirementSource::Registry { .. }) {
            return false;
        }
        let Some(package) = self.simple_external_source() else {
            return false;
        };
        let Ok(constraint) = normalize_requirement(constraint.clone(), root, &self.requires_python)
        else {
            return false;
        };
        Self::source_selected_by(package, &constraint, root).is_ok_and(|selected| selected)
    }
}

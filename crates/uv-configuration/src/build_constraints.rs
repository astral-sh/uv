use std::borrow::Cow;

use uv_distribution_types::NameRequirementSpecification;
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::{Constraint, Constraints};

/// Constraints selected by the identity of the package whose build environment is being resolved.
#[derive(Debug, Default, Clone)]
pub struct BuildConstraints {
    entries: Vec<Constraint<NameRequirementSpecification>>,
    global: Constraints,
}

impl BuildConstraints {
    /// Create global build constraints, preserving their archive hashes.
    pub fn from_specifications(
        specifications: impl IntoIterator<Item = NameRequirementSpecification>,
    ) -> Self {
        Self::from_entries(specifications.into_iter().map(Constraint::Requirement))
    }

    /// Retain global and package-scoped declarations for individual build environments.
    pub fn from_entries(
        entries: impl IntoIterator<Item = Constraint<NameRequirementSpecification>>,
    ) -> Self {
        let entries: Vec<_> = entries.into_iter().collect();
        let global = Constraints::from_specifications(
            entries
                .iter()
                .filter_map(Constraint::as_requirement)
                .cloned(),
        );
        Self { entries, global }
    }

    /// Return all declarations, including scopes and archive hashes, in input order.
    pub fn entries(&self) -> impl Iterator<Item = &Constraint<NameRequirementSpecification>> {
        self.entries.iter()
    }

    /// Return the constraints shared by every build environment.
    pub fn global(&self) -> &Constraints {
        &self.global
    }

    /// Whether this build environment has package-specific constraints.
    pub fn has_scope(&self, name: Option<&PackageName>, version: Option<&Version>) -> bool {
        self.entries.iter().any(|entry| match entry {
            Constraint::Requirement(_) => false,
            Constraint::Package(package) => {
                name.is_some_and(|name| package.package.matches(name, version))
            }
        })
    }

    /// Select constraints for an entire build dependency graph.
    ///
    /// An exact-version scope is inactive when the package's version is not yet known.
    pub fn for_package(
        &self,
        name: Option<&PackageName>,
        version: Option<&Version>,
    ) -> Cow<'_, Constraints> {
        if !self.has_scope(name, version) {
            return Cow::Borrowed(&self.global);
        }
        Cow::Owned(Constraints::from_specifications(
            self.entries
                .iter()
                .flat_map(|entry| match entry {
                    Constraint::Requirement(requirement) => std::slice::from_ref(requirement),
                    Constraint::Package(package)
                        if name.is_some_and(|name| package.package.matches(name, version)) =>
                    {
                        &package.dependencies
                    }
                    Constraint::Package(_) => &[],
                })
                .cloned(),
        ))
    }
}

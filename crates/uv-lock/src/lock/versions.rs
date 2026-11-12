use uv_distribution_types::{Requirement, RequirementSource};
use uv_pep508::MarkerTree;

use super::{Dependency, DependencyContext, Lock, LockSatisfactionError, Package, marker_is_unreachable};

impl Lock {
    /// Check that resolved versions satisfy the declarations available in the lockfile.
    pub(super) fn check_locked_versions(&self) -> Option<LockSatisfactionError> {
        VersionChecker { lock: self }.check()
    }
}

struct VersionChecker<'lock> {
    /// Supplies the exact locked selections and any recorded dependency declarations.
    lock: &'lock Lock,
}

impl<'lock> VersionChecker<'lock> {
    fn check(&self) -> Option<LockSatisfactionError> {
        for package in &self.lock.packages {
            let context = DependencyContext::Production;
            let dependencies = context.dependencies(package);
            if let Some(mismatch) =
                self.check_requirements(context, &package.metadata.requires_dist, dependencies)
            {
                return Some(mismatch);
            }
        }
        None
    }

    fn check_requirements(
        &self,
        context: DependencyContext<'lock>,
        requirements: &[Requirement],
        dependencies: &'lock [Dependency],
    ) -> Option<LockSatisfactionError> {
        for requirement in requirements {
            let first = dependencies
                .partition_point(|dependency| dependency.package_id.name < requirement.name);
            for dependency in dependencies[first..]
                .iter()
                .take_while(|dependency| dependency.package_id.name == requirement.name)
            {
                let target = self.lock.package(dependency.index);
                if let Some(mismatch) = self.check_requirement(target, requirement, || {
                    context
                        .requirement_marker(requirement.marker)
                        .and(dependency.complexified_marker.combined())
                }) {
                    return Some(mismatch);
                }
            }
        }
        None
    }

    /// Given a package in the lock file and its corresponding
    /// requirement (also from the lock file), along with a
    /// corresponding marker, check whether there is a mismatch
    /// between the package version and its requirement.
    ///
    /// `None` is returned when there is no mismatch.
    fn check_requirement(
        &self,
        package: &'lock Package,
        requirement: &Requirement,
        marker: impl FnOnce() -> MarkerTree,
    ) -> Option<LockSatisfactionError> {
        let version = package.id.version.as_ref()?;
        // Only registry requirements carry a version specifier. Direct references select a URL,
        // Git source, or local path and are outside this check.
        let RequirementSource::Registry { specifier, .. } = &requirement.source else {
            return None;
        };
        if specifier.contains(version) {
            return None;
        }

        if marker_is_unreachable(&self.lock.requires_python, marker()) {
            return None;
        }

        Some(LockSatisfactionError::MismatchedDependencyVersion(
            package.id.name.clone(),
            version.clone(),
            specifier.clone(),
        ))
    }
}

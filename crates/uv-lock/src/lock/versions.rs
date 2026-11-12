use std::iter;

use uv_configuration::{Constraints, DependencyModifiers};
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{DependencyMetadata, Requirement, RequirementScope, RequirementSource};
use uv_pep440::VersionSpecifiers;
use uv_pep508::MarkerTree;

use super::{
    Dependency, DependencyContext, Lock, Package, PackageIndex, SatisfiesResult, Source,
    marker_is_unreachable,
};

impl Lock {
    /// Check that resolved versions satisfy the declarations available in the lockfile.
    pub(super) fn check_locked_versions(
        &self,
        dependency_metadata: &DependencyMetadata,
        constraints: Constraints,
        modifiers: &DependencyModifiers,
    ) -> Option<SatisfiesResult<'_>> {
        VersionChecker {
            lock: self,
            dependency_metadata,
            constraints,
            modifiers,
        }
        .check()
    }
}

/// Detects version mismatches from the lockfile and validated resolution inputs.
struct VersionChecker<'lock, 'settings> {
    /// Supplies the exact locked selections and any recorded dependency declarations.
    lock: &'lock Lock,
    /// User-provided declarations checked against the manifest.
    dependency_metadata: &'settings DependencyMetadata,
    /// Normalized constraints that matched the manifest, checked alongside dependency requirements.
    constraints: Constraints,
    /// Overrides and exclusions that determine the effective requirements in each scope.
    modifiers: &'settings DependencyModifiers,
}

impl<'lock> VersionChecker<'lock, '_> {
    fn check(&self) -> Option<SatisfiesResult<'lock>> {
        let has_constraints = self.constraints.requirements().next().is_some();
        let has_overrides = !self.lock.manifest.overrides.is_empty();
        for (index, package) in self.lock.packages.iter().enumerate() {
            // Registry and Git packages omit their declarations from the lockfile. For Git,
            // only a wheel filename gives the resolver a version before it reads the metadata.
            let configured_metadata = match &package.id.source {
                Source::Registry(_) => package.id.version.as_ref().and_then(|version| {
                    self.dependency_metadata
                        .get(&package.id.name, Some(version))
                }),
                Source::Git(_, git) => {
                    let wheel = git
                        .path
                        .as_ref()
                        .and_then(|path| path.file_name()?.to_str()?.parse::<WheelFilename>().ok());
                    self.dependency_metadata
                        .get(&package.id.name, wheel.as_ref().map(|wheel| &wheel.version))
                }
                Source::Direct(..)
                | Source::Path(_)
                | Source::Directory(_)
                | Source::Editable(_)
                | Source::Virtual(_) => None,
            };
            let configured_requirements = configured_metadata.map(|metadata| {
                Box::into_iter(metadata.requires_dist)
                    .map(Requirement::from)
                    .collect::<Vec<_>>()
            });
            let requires_dist = configured_requirements
                .as_deref()
                .unwrap_or(&package.metadata.requires_dist);
            if !has_constraints
                && !has_overrides
                && requires_dist.is_empty()
                && package.metadata.dependency_groups.is_empty()
            {
                continue;
            }
            let index = PackageIndex(index);
            for context in iter::once(DependencyContext::Production)
                .chain(
                    package
                        .optional_dependencies
                        .keys()
                        .map(DependencyContext::Extra),
                )
                .chain(
                    package
                        .dependency_groups
                        .keys()
                        .map(DependencyContext::Group),
                )
            {
                let dependencies = context.dependencies(package);
                if dependencies.is_empty() {
                    continue;
                }
                let requirements: &[Requirement] = match context {
                    DependencyContext::Production | DependencyContext::Extra(_) => requires_dist,
                    DependencyContext::Group(group) => package
                        .metadata
                        .dependency_groups
                        .get(group)
                        .map_or(&[], Box::as_ref),
                };
                if let Some(mismatch) =
                    self.check_requirements(index, context, requirements, dependencies)
                {
                    return Some(mismatch);
                }
                if !has_constraints {
                    continue;
                }

                for dependency in dependencies {
                    if let Some(mismatch) = self.check_constraints(
                        self.lock.package(dependency.index),
                        context,
                        |constraint| constraint.and(dependency.complexified_marker.combined()),
                    ) {
                        return Some(mismatch);
                    }
                }
            }
        }
        None
    }

    fn check_requirements(
        &self,
        index: PackageIndex,
        context: DependencyContext<'lock>,
        requirements: &[Requirement],
        dependencies: &'lock [Dependency],
    ) -> Option<SatisfiesResult<'lock>> {
        let package = self.lock.package(index);
        // If this parent has package-scoped overrides or exclusions and its version is missing,
        // skip its production and optional requirements. For example, we can't tell whether an
        // `idna` override scoped to `anyio==3.7.0` applies to a checkout with a dynamic version.
        // Dependency groups use only global overrides. A package-scoped exclusion removes the
        // dependency's group edge, so there is no locked version to check for it.
        if package.id.version.is_none()
            && self.modifiers.has_scoped_package(&package.id.name)
            && !matches!(context, DependencyContext::Group(_))
        {
            return None;
        }

        // The lock usually omits registry packages' declared requirements. Start with a bare
        // requirement for each locked dependency so a matching override can supply its specifier.
        let inferred = requirements.is_empty() && !self.lock.manifest.overrides.is_empty();
        let inferred_requirements;
        let requirements = if inferred {
            inferred_requirements = dependencies
                .iter()
                .map(|dependency| Requirement {
                    name: dependency.package_id.name.clone(),
                    extras: Box::new([]),
                    groups: Box::new([]),
                    marker: MarkerTree::TRUE,
                    source: RequirementSource::Registry {
                        specifier: VersionSpecifiers::empty(),
                        index: None,
                        conflict: None,
                    },
                    scope: RequirementScope::Global,
                    origin: None,
                })
                .collect::<Vec<_>>();
            &inferred_requirements
        } else {
            requirements
        };
        for requirement in Lock::preprocess_requirements(
            &package.id.name,
            package.id.version.as_ref(),
            requirements,
            context,
            self.modifiers,
        ) {
            let first = dependencies
                .partition_point(|dependency| dependency.package_id.name < requirement.name);
            for dependency in dependencies[first..]
                .iter()
                .take_while(|dependency| dependency.package_id.name == requirement.name)
            {
                let target = self.lock.package(dependency.index);
                if let Some(mismatch) = self.check_requirement(target, &requirement, || {
                    let marker = context.requirement_marker(requirement.marker);
                    marker.and(dependency.complexified_marker.combined())
                }) {
                    return Some(mismatch);
                }
            }
        }
        None
    }

    fn check_constraints(
        &self,
        package: &'lock Package,
        context: DependencyContext<'_>,
        marker: impl Fn(MarkerTree) -> MarkerTree,
    ) -> Option<SatisfiesResult<'lock>> {
        self.constraints
            .get(&package.id.name)?
            .iter()
            .find_map(|constraint| {
                self.check_requirement(package, constraint, || {
                    marker(context.requirement_marker(constraint.marker))
                })
            })
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
    ) -> Option<SatisfiesResult<'lock>> {
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

        Some(SatisfiesResult::MismatchedDependencyVersion(
            &package.id.name,
            version,
            specifier.clone(),
        ))
    }
}

use std::sync::Arc;

use futures::channel::mpsc::UnboundedSender;
use resolvo::{Candidates, Dependencies, NameId, Pool, SolvableId, SolverCache};
use tokio::runtime::Handle;
use tracing::info;

use pep440_rs::{VersionSpecifier, VersionSpecifiers};
use pep508_rs::{MarkerEnvironment, VersionOrUrl};
use puffin_distribution::Dist;
use puffin_traits::BuildContext;

use crate::database::{Database, Request};
use crate::file::DistFile;
use crate::resolvo::{ResolvoPackage, ResolvoVersion, ResolvoVersionSet};

/// A [`resolvo::DependencyProvider`] that uses a [`Database`] to fetch dependencies.
pub(crate) struct ResolvoDependencyProvider<Context: BuildContext> {
    database: Arc<Database<Context>>,
    sender: UnboundedSender<Request>,
    markers: MarkerEnvironment,
    pool: Pool<ResolvoVersionSet, ResolvoPackage>,
}

impl<Context: BuildContext> ResolvoDependencyProvider<Context> {
    /// Initialize a new [`ResolvoDependencyProvider`] with the given [`Database`].
    pub(crate) fn new(
        database: Arc<Database<Context>>,
        sender: UnboundedSender<Request>,
        markers: MarkerEnvironment,
    ) -> Self {
        Self {
            database,
            sender,
            markers,
            pool: Pool::new(),
        }
    }

    /// Return the underlying [`Pool`].
    pub(crate) fn pool(&self) -> &Pool<ResolvoVersionSet, ResolvoPackage> {
        &self.pool
    }

    /// Convert a [`SolvableId`] into a [`Dist`].
    pub(crate) fn dist(&self, solvable: SolvableId) -> Dist {
        let solvable = self.pool.resolve_solvable(solvable);
        let package = self.pool.resolve_package_name(solvable.name_id());
        match solvable.inner() {
            ResolvoVersion::Version(version) => {
                let metadata = self.database.get_package(package.name()).unwrap();
                let version_map = metadata.value();
                let file = version_map.get(&version.clone()).unwrap();
                match file {
                    DistFile::Wheel(file) => Dist::from_registry(
                        package.name().clone(),
                        version.clone(),
                        file.clone().into(),
                    ),
                    DistFile::Sdist(file) => Dist::from_registry(
                        package.name().clone(),
                        version.clone(),
                        file.clone().into(),
                    ),
                }
            }
            ResolvoVersion::Url(url) => Dist::from_url(package.name().clone(), url.clone()),
        }
    }
}

impl<Context: BuildContext> resolvo::DependencyProvider<ResolvoVersionSet, ResolvoPackage>
    for &ResolvoDependencyProvider<Context>
{
    fn pool(&self) -> &Pool<ResolvoVersionSet, ResolvoPackage> {
        &self.pool
    }

    /// Sort candidates such that the highest version is preferred.
    fn sort_candidates(
        &self,
        solver: &SolverCache<ResolvoVersionSet, ResolvoPackage, Self>,
        solvables: &mut [SolvableId],
    ) {
        solvables.sort_by_key(|&solvable| {
            let solvable = solver.pool().resolve_solvable(solvable);
            std::cmp::Reverse(solvable.inner())
        });
    }

    /// Return all candidate distributions for a given package.
    fn get_candidates(&self, name: NameId) -> Option<Candidates> {
        let package = self.pool.resolve_package_name(name);
        let package_name = package.name();

        info!("Fetching candidates for: {package_name}");

        // Get the metadata for this package, which includes the `VersionMap`.
        let entry = tokio::task::block_in_place(|| {
            Handle::current().block_on(self.database.wait_package(&self.sender, package.name()))
        });
        let version_map = entry.value();

        // Create a candidate for each version in the `VersionMap`.
        let mut candidates = Candidates::default();
        for version in version_map.keys() {
            // TODO(charlie): Implement proper pre-release support.
            if version.any_prerelease() {
                continue;
            }
            let solvable_id = self
                .pool
                .intern_solvable(name, ResolvoVersion::Version(version.clone()));
            candidates.candidates.push(solvable_id);
        }

        Some(candidates)
    }

    fn get_dependencies(&self, solvable: SolvableId) -> Dependencies {
        let solvable = self.pool.resolve_solvable(solvable);
        let package = self.pool.resolve_package_name(solvable.name_id());
        let package_name = package.name();
        let extra = package.extra();

        info!("Fetching dependencies for: {package_name}");

        let entry = match solvable.inner() {
            ResolvoVersion::Version(version) => {
                let metadata = self.database.get_package(package_name).unwrap();
                let version_map = metadata.value();
                let file = version_map.get(&version.clone()).unwrap();
                tokio::task::block_in_place(|| {
                    Handle::current().block_on(self.database.wait_file(
                        &self.sender,
                        package_name,
                        version,
                        file,
                    ))
                })
            }
            ResolvoVersion::Url(url) => tokio::task::block_in_place(|| {
                Handle::current().block_on(self.database.wait_url(&self.sender, package_name, url))
            }),
        };
        let metadata = entry.value();

        let mut dependencies = Dependencies::default();

        match package {
            ResolvoPackage::Package(package_name) => {
                // Ensure that extra packages are pinned to the same version as the base package.
                for extra in &metadata.provides_extras {
                    let solvable = self.pool.intern_package_name(ResolvoPackage::Extra(
                        package_name.clone(),
                        extra.clone(),
                    ));
                    let specifiers =
                        VersionSpecifiers::from_iter([VersionSpecifier::equals_version(
                            metadata.version.clone(),
                        )]);
                    let version_set_id = self.pool.intern_version_set(
                        solvable,
                        Some(VersionOrUrl::VersionSpecifier(specifiers)).into(),
                    );
                    dependencies.constrains.push(version_set_id);
                }
            }
            ResolvoPackage::Extra(package_name, _extra) => {
                // Mark the extra as a dependency of the base package.
                let ResolvoVersion::Version(package_version) = solvable.inner() else {
                    unreachable!("extra should only be set for registry packages");
                };

                let base_name_id = self
                    .pool
                    .lookup_package_name(&ResolvoPackage::Package(package_name.clone()))
                    .expect("extra should have base");
                let specifiers = VersionSpecifiers::from_iter([VersionSpecifier::equals_version(
                    package_version.clone(),
                )]);
                let version_set_id = self.pool.intern_version_set(
                    base_name_id,
                    Some(VersionOrUrl::VersionSpecifier(specifiers)).into(),
                );
                dependencies.requirements.push(version_set_id);
            }
        }

        // Iterate over all declared requirements.
        for requirement in &metadata.requires_dist {
            // If the requirement isn't relevant for the current platform, skip it.
            if let Some(extra) = extra {
                if !requirement.evaluate_markers(&self.markers, &[extra.as_ref()]) {
                    continue;
                }
            } else {
                if !requirement.evaluate_markers(&self.markers, &[]) {
                    continue;
                }
            }

            // Add a dependency on the package itself.
            let dependency_name_id = self
                .pool
                .intern_package_name(ResolvoPackage::Package(requirement.name.clone()));
            let version_set_id = self.pool.intern_version_set(
                dependency_name_id,
                requirement.version_or_url.clone().into(),
            );
            dependencies.requirements.push(version_set_id);

            // Add an additional package for each extra.
            for extra in requirement.extras.iter().flatten() {
                let dependency_name_id = self.pool.intern_package_name(ResolvoPackage::Extra(
                    requirement.name.clone(),
                    extra.clone(),
                ));
                let version_set_id = self.pool.intern_version_set(
                    dependency_name_id,
                    requirement.version_or_url.clone().into(),
                );
                dependencies.requirements.push(version_set_id);
            }
        }

        dependencies
    }
}

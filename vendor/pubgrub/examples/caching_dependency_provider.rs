// SPDX-License-Identifier: MPL-2.0

use std::cell::RefCell;
use std::error::Error;

use pubgrub::package::Package;
use pubgrub::range::Range;
use pubgrub::solver::{resolve, Dependencies, DependencyProvider, OfflineDependencyProvider};
use pubgrub::version::NumberVersion;
use pubgrub::version_set::VersionSet;

type NumVS = Range<NumberVersion>;

// An example implementing caching dependency provider that will
// store queried dependencies in memory and check them before querying more from remote.
struct CachingDependencyProvider<P: Package, VS: VersionSet, DP: DependencyProvider<P, VS>> {
    remote_dependencies: DP,
    cached_dependencies: RefCell<OfflineDependencyProvider<P, VS>>,
}

impl<P: Package, VS: VersionSet, DP: DependencyProvider<P, VS>>
    CachingDependencyProvider<P, VS, DP>
{
    pub fn new(remote_dependencies_provider: DP) -> Self {
        CachingDependencyProvider {
            remote_dependencies: remote_dependencies_provider,
            cached_dependencies: RefCell::new(OfflineDependencyProvider::new()),
        }
    }
}

impl<P: Package, VS: VersionSet, DP: DependencyProvider<P, VS>> DependencyProvider<P, VS>
    for CachingDependencyProvider<P, VS, DP>
{
    fn choose_package_version<T: std::borrow::Borrow<P>, U: std::borrow::Borrow<VS>>(
        &self,
        packages: impl Iterator<Item = (T, U)>,
    ) -> Result<(T, Option<VS::V>), Box<dyn Error + Send + Sync>> {
        self.remote_dependencies.choose_package_version(packages)
    }

    // Caches dependencies if they were already queried
    fn get_dependencies(
        &self,
        package: &P,
        version: &VS::V,
    ) -> Result<Dependencies<P, VS>, Box<dyn Error + Send + Sync>> {
        let mut cache = self.cached_dependencies.borrow_mut();
        match cache.get_dependencies(package, version) {
            Ok(Dependencies::Unknown) => {
                let dependencies = self.remote_dependencies.get_dependencies(package, version);
                match dependencies {
                    Ok(Dependencies::Known(dependencies)) => {
                        cache.add_dependencies(
                            package.clone(),
                            version.clone(),
                            dependencies.clone(),
                        );
                        Ok(Dependencies::Known(dependencies))
                    }
                    Ok(Dependencies::Unknown) => Ok(Dependencies::Unknown),
                    error @ Err(_) => error,
                }
            }
            dependencies @ Ok(_) => dependencies,
            error @ Err(_) => error,
        }
    }
}

fn main() {
    // Simulating remote provider locally.
    let mut remote_dependencies_provider = OfflineDependencyProvider::<&str, NumVS>::new();

    // Add dependencies as needed. Here only root package is added.
    remote_dependencies_provider.add_dependencies("root", 1, Vec::new());

    let caching_dependencies_provider =
        CachingDependencyProvider::new(remote_dependencies_provider);

    let solution = resolve(&caching_dependencies_provider, "root", 1);
    println!("Solution: {:?}", solution);
}

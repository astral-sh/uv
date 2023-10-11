// SPDX-License-Identifier: MPL-2.0

use std::cell::RefCell;
use std::error::Error;

use pubgrub::package::Package;
use pubgrub::range::Range;
use pubgrub::solver::{resolve, Dependencies, DependencyProvider, OfflineDependencyProvider};
use pubgrub::version::{NumberVersion, Version};

// An example implementing caching dependency provider that will
// store queried dependencies in memory and check them before querying more from remote.
struct CachingDependencyProvider<P: Package, V: Version, DP: DependencyProvider<P, V>> {
    remote_dependencies: DP,
    cached_dependencies: RefCell<OfflineDependencyProvider<P, V>>,
}

impl<P: Package, V: Version, DP: DependencyProvider<P, V>> CachingDependencyProvider<P, V, DP> {
    pub fn new(remote_dependencies_provider: DP) -> Self {
        CachingDependencyProvider {
            remote_dependencies: remote_dependencies_provider,
            cached_dependencies: RefCell::new(OfflineDependencyProvider::new()),
        }
    }
}

impl<P: Package, V: Version, DP: DependencyProvider<P, V>> DependencyProvider<P, V>
    for CachingDependencyProvider<P, V, DP>
{
    fn choose_package_version<T: std::borrow::Borrow<P>, U: std::borrow::Borrow<Range<V>>>(
        &self,
        packages: impl Iterator<Item = (T, U)>,
    ) -> Result<(T, Option<V>), Box<dyn Error>> {
        self.remote_dependencies.choose_package_version(packages)
    }

    // Caches dependencies if they were already queried
    fn get_dependencies(
        &self,
        package: &P,
        version: &V,
    ) -> Result<Dependencies<P, V>, Box<dyn Error>> {
        let mut cache = self.cached_dependencies.borrow_mut();
        match cache.get_dependencies(package, version) {
            Ok(Dependencies::Unknown) => {
                let dependencies = self.remote_dependencies.get_dependencies(package, version);
                match dependencies {
                    Ok(Dependencies::Known(dependencies)) => {
                        cache.add_dependencies(
                            package.clone(),
                            version.clone(),
                            dependencies.clone().into_iter(),
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
    let mut remote_dependencies_provider = OfflineDependencyProvider::<&str, NumberVersion>::new();

    // Add dependencies as needed. Here only root package is added.
    remote_dependencies_provider.add_dependencies("root", 1, Vec::new());

    let caching_dependencies_provider =
        CachingDependencyProvider::new(remote_dependencies_provider);

    let solution = resolve(&caching_dependencies_provider, "root", 1);
    println!("Solution: {:?}", solution);
}

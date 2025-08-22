use std::path::Path;

use either::Either;
use rustc_hash::FxHashMap;

use uv_cache::Refresh;
use uv_cache_info::Timestamp;
use uv_distribution_types::Requirement;
use uv_normalize::PackageName;

/// Whether to reinstall packages.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Reinstall {
    /// Don't reinstall any packages; respect the existing installation.
    #[default]
    None,

    /// Reinstall all packages in the plan.
    All,

    /// Reinstall only the specified packages.
    Packages(Vec<PackageName>, Vec<Box<Path>>),
}

impl Reinstall {
    /// Determine the reinstall strategy to use.
    pub fn from_args(reinstall: Option<bool>, reinstall_package: Vec<PackageName>) -> Option<Self> {
        match reinstall {
            Some(true) => Some(Self::All),
            Some(false) => Some(Self::None),
            None if reinstall_package.is_empty() => None,
            None => Some(Self::Packages(reinstall_package, Vec::new())),
        }
    }

    /// Returns `true` if no packages should be reinstalled.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Returns `true` if all packages should be reinstalled.
    pub fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }

    /// Returns `true` if the specified package should be reinstalled.
    pub fn contains_package(&self, package_name: &PackageName) -> bool {
        match self {
            Self::None => false,
            Self::All => true,
            Self::Packages(packages, ..) => packages.contains(package_name),
        }
    }

    /// Returns `true` if the specified path should be reinstalled.
    pub fn contains_path(&self, path: &Path) -> bool {
        match self {
            Self::None => false,
            Self::All => true,
            Self::Packages(.., paths) => paths
                .iter()
                .any(|target| same_file::is_same_file(path, target).unwrap_or(false)),
        }
    }

    /// Combine a set of [`Reinstall`] values.
    #[must_use]
    pub fn combine(self, other: Self) -> Self {
        match self {
            // Setting `--reinstall` or `--no-reinstall` should clear previous `--reinstall-package` selections.
            Self::All | Self::None => self,
            Self::Packages(self_packages, self_paths) => match other {
                // If `--reinstall` was enabled previously, `--reinstall-package` is subsumed by reinstalling all packages.
                Self::All => other,
                // If `--no-reinstall` was enabled previously, then `--reinstall-package` enables an explicit reinstall of those packages.
                Self::None => Self::Packages(self_packages, self_paths),
                // If `--reinstall-package` was included twice, combine the requirements.
                Self::Packages(other_packages, other_paths) => {
                    let mut combined_packages = self_packages;
                    combined_packages.extend(other_packages);
                    let mut combined_paths = self_paths;
                    combined_paths.extend(other_paths);
                    Self::Packages(combined_packages, combined_paths)
                }
            },
        }
    }

    /// Add a [`Box<Path>`] to the [`Reinstall`] policy.
    #[must_use]
    pub fn with_path(self, path: Box<Path>) -> Self {
        match self {
            Self::None => Self::Packages(vec![], vec![path]),
            Self::All => Self::All,
            Self::Packages(packages, mut paths) => {
                paths.push(path);
                Self::Packages(packages, paths)
            }
        }
    }

    /// Add a [`Package`] to the [`Reinstall`] policy.
    #[must_use]
    pub fn with_package(self, package_name: PackageName) -> Self {
        match self {
            Self::None => Self::Packages(vec![package_name], vec![]),
            Self::All => Self::All,
            Self::Packages(mut packages, paths) => {
                packages.push(package_name);
                Self::Packages(packages, paths)
            }
        }
    }

    /// Create a [`Reinstall`] strategy to reinstall a single package.
    pub fn package(package_name: PackageName) -> Self {
        Self::Packages(vec![package_name], vec![])
    }
}

/// Create a [`Refresh`] policy by integrating the [`Reinstall`] policy.
impl From<Reinstall> for Refresh {
    fn from(value: Reinstall) -> Self {
        match value {
            Reinstall::None => Self::None(Timestamp::now()),
            Reinstall::All => Self::All(Timestamp::now()),
            Reinstall::Packages(packages, paths) => {
                Self::Packages(packages, paths, Timestamp::now())
            }
        }
    }
}

/// Whether to allow package upgrades.
#[derive(Debug, Default, Clone)]
pub enum Upgrade {
    /// Prefer pinned versions from the existing lockfile, if possible.
    #[default]
    None,

    /// Allow package upgrades for all packages, ignoring the existing lockfile.
    All,

    /// Allow package upgrades, but only for the specified packages.
    Packages(FxHashMap<PackageName, Vec<Requirement>>),
}

impl Upgrade {
    /// Determine the upgrade selection strategy from the command-line arguments.
    pub fn from_args(upgrade: Option<bool>, upgrade_package: Vec<Requirement>) -> Option<Self> {
        match upgrade {
            Some(true) => Some(Self::All),
            // TODO(charlie): `--no-upgrade` with `--upgrade-package` should allow the specified
            // packages to be upgraded. Right now, `--upgrade-package` is silently ignored.
            Some(false) => Some(Self::None),
            None if upgrade_package.is_empty() => None,
            None => Some(Self::Packages(upgrade_package.into_iter().fold(
                FxHashMap::default(),
                |mut map, requirement| {
                    map.entry(requirement.name.clone())
                        .or_default()
                        .push(requirement);
                    map
                },
            ))),
        }
    }

    /// Create an [`Upgrade`] strategy to upgrade a single package.
    pub fn package(package_name: PackageName) -> Self {
        Self::Packages({
            let mut map = FxHashMap::default();
            map.insert(package_name, vec![]);
            map
        })
    }

    /// Returns `true` if no packages should be upgraded.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Returns `true` if all packages should be upgraded.
    pub fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }

    /// Returns `true` if the specified package should be upgraded.
    pub fn contains(&self, package_name: &PackageName) -> bool {
        match self {
            Self::None => false,
            Self::All => true,
            Self::Packages(packages) => packages.contains_key(package_name),
        }
    }

    /// Returns an iterator over the constraints.
    ///
    /// When upgrading, users can provide bounds on the upgrade (e.g., `--upgrade-package flask<3`).
    pub fn constraints(&self) -> impl Iterator<Item = &Requirement> {
        if let Self::Packages(packages) = self {
            Either::Right(
                packages
                    .values()
                    .flat_map(|requirements| requirements.iter()),
            )
        } else {
            Either::Left(std::iter::empty())
        }
    }

    /// Combine a set of [`Upgrade`] values.
    #[must_use]
    pub fn combine(self, other: Self) -> Self {
        match self {
            // Setting `--upgrade` or `--no-upgrade` should clear previous `--upgrade-package` selections.
            Self::All | Self::None => self,
            Self::Packages(self_packages) => match other {
                // If `--upgrade` was enabled previously, `--upgrade-package` is subsumed by upgrading all packages.
                Self::All => other,
                // If `--no-upgrade` was enabled previously, then `--upgrade-package` enables an explicit upgrade of those packages.
                Self::None => Self::Packages(self_packages),
                // If `--upgrade-package` was included twice, combine the requirements.
                Self::Packages(other_packages) => {
                    let mut combined = self_packages;
                    for (package, requirements) in other_packages {
                        combined.entry(package).or_default().extend(requirements);
                    }
                    Self::Packages(combined)
                }
            },
        }
    }
}

/// Create a [`Refresh`] policy by integrating the [`Upgrade`] policy.
impl From<Upgrade> for Refresh {
    fn from(value: Upgrade) -> Self {
        match value {
            Upgrade::None => Self::None(Timestamp::now()),
            Upgrade::All => Self::All(Timestamp::now()),
            Upgrade::Packages(packages) => Self::Packages(
                packages.into_keys().collect::<Vec<_>>(),
                Vec::new(),
                Timestamp::now(),
            ),
        }
    }
}

/// Whether to isolate builds.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum BuildIsolation {
    /// Isolate all builds.
    #[default]
    Isolate,

    /// Do not isolate any builds.
    Shared,

    /// Do not isolate builds for the specified packages.
    SharedPackage(Vec<PackageName>),
}

impl BuildIsolation {
    /// Determine the build isolation strategy from the command-line arguments.
    pub fn from_args(
        no_build_isolation: Option<bool>,
        no_build_isolation_package: Vec<PackageName>,
    ) -> Option<Self> {
        match no_build_isolation {
            Some(true) => Some(Self::Shared),
            Some(false) => Some(Self::Isolate),
            None if no_build_isolation_package.is_empty() => None,
            None => Some(Self::SharedPackage(no_build_isolation_package)),
        }
    }

    /// Combine a set of [`BuildIsolation`] values.
    #[must_use]
    pub fn combine(self, other: Self) -> Self {
        match self {
            // Setting `--build-isolation` or `--no-build-isolation` should clear previous `--no-build-isolation-package` selections.
            Self::Isolate | Self::Shared => self,
            Self::SharedPackage(self_packages) => match other {
                // If `--no-build-isolation` was enabled previously, `--no-build-isolation-package` is subsumed by sharing all builds.
                Self::Shared => other,
                // If `--build-isolation` was enabled previously, then `--no-build-isolation-package` enables specific packages to be shared.
                Self::Isolate => Self::SharedPackage(self_packages),
                // If `--no-build-isolation-package` was included twice, combine the packages.
                Self::SharedPackage(other_packages) => {
                    let mut combined = self_packages;
                    combined.extend(other_packages);
                    Self::SharedPackage(combined)
                }
            },
        }
    }
}

use either::Either;
use std::path::{Path, PathBuf};
use uv_pep508::PackageName;

use rustc_hash::FxHashMap;
use uv_cache::Refresh;
use uv_cache_info::Timestamp;
use uv_pypi_types::Requirement;

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
    Packages(Vec<PackageName>, Vec<PathBuf>),
}

impl Reinstall {
    /// Determine the reinstall strategy to use.
    pub fn from_args(reinstall: Option<bool>, reinstall_package: Vec<PackageName>) -> Self {
        match reinstall {
            Some(true) => Self::All,
            Some(false) => Self::None,
            None => {
                if reinstall_package.is_empty() {
                    Self::None
                } else {
                    Self::Packages(reinstall_package, Vec::new())
                }
            }
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
        match &self {
            Self::None => false,
            Self::All => true,
            Self::Packages(packages, ..) => packages.contains(package_name),
        }
    }

    /// Returns `true` if the specified path should be reinstalled.
    pub fn contains_path(&self, path: &Path) -> bool {
        match &self {
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
        match (self, other) {
            // If both are `None`, the result is `None`.
            (Self::None, Self::None) => Self::None,
            // If either is `All`, the result is `All`.
            (Self::All, _) | (_, Self::All) => Self::All,
            // If one is `None`, the result is the other.
            (Self::Packages(a1, a2), Self::None) => Self::Packages(a1, a2),
            (Self::None, Self::Packages(b1, b2)) => Self::Packages(b1, b2),
            // If both are `Packages`, the result is the union of the two.
            (Self::Packages(mut a1, mut a2), Self::Packages(b1, b2)) => {
                a1.extend(b1);
                a2.extend(b2);
                Self::Packages(a1, a2)
            }
        }
    }

    /// Add a [`PathBuf`] to the [`Reinstall`] policy.
    #[must_use]
    pub fn with_path(self, path: PathBuf) -> Self {
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
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
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
    /// Determine the [`Upgrade`] strategy from the command-line arguments.
    pub fn from_args(upgrade: Option<bool>, upgrade_package: Vec<Requirement>) -> Self {
        match upgrade {
            Some(true) => Self::All,
            Some(false) => Self::None,
            None => {
                if upgrade_package.is_empty() {
                    Self::None
                } else {
                    Self::Packages(upgrade_package.into_iter().fold(
                        FxHashMap::default(),
                        |mut map, requirement| {
                            map.entry(requirement.name.clone())
                                .or_default()
                                .push(requirement);
                            map
                        },
                    ))
                }
            }
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
        match &self {
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
        match (self, other) {
            // If both are `None`, the result is `None`.
            (Self::None, Self::None) => Self::None,
            // If either is `All`, the result is `All`.
            (Self::All, _) | (_, Self::All) => Self::All,
            // If one is `None`, the result is the other.
            (Self::Packages(a), Self::None) => Self::Packages(a),
            (Self::None, Self::Packages(b)) => Self::Packages(b),
            // If both are `Packages`, the result is the union of the two.
            (Self::Packages(mut a), Self::Packages(b)) => {
                a.extend(b);
                Self::Packages(a)
            }
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

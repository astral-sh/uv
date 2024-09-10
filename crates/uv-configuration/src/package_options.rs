use either::Either;
use pep508_rs::PackageName;

use pypi_types::Requirement;
use rustc_hash::FxHashMap;
use uv_cache::Refresh;
use uv_cache_info::Timestamp;

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
    Packages(Vec<PackageName>),
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
                    Self::Packages(reinstall_package)
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
}

/// Create a [`Refresh`] policy by integrating the [`Reinstall`] policy.
impl From<Reinstall> for Refresh {
    fn from(value: Reinstall) -> Self {
        match value {
            Reinstall::None => Self::None(Timestamp::now()),
            Reinstall::All => Self::All(Timestamp::now()),
            Reinstall::Packages(packages) => Self::Packages(packages, Timestamp::now()),
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
            Upgrade::Packages(packages) => {
                Self::Packages(packages.into_keys().collect::<Vec<_>>(), Timestamp::now())
            }
        }
    }
}

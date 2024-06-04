use pep508_rs::PackageName;

use rustc_hash::FxHashSet;

/// Whether to reinstall packages.
#[derive(Debug, Default, Clone)]
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

/// Whether to allow package upgrades.
#[derive(Debug, Default, Clone)]
pub enum Upgrade {
    /// Prefer pinned versions from the existing lockfile, if possible.
    #[default]
    None,

    /// Allow package upgrades for all packages, ignoring the existing lockfile.
    All,

    /// Allow package upgrades, but only for the specified packages.
    Packages(FxHashSet<PackageName>),
}

impl Upgrade {
    /// Determine the upgrade strategy from the command-line arguments.
    pub fn from_args(upgrade: Option<bool>, upgrade_package: Vec<PackageName>) -> Self {
        match upgrade {
            Some(true) => Self::All,
            Some(false) => Self::None,
            None => {
                if upgrade_package.is_empty() {
                    Self::None
                } else {
                    Self::Packages(upgrade_package.into_iter().collect())
                }
            }
        }
    }

    /// Returns `true` if no packages should be upgraded.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Returns `true` if all packages should be upgraded.
    pub fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }
}

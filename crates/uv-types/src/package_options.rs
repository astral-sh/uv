use pep508_rs::PackageName;
use rustc_hash::FxHashSet;

/// Whether to reinstall packages.
#[derive(Debug, Clone)]
pub enum Reinstall {
    /// Don't reinstall any packages; respect the existing installation.
    None,

    /// Reinstall all packages in the plan.
    All,

    /// Reinstall only the specified packages.
    Packages(Vec<PackageName>),
}

impl Reinstall {
    /// Determine the reinstall strategy to use.
    pub fn from_args(reinstall: bool, reinstall_package: Vec<PackageName>) -> Self {
        if reinstall {
            Self::All
        } else if !reinstall_package.is_empty() {
            Self::Packages(reinstall_package)
        } else {
            Self::None
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
#[derive(Debug)]
pub enum Upgrade {
    /// Prefer pinned versions from the existing lockfile, if possible.
    None,

    /// Allow package upgrades for all packages, ignoring the existing lockfile.
    All,

    /// Allow package upgrades, but only for the specified packages.
    Packages(FxHashSet<PackageName>),
}

impl Upgrade {
    /// Determine the upgrade strategy from the command-line arguments.
    pub fn from_args(upgrade: bool, upgrade_package: Vec<PackageName>) -> Self {
        if upgrade {
            Self::All
        } else if !upgrade_package.is_empty() {
            Self::Packages(upgrade_package.into_iter().collect())
        } else {
            Self::None
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

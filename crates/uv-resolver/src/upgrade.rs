use rustc_hash::FxHashSet;

use uv_configuration::Upgrade;
use uv_normalize::PackageName;

/// The resolved set of packages that should be upgraded.
///
/// This combines `--upgrade` and `--upgrade-package` into a single `contains` check.
#[derive(Debug, Default, Clone)]
pub struct UpgradePackages {
    /// Whether all packages should be upgraded.
    all: bool,
    /// The specific packages to upgrade.
    packages: FxHashSet<PackageName>,
}

impl UpgradePackages {
    /// Create an [`UpgradePackages`] for non-project commands (e.g., `pip compile`, `pip install`)
    /// where dependency groups are not supported.
    pub fn for_non_project(upgrade: &Upgrade) -> Self {
        match (upgrade.is_all(), upgrade.packages()) {
            (true, _) => Self {
                all: true,
                packages: FxHashSet::default(),
            },
            (false, Some(packages)) => Self {
                all: false,
                packages: packages.clone(),
            },
            (false, None) => Self::default(),
        }
    }

    /// Returns `true` if the given package should be upgraded.
    pub fn contains(&self, package_name: &PackageName) -> bool {
        self.all || self.packages.contains(package_name)
    }
}

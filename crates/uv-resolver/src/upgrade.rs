use rustc_hash::FxHashSet;

use uv_configuration::Upgrade;
use uv_normalize::PackageName;

use crate::Lock;

/// The resolved set of packages that should be upgraded.
///
/// This combines explicitly named packages (from `--upgrade-package`) with packages belonging to
/// upgraded dependency groups (from `--upgrade-group`), providing a single `contains` check that
/// accounts for both.
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

    /// Create an [`UpgradePackages`] for workspace/project commands, combining explicitly named
    /// packages with packages resolved from dependency groups in the lockfile.
    pub fn for_workspace(lock: &Lock, upgrade: &Upgrade) -> Self {
        match (upgrade.is_all(), upgrade.packages()) {
            (true, _) => Self {
                all: true,
                packages: FxHashSet::default(),
            },
            (false, Some(packages)) => {
                let mut combined = packages.clone();

                if let Some(groups) = upgrade.groups() {
                    // Check package-level dependency groups (the standard case for projects with
                    // a `[project]` table).
                    for package in lock.packages() {
                        for (group_name, dependencies) in package.resolved_dependency_groups() {
                            if groups.contains(group_name) {
                                for dependency in dependencies {
                                    combined.insert(dependency.package_name().clone());
                                }
                            }
                        }
                    }

                    // Check manifest-level dependency groups, which cover projects without a
                    // `[project]` table (e.g., virtual workspace roots or PEP 723 scripts).
                    for (group_name, requirements) in lock.dependency_groups() {
                        if groups.contains(group_name) {
                            for requirement in requirements {
                                combined.insert(requirement.name.clone());
                            }
                        }
                    }
                }

                Self {
                    all: false,
                    packages: combined,
                }
            }
            (false, None) => Self::default(),
        }
    }

    /// Returns `true` if the given package should be upgraded.
    pub fn contains(&self, package_name: &PackageName) -> bool {
        self.all || self.packages.contains(package_name)
    }
}

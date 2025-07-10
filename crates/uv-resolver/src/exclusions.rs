use uv_configuration::{Reinstall, Upgrade};
use uv_pep508::PackageName;

/// Tracks locally installed packages that should not be selected during resolution.
#[derive(Debug, Default, Clone)]
pub struct Exclusions {
    reinstall: Reinstall,
    upgrade: Upgrade,
}

impl Exclusions {
    pub fn new(reinstall: Reinstall, upgrade: Upgrade) -> Self {
        Self { reinstall, upgrade }
    }

    pub fn reinstall(&self, package: &PackageName) -> bool {
        self.reinstall.contains_package(package)
    }

    pub fn upgrade(&self, package: &PackageName) -> bool {
        self.upgrade.contains(package)
    }

    pub fn contains(&self, package: &PackageName) -> bool {
        self.reinstall(package) || self.upgrade(package)
    }
}

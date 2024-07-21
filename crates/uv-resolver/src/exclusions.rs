use pep508_rs::PackageName;
use uv_configuration::{Reinstall, Upgrade};
use uv_normalize::InternedSet;

/// Tracks locally installed packages that should not be selected during resolution.
#[derive(Debug, Default, Clone)]
pub enum Exclusions {
    #[default]
    None,
    /// Exclude some local packages from consideration, e.g. from `--reinstall-package foo --upgrade-package bar`
    Some(InternedSet<PackageName>),
    /// Exclude all local packages from consideration, e.g. from `--reinstall` or `--upgrade`
    All,
}

impl Exclusions {
    pub fn new(reinstall: Reinstall, upgrade: Upgrade) -> Self {
        if upgrade.is_all() || reinstall.is_all() {
            Self::All
        } else {
            let mut exclusions: InternedSet<PackageName> =
                if let Reinstall::Packages(packages) = reinstall {
                    InternedSet::from_iter(packages)
                } else {
                    InternedSet::default()
                };

            if let Upgrade::Packages(packages) = upgrade {
                exclusions.extend(packages.into_keys());
            };

            if exclusions.is_empty() {
                Self::None
            } else {
                Self::Some(exclusions)
            }
        }
    }

    /// Returns true if the package is excluded and a local distribution should not be used.
    pub fn contains(&self, package: &PackageName) -> bool {
        match self {
            Self::None => false,
            Self::Some(packages) => packages.contains(package),
            Self::All => true,
        }
    }
}

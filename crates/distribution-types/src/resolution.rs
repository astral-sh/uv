use rustc_hash::FxHashMap;

use pep508_rs::Requirement;
use uv_normalize::PackageName;

use crate::{BuiltDist, Dist, PathSourceDist, SourceDist};

/// A set of packages pinned at specific versions.
#[derive(Debug, Default, Clone)]
pub struct Resolution(FxHashMap<PackageName, Dist>);

impl Resolution {
    /// Create a new resolution from the given pinned packages.
    pub fn new(packages: FxHashMap<PackageName, Dist>) -> Self {
        Self(packages)
    }

    /// Return the distribution for the given package name, if it exists.
    pub fn get(&self, package_name: &PackageName) -> Option<&Dist> {
        self.0.get(package_name)
    }

    /// Iterate over the [`PackageName`] entities in this resolution.
    pub fn packages(&self) -> impl Iterator<Item = &PackageName> {
        self.0.keys()
    }

    /// Iterate over the [`Dist`] entities in this resolution.
    pub fn distributions(&self) -> impl Iterator<Item = &Dist> {
        self.0.values()
    }

    /// Iterate over the [`Dist`] entities in this resolution.
    pub fn into_distributions(self) -> impl Iterator<Item = Dist> {
        self.0.into_values()
    }

    /// Return the number of distributions in this resolution.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Return `true` if there are no pinned packages in this resolution.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Return the set of [`Requirement`]s that this resolution represents, exclusive of any
    /// editable requirements.
    pub fn requirements(&self) -> Vec<Requirement> {
        let mut requirements = self
            .0
            .values()
            .filter_map(|dist| match dist {
                Dist::Source(SourceDist::Path(PathSourceDist { editable: true, .. })) => None,
                dist => Some(Requirement::from(dist.clone())),
            })
            .collect::<Vec<_>>();
        requirements.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        requirements
    }
}

impl From<Dist> for Requirement {
    fn from(dist: Dist) -> Self {
        match dist {
            Dist::Built(BuiltDist::Registry(wheel)) => Requirement {
                name: wheel.filename.name,
                extras: vec![],
                version_or_url: Some(pep508_rs::VersionOrUrl::VersionSpecifier(
                    pep440_rs::VersionSpecifiers::from(
                        pep440_rs::VersionSpecifier::equals_version(wheel.filename.version),
                    ),
                )),
                marker: None,
            },
            Dist::Built(BuiltDist::DirectUrl(wheel)) => Requirement {
                name: wheel.filename.name,
                extras: vec![],
                version_or_url: Some(pep508_rs::VersionOrUrl::Url(wheel.url)),
                marker: None,
            },
            Dist::Built(BuiltDist::Path(wheel)) => Requirement {
                name: wheel.filename.name,
                extras: vec![],
                version_or_url: Some(pep508_rs::VersionOrUrl::Url(wheel.url)),
                marker: None,
            },
            Dist::Source(SourceDist::Registry(sdist)) => Requirement {
                name: sdist.filename.name,
                extras: vec![],
                version_or_url: Some(pep508_rs::VersionOrUrl::VersionSpecifier(
                    pep440_rs::VersionSpecifiers::from(
                        pep440_rs::VersionSpecifier::equals_version(sdist.filename.version),
                    ),
                )),
                marker: None,
            },
            Dist::Source(SourceDist::DirectUrl(sdist)) => Requirement {
                name: sdist.name,
                extras: vec![],
                version_or_url: Some(pep508_rs::VersionOrUrl::Url(sdist.url)),
                marker: None,
            },
            Dist::Source(SourceDist::Git(sdist)) => Requirement {
                name: sdist.name,
                extras: vec![],
                version_or_url: Some(pep508_rs::VersionOrUrl::Url(sdist.url)),
                marker: None,
            },
            Dist::Source(SourceDist::Path(sdist)) => Requirement {
                name: sdist.name,
                extras: vec![],
                version_or_url: Some(pep508_rs::VersionOrUrl::Url(sdist.url)),
                marker: None,
            },
        }
    }
}

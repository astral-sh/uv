use rustc_hash::FxHashMap;

use uv_normalize::PackageName;

use crate::{
    BuiltDist, Dist, Name, ParsedGitUrl, Requirement, RequirementSource, ResolvedDist, SourceDist,
};

/// A set of packages pinned at specific versions.
#[derive(Debug, Default, Clone)]
pub struct Resolution(FxHashMap<PackageName, ResolvedDist>);

impl Resolution {
    /// Create a new resolution from the given pinned packages.
    pub fn new(packages: FxHashMap<PackageName, ResolvedDist>) -> Self {
        Self(packages)
    }

    /// Return the distribution for the given package name, if it exists.
    pub fn get(&self, package_name: &PackageName) -> Option<&ResolvedDist> {
        self.0.get(package_name)
    }

    /// Return the remote distribution for the given package name, if it exists.
    pub fn get_remote(&self, package_name: &PackageName) -> Option<&Dist> {
        match self.0.get(package_name) {
            Some(dist) => match dist {
                ResolvedDist::Installable(dist) => Some(dist),
                ResolvedDist::Installed(_) => None,
            },
            None => None,
        }
    }

    /// Iterate over the [`PackageName`] entities in this resolution.
    pub fn packages(&self) -> impl Iterator<Item = &PackageName> {
        self.0.keys()
    }

    /// Iterate over the [`ResolvedDist`] entities in this resolution.
    pub fn distributions(&self) -> impl Iterator<Item = &ResolvedDist> {
        self.0.values()
    }

    /// Iterate over the [`ResolvedDist`] entities in this resolution.
    pub fn into_distributions(self) -> impl Iterator<Item = ResolvedDist> {
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
        let mut requirements: Vec<_> = self
            .0
            .values()
            // Remove editable requirements
            .filter(|dist| !dist.is_editable())
            .map(Requirement::from)
            .collect();
        requirements.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        requirements
    }
}

impl From<&ResolvedDist> for Requirement {
    fn from(resolved_dist: &ResolvedDist) -> Self {
        let source = match resolved_dist {
            ResolvedDist::Installable(dist) => match dist {
                Dist::Built(BuiltDist::Registry(wheel)) => RequirementSource::Registry {
                    specifier: pep440_rs::VersionSpecifiers::from(
                        pep440_rs::VersionSpecifier::equals_version(wheel.filename.version.clone()),
                    ),
                    index: None,
                },
                Dist::Built(BuiltDist::DirectUrl(wheel)) => {
                    let mut location = wheel.url.to_url();
                    location.set_fragment(None);
                    RequirementSource::Url {
                        url: wheel.url.clone(),
                        location,
                        subdirectory: None,
                    }
                }
                Dist::Built(BuiltDist::Path(wheel)) => RequirementSource::Path {
                    path: wheel.path.clone(),
                    url: wheel.url.clone(),
                    editable: None,
                },
                Dist::Source(SourceDist::Registry(sdist)) => RequirementSource::Registry {
                    specifier: pep440_rs::VersionSpecifiers::from(
                        pep440_rs::VersionSpecifier::equals_version(sdist.filename.version.clone()),
                    ),
                    index: None,
                },
                Dist::Source(SourceDist::DirectUrl(sdist)) => {
                    let mut location = sdist.url.to_url();
                    location.set_fragment(None);
                    RequirementSource::Url {
                        url: sdist.url.clone(),
                        location,
                        subdirectory: None,
                    }
                }
                Dist::Source(SourceDist::Git(sdist)) => {
                    let git_url = ParsedGitUrl::try_from(sdist.url.to_url())
                        .expect("urls must be valid at this point");
                    RequirementSource::Git {
                        url: sdist.url.clone(),
                        repository: git_url.url.repository().clone(),
                        reference: git_url.url.reference().clone(),
                        subdirectory: git_url.subdirectory,
                    }
                }
                Dist::Source(SourceDist::Path(sdist)) => RequirementSource::Path {
                    path: sdist.path.clone(),
                    url: sdist.url.clone(),
                    editable: None,
                },
            },
            ResolvedDist::Installed(dist) => RequirementSource::Registry {
                specifier: pep440_rs::VersionSpecifiers::from(
                    pep440_rs::VersionSpecifier::equals_version(dist.version().clone()),
                ),
                index: None,
            },
        };
        Requirement {
            name: resolved_dist.name().clone(),
            extras: vec![],
            marker: None,
            source,
        }
    }
}

use rustc_hash::FxHashMap;
use url::Url;

use uv_git::GitUrl;
use uv_normalize::PackageName;

use crate::{BuiltDist, Dist, Name, ResolvedDist, SourceDist, UvRequirement, UvSource};

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

    /// Return the set of [`UvRequirement`]s that this resolution represents, exclusive of any
    /// editable requirements.
    pub fn requirements(&self) -> Vec<UvRequirement> {
        let mut requirements = self
            .0
            .values()
            // Remove editable requirements
            .filter(|dist| !dist.is_editable())
            .map(dist_to_uv_requirement)
            .collect::<Vec<_>>();
        requirements.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        requirements
    }
}

fn dist_to_uv_requirement(resolved_dist: &ResolvedDist) -> UvRequirement {
    match resolved_dist {
        ResolvedDist::Installable(dist) => match dist {
            Dist::Built(BuiltDist::Registry(wheel)) => UvRequirement {
                name: wheel.filename.name.clone(),
                extras: vec![],
                source: UvSource::Registry {
                    version: pep440_rs::VersionSpecifiers::from(
                        pep440_rs::VersionSpecifier::equals_version(wheel.filename.version.clone()),
                    ),
                    index: None,
                },
                marker: None,
            },

            Dist::Built(BuiltDist::DirectUrl(wheel)) => UvRequirement {
                name: wheel.filename.name.clone(),
                extras: vec![],
                source: UvSource::Url {
                    url: wheel.url.clone(),
                    subdirectory: None,
                },
                marker: None,
            },
            Dist::Built(BuiltDist::Path(wheel)) => UvRequirement {
                name: wheel.filename.name.clone(),
                extras: vec![],
                source: UvSource::Path {
                    path: wheel.path.clone(),
                    url: wheel.url.clone(),
                    editable: None,
                },
                marker: None,
            },
            Dist::Source(SourceDist::Registry(sdist)) => UvRequirement {
                name: sdist.filename.name.clone(),
                extras: vec![],
                source: UvSource::Registry {
                    version: pep440_rs::VersionSpecifiers::from(
                        pep440_rs::VersionSpecifier::equals_version(sdist.filename.version.clone()),
                    ),
                    index: None,
                },
                marker: None,
            },
            Dist::Source(SourceDist::DirectUrl(sdist)) => UvRequirement {
                name: sdist.name.clone(),
                extras: vec![],
                source: UvSource::Url {
                    url: sdist.url.clone(),
                    subdirectory: None,
                },
                marker: None,
            },
            Dist::Source(SourceDist::Git(sdist)) => {
                // TODO(konsti)
                let url = sdist
                    .url
                    .as_str()
                    .strip_prefix("git+")
                    .expect("Missing git+ prefix for Git URL");
                let url = Url::parse(url).expect("TODO(konsti)");
                // TODO(konsti)
                let subdirectory = url.fragment().and_then(|fragment| {
                    fragment
                        .split('&')
                        .find_map(|fragment| fragment.strip_prefix("subdirectory="))
                        .map(ToString::to_string)
                });
                let git_url = GitUrl::try_from(url).expect("TODO(konsti)");

                UvRequirement {
                    name: sdist.name.clone(),
                    extras: vec![],
                    source: UvSource::Git {
                        url: sdist.url.clone(),
                        repository: git_url.repository().clone(),
                        reference: git_url.reference().clone(),
                        subdirectory,
                    },
                    marker: None,
                }
            }
            Dist::Source(SourceDist::Path(sdist)) => UvRequirement {
                name: sdist.name.clone(),
                extras: vec![],
                source: UvSource::Path {
                    path: sdist.path.clone(),
                    url: sdist.url.clone(),
                    editable: None,
                },
                marker: None,
            },
        },
        ResolvedDist::Installed(dist) => UvRequirement {
            name: dist.name().clone(),
            extras: vec![],
            source: UvSource::Registry {
                version: pep440_rs::VersionSpecifiers::from(
                    pep440_rs::VersionSpecifier::equals_version(dist.version().clone()),
                ),
                index: None,
            },
            marker: None,
        },
    }
}

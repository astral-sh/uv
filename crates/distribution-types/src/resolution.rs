use distribution_filename::DistExtension;
use pep508_rs::MarkerTree;
use pypi_types::{HashDigest, Requirement, RequirementSource};
use std::collections::BTreeMap;
use uv_normalize::{ExtraName, GroupName, PackageName};

use crate::{BuiltDist, Diagnostic, Dist, Name, ResolvedDist, SourceDist};

/// A set of packages pinned at specific versions.
#[derive(Debug, Default, Clone)]
pub struct Resolution {
    packages: BTreeMap<PackageName, ResolvedDist>,
    hashes: BTreeMap<PackageName, Vec<HashDigest>>,
    diagnostics: Vec<ResolutionDiagnostic>,
}

impl Resolution {
    /// Create a new resolution from the given pinned packages.
    pub fn new(
        packages: BTreeMap<PackageName, ResolvedDist>,
        hashes: BTreeMap<PackageName, Vec<HashDigest>>,
        diagnostics: Vec<ResolutionDiagnostic>,
    ) -> Self {
        Self {
            packages,
            hashes,
            diagnostics,
        }
    }

    /// Return the remote distribution for the given package name, if it exists.
    pub fn get_remote(&self, package_name: &PackageName) -> Option<&Dist> {
        match self.packages.get(package_name)? {
            ResolvedDist::Installable(dist) => Some(dist),
            ResolvedDist::Installed(_) => None,
        }
    }

    /// Return the hashes for the given package name, if they exist.
    pub fn get_hashes(&self, package_name: &PackageName) -> &[HashDigest] {
        self.hashes.get(package_name).map_or(&[], Vec::as_slice)
    }

    /// Iterate over the [`PackageName`] entities in this resolution.
    pub fn packages(&self) -> impl Iterator<Item = &PackageName> {
        self.packages.keys()
    }

    /// Iterate over the [`ResolvedDist`] entities in this resolution.
    pub fn distributions(&self) -> impl Iterator<Item = &ResolvedDist> {
        self.packages.values()
    }

    /// Return the number of distributions in this resolution.
    pub fn len(&self) -> usize {
        self.packages.len()
    }

    /// Return `true` if there are no pinned packages in this resolution.
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }

    /// Return the set of [`Requirement`]s that this resolution represents.
    pub fn requirements(&self) -> impl Iterator<Item = Requirement> + '_ {
        self.packages.values().map(Requirement::from)
    }

    /// Return the [`ResolutionDiagnostic`]s that were produced during resolution.
    pub fn diagnostics(&self) -> &[ResolutionDiagnostic] {
        &self.diagnostics
    }

    /// Filter the resolution to only include packages that match the given predicate.
    #[must_use]
    pub fn filter(self, predicate: impl Fn(&ResolvedDist) -> bool) -> Self {
        let packages = self
            .packages
            .into_iter()
            .filter(|(_, dist)| predicate(dist))
            .collect::<BTreeMap<_, _>>();
        let hashes = self
            .hashes
            .into_iter()
            .filter(|(name, _)| packages.contains_key(name))
            .collect();
        let diagnostics = self.diagnostics.clone();
        Self {
            packages,
            hashes,
            diagnostics,
        }
    }

    /// Map over the resolved distributions in this resolution.
    #[must_use]
    pub fn map(self, predicate: impl Fn(ResolvedDist) -> ResolvedDist) -> Self {
        let packages = self
            .packages
            .into_iter()
            .map(|(name, dist)| (name, predicate(dist)))
            .collect::<BTreeMap<_, _>>();
        let hashes = self
            .hashes
            .into_iter()
            .filter(|(name, _)| packages.contains_key(name))
            .collect();
        let diagnostics = self.diagnostics.clone();
        Self {
            packages,
            hashes,
            diagnostics,
        }
    }
}

#[derive(Debug, Clone, Hash)]
pub enum ResolutionDiagnostic {
    MissingExtra {
        /// The distribution that was requested with a non-existent extra. For example,
        /// `black==23.10.0`.
        dist: ResolvedDist,
        /// The extra that was requested. For example, `colorama` in `black[colorama]`.
        extra: ExtraName,
    },
    MissingDev {
        /// The distribution that was requested with a non-existent development dependency group.
        dist: ResolvedDist,
        /// The development dependency group that was requested.
        dev: GroupName,
    },
    YankedVersion {
        /// The package that was requested with a yanked version. For example, `black==23.10.0`.
        dist: ResolvedDist,
        /// The reason that the version was yanked, if any.
        reason: Option<String>,
    },
    MissingLowerBound {
        /// The name of the package that had no lower bound from any other package in the
        /// resolution. For example, `black`.
        package_name: PackageName,
    },
}

impl Diagnostic for ResolutionDiagnostic {
    /// Convert the diagnostic into a user-facing message.
    fn message(&self) -> String {
        match self {
            Self::MissingExtra { dist, extra } => {
                format!("The package `{dist}` does not have an extra named `{extra}`")
            }
            Self::MissingDev { dist, dev } => {
                format!("The package `{dist}` does not have a development dependency group named `{dev}`")
            }
            Self::YankedVersion { dist, reason } => {
                if let Some(reason) = reason {
                    format!("`{dist}` is yanked (reason: \"{reason}\")")
                } else {
                    format!("`{dist}` is yanked")
                }
            }
            Self::MissingLowerBound { package_name: name } => {
                format!(
                    "The transitive dependency `{name}` is unpinned. \
                    Consider setting a lower bound with a constraint when using \
                    `--resolution-strategy lowest` to avoid using outdated versions."
                )
            }
        }
    }

    /// Returns `true` if the [`PackageName`] is involved in this diagnostic.
    fn includes(&self, name: &PackageName) -> bool {
        match self {
            Self::MissingExtra { dist, .. } => name == dist.name(),
            Self::MissingDev { dist, .. } => name == dist.name(),
            Self::YankedVersion { dist, .. } => name == dist.name(),
            Self::MissingLowerBound { package_name } => name == package_name,
        }
    }
}

impl From<&ResolvedDist> for Requirement {
    fn from(resolved_dist: &ResolvedDist) -> Self {
        let source = match resolved_dist {
            ResolvedDist::Installable(dist) => match dist {
                Dist::Built(BuiltDist::Registry(wheels)) => RequirementSource::Registry {
                    specifier: pep440_rs::VersionSpecifiers::from(
                        pep440_rs::VersionSpecifier::equals_version(
                            wheels.best_wheel().filename.version.clone(),
                        ),
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
                        ext: DistExtension::Wheel,
                    }
                }
                Dist::Built(BuiltDist::Path(wheel)) => RequirementSource::Path {
                    install_path: wheel.install_path.clone(),
                    url: wheel.url.clone(),
                    ext: DistExtension::Wheel,
                },
                Dist::Source(SourceDist::Registry(sdist)) => RequirementSource::Registry {
                    specifier: pep440_rs::VersionSpecifiers::from(
                        pep440_rs::VersionSpecifier::equals_version(sdist.version.clone()),
                    ),
                    index: None,
                },
                Dist::Source(SourceDist::DirectUrl(sdist)) => {
                    let mut location = sdist.url.to_url();
                    location.set_fragment(None);
                    RequirementSource::Url {
                        url: sdist.url.clone(),
                        location,
                        subdirectory: sdist.subdirectory.clone(),
                        ext: DistExtension::Source(sdist.ext),
                    }
                }
                Dist::Source(SourceDist::Git(sdist)) => RequirementSource::Git {
                    url: sdist.url.clone(),
                    repository: sdist.git.repository().clone(),
                    reference: sdist.git.reference().clone(),
                    precise: sdist.git.precise(),
                    subdirectory: sdist.subdirectory.clone(),
                },
                Dist::Source(SourceDist::Path(sdist)) => RequirementSource::Path {
                    install_path: sdist.install_path.clone(),
                    url: sdist.url.clone(),
                    ext: DistExtension::Source(sdist.ext),
                },
                Dist::Source(SourceDist::Directory(sdist)) => RequirementSource::Directory {
                    install_path: sdist.install_path.clone(),
                    url: sdist.url.clone(),
                    editable: sdist.editable,
                    r#virtual: sdist.r#virtual,
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
            marker: MarkerTree::TRUE,
            source,
            origin: None,
        }
    }
}

use std::collections::BTreeMap;

use pypi_types::{Requirement, RequirementSource};
use uv_normalize::{ExtraName, GroupName, PackageName};

use crate::{BuiltDist, Diagnostic, Dist, Name, ResolvedDist, SourceDist};

/// A set of packages pinned at specific versions.
#[derive(Debug, Default, Clone)]
pub struct Resolution {
    packages: BTreeMap<PackageName, ResolvedDist>,
    diagnostics: Vec<ResolutionDiagnostic>,
}

impl Resolution {
    /// Create a new resolution from the given pinned packages.
    pub fn new(
        packages: BTreeMap<PackageName, ResolvedDist>,
        diagnostics: Vec<ResolutionDiagnostic>,
    ) -> Self {
        Self {
            packages,
            diagnostics,
        }
    }

    /// Return the remote distribution for the given package name, if it exists.
    pub fn get_remote(&self, package_name: &PackageName) -> Option<&Dist> {
        match self.packages.get(package_name) {
            Some(dist) => match dist {
                ResolvedDist::Installable(dist) => Some(dist),
                ResolvedDist::Installed(_) => None,
            },
            None => None,
        }
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
}

#[derive(Debug, Clone)]
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
}

impl Diagnostic for ResolutionDiagnostic {
    /// Convert the diagnostic into a user-facing message.
    fn message(&self) -> String {
        match self {
            Self::MissingExtra { dist, extra } => {
                format!("The package `{dist}` does not have an extra named `{extra}`.")
            }
            Self::MissingDev { dist, dev } => {
                format!("The package `{dist}` does not have a development dependency group named `{dev}`.")
            }
            Self::YankedVersion { dist, reason } => {
                if let Some(reason) = reason {
                    format!("`{dist}` is yanked (reason: \"{reason}\").")
                } else {
                    format!("`{dist}` is yanked.")
                }
            }
        }
    }

    /// Returns `true` if the [`PackageName`] is involved in this diagnostic.
    fn includes(&self, name: &PackageName) -> bool {
        match self {
            Self::MissingExtra { dist, .. } => name == dist.name(),
            Self::MissingDev { dist, .. } => name == dist.name(),
            Self::YankedVersion { dist, .. } => name == dist.name(),
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
                    }
                }
                Dist::Built(BuiltDist::Path(wheel)) => RequirementSource::Path {
                    path: wheel.path.clone(),
                    url: wheel.url.clone(),
                    editable: false,
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
                    path: sdist.path.clone(),
                    url: sdist.url.clone(),
                    editable: false,
                },
                Dist::Source(SourceDist::Directory(sdist)) => RequirementSource::Path {
                    path: sdist.path.clone(),
                    url: sdist.url.clone(),
                    editable: sdist.editable,
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
            origin: None,
        }
    }
}

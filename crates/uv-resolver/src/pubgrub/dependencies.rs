use std::iter;

use itertools::Itertools;
use pubgrub::range::Range;
use tracing::warn;

use pep440_rs::{Version, VersionSpecifiers};
use pypi_types::{
    ParsedArchiveUrl, ParsedDirectoryUrl, ParsedGitUrl, ParsedPathUrl, ParsedUrl, Requirement,
    RequirementSource, VerbatimParsedUrl,
};
use uv_normalize::{ExtraName, PackageName};

use crate::pubgrub::{PubGrubPackage, PubGrubPackageInner};
use crate::resolver::ForkLocals;
use crate::{PubGrubSpecifier, ResolveError};

#[derive(Clone, Debug)]
pub(crate) struct PubGrubDependency {
    pub(crate) package: PubGrubPackage,
    pub(crate) version: Range<Version>,
    /// This field is set if the [`Requirement`] had a URL. We still use a URL from [`Urls`]
    /// even if this field is None where there is an override with a URL or there is a different
    /// requirement or constraint for the same package that has a URL.
    pub(crate) url: Option<VerbatimParsedUrl>,
    /// The local version for this requirement, if specified.
    pub(crate) local: Option<Version>,
}

impl PubGrubDependency {
    pub(crate) fn from_requirement<'a>(
        requirement: &'a Requirement,
        source_name: Option<&'a PackageName>,
        fork_locals: &'a ForkLocals,
    ) -> impl Iterator<Item = Result<Self, ResolveError>> + 'a {
        // Add the package, plus any extra variants.
        iter::once(None)
            .chain(requirement.extras.clone().into_iter().map(Some))
            .map(|extra| PubGrubRequirement::from_requirement(requirement, extra, fork_locals))
            .filter_map_ok(move |requirement| {
                let PubGrubRequirement {
                    package,
                    version,
                    url,
                    local,
                } = requirement;
                match &*package {
                    PubGrubPackageInner::Package { name, .. } => {
                        // Detect self-dependencies.
                        if source_name.is_some_and(|source_name| source_name == name) {
                            warn!("{name} has a dependency on itself");
                            return None;
                        }

                        Some(PubGrubDependency {
                            package: package.clone(),
                            version: version.clone(),
                            url,
                            local,
                        })
                    }
                    PubGrubPackageInner::Marker { .. } => Some(PubGrubDependency {
                        package: package.clone(),
                        version: version.clone(),
                        url,
                        local,
                    }),
                    PubGrubPackageInner::Extra { name, .. } => {
                        debug_assert!(
                            !source_name.is_some_and(|source_name| source_name == name),
                            "extras not flattened for {name}"
                        );
                        Some(PubGrubDependency {
                            package: package.clone(),
                            version: version.clone(),
                            url: None,
                            local: None,
                        })
                    }
                    _ => None,
                }
            })
    }
}

/// A PubGrub-compatible package and version range.
#[derive(Debug, Clone)]
pub(crate) struct PubGrubRequirement {
    pub(crate) package: PubGrubPackage,
    pub(crate) version: Range<Version>,
    pub(crate) url: Option<VerbatimParsedUrl>,
    pub(crate) local: Option<Version>,
}

impl PubGrubRequirement {
    /// Convert a [`Requirement`] to a PubGrub-compatible package and range, while returning the URL
    /// on the [`Requirement`], if any.
    pub(crate) fn from_requirement(
        requirement: &Requirement,
        extra: Option<ExtraName>,
        fork_locals: &ForkLocals,
    ) -> Result<Self, ResolveError> {
        let (verbatim_url, parsed_url) = match &requirement.source {
            RequirementSource::Registry { specifier, .. } => {
                return Self::from_registry_requirement(specifier, extra, requirement, fork_locals);
            }
            RequirementSource::Url {
                subdirectory,
                location,
                url,
            } => {
                let parsed_url = ParsedUrl::Archive(ParsedArchiveUrl::from_source(
                    location.clone(),
                    subdirectory.clone(),
                ));
                (url, parsed_url)
            }
            RequirementSource::Git {
                repository,
                reference,
                precise,
                url,
                subdirectory,
            } => {
                let parsed_url = ParsedUrl::Git(ParsedGitUrl::from_source(
                    repository.clone(),
                    reference.clone(),
                    *precise,
                    subdirectory.clone(),
                ));
                (url, parsed_url)
            }
            RequirementSource::Path {
                url,
                install_path,
                lock_path,
            } => {
                let parsed_url = ParsedUrl::Path(ParsedPathUrl::from_source(
                    install_path.clone(),
                    lock_path.clone(),
                    url.to_url(),
                ));
                (url, parsed_url)
            }
            RequirementSource::Directory {
                editable,
                url,
                install_path,
                lock_path,
            } => {
                let parsed_url = ParsedUrl::Directory(ParsedDirectoryUrl::from_source(
                    install_path.clone(),
                    lock_path.clone(),
                    *editable,
                    url.to_url(),
                ));
                (url, parsed_url)
            }
        };

        Ok(Self {
            package: PubGrubPackage::from_package(
                requirement.name.clone(),
                extra,
                requirement.marker.clone(),
            ),
            version: Range::full(),
            url: Some(VerbatimParsedUrl {
                parsed_url,
                verbatim: verbatim_url.clone(),
            }),
            local: None,
        })
    }

    fn from_registry_requirement(
        specifier: &VersionSpecifiers,
        extra: Option<ExtraName>,
        requirement: &Requirement,
        fork_locals: &ForkLocals,
    ) -> Result<PubGrubRequirement, ResolveError> {
        // If the specifier is an exact version and the user requested a local version for this
        // fork that's more precise than the specifier, use the local version instead.
        let version = if let Some(local) = fork_locals.get(&requirement.name) {
            specifier
                .iter()
                .map(|specifier| {
                    ForkLocals::map(local, specifier)
                        .map_err(ResolveError::InvalidVersion)
                        .and_then(|specifier| {
                            Ok(PubGrubSpecifier::from_pep440_specifier(&specifier)?)
                        })
                })
                .fold_ok(Range::full(), |range, specifier| {
                    range.intersection(&specifier.into())
                })?
        } else {
            PubGrubSpecifier::from_pep440_specifiers(specifier)?.into()
        };

        let requirement = Self {
            package: PubGrubPackage::from_package(
                requirement.name.clone(),
                extra,
                requirement.marker.clone(),
            ),
            version,
            url: None,
            local: None,
        };

        Ok(requirement)
    }
}

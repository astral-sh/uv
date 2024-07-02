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
use crate::resolver::Locals;
use crate::{PubGrubSpecifier, ResolveError};

#[derive(Clone, Debug)]
pub(crate) struct PubGrubDependency {
    pub(crate) package: PubGrubPackage,
    pub(crate) version: Range<Version>,
    /// This field is set if the [`Requirement`] had a URL. We still use a URL from [`Urls`]
    /// even if this field is None where there is an override with a URL or there is a different
    /// requirement or constraint for the same package that has a URL.
    pub(crate) url: Option<VerbatimParsedUrl>,
}

impl PubGrubDependency {
    pub(crate) fn from_requirement<'a>(
        requirement: &'a Requirement,
        source_name: Option<&'a PackageName>,
        locals: &'a Locals,
    ) -> impl Iterator<Item = Result<Self, ResolveError>> + 'a {
        // Add the package, plus any extra variants.
        iter::once(None)
            .chain(requirement.extras.clone().into_iter().map(Some))
            .map(|extra| PubGrubRequirement::from_requirement(requirement, extra, locals))
            .filter_map_ok(move |requirement| {
                let PubGrubRequirement {
                    package,
                    version,
                    url,
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
                        })
                    }
                    PubGrubPackageInner::Marker { .. } => Some(PubGrubDependency {
                        package: package.clone(),
                        version: version.clone(),
                        url,
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
}

impl PubGrubRequirement {
    /// Convert a [`Requirement`] to a PubGrub-compatible package and range, while returning the URL
    /// on the [`Requirement`], if any.
    pub(crate) fn from_requirement(
        requirement: &Requirement,
        extra: Option<ExtraName>,
        locals: &Locals,
    ) -> Result<Self, ResolveError> {
        let (verbatim_url, parsed_url) = match &requirement.source {
            RequirementSource::Registry { specifier, .. } => {
                return Self::from_registry_requirement(specifier, extra, requirement, locals);
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
        })
    }

    fn from_registry_requirement(
        specifier: &VersionSpecifiers,
        extra: Option<ExtraName>,
        requirement: &Requirement,
        locals: &Locals,
    ) -> Result<PubGrubRequirement, ResolveError> {
        // If the specifier is an exact version, and the user requested a local version that's
        // more precise than the specifier, use the local version instead.
        let version = if let Some(expected) = locals.get(&requirement.name) {
            specifier
                .iter()
                .map(|specifier| {
                    Locals::map(expected, specifier)
                        .map_err(ResolveError::InvalidVersion)
                        .and_then(|specifier| Ok(PubGrubSpecifier::try_from(&specifier)?))
                })
                .fold_ok(Range::full(), |range, specifier| {
                    range.intersection(&specifier.into())
                })?
        } else {
            PubGrubSpecifier::try_from(specifier)?.into()
        };

        let requirement = Self {
            package: PubGrubPackage::from_package(
                requirement.name.clone(),
                extra,
                requirement.marker.clone(),
            ),
            version,
            url: None,
        };
        Ok(requirement)
    }
}

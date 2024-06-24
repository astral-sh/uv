use itertools::Itertools;
use pubgrub::range::Range;
use tracing::warn;

use distribution_types::Verbatim;
use pep440_rs::Version;
use pypi_types::{
    ParsedArchiveUrl, ParsedDirectoryUrl, ParsedGitUrl, ParsedPathUrl, ParsedUrl, Requirement,
    RequirementSource,
};
use uv_git::GitResolver;
use uv_normalize::{ExtraName, PackageName};

use crate::pubgrub::{PubGrubPackage, PubGrubPackageInner};
use crate::resolver::{Locals, Urls};
use crate::{PubGrubSpecifier, ResolveError};

#[derive(Clone, Debug)]
pub(crate) struct PubGrubDependency {
    pub(crate) package: PubGrubPackage,
    pub(crate) version: Range<Version>,
}

impl PubGrubDependency {
    pub(crate) fn from_requirement<'a>(
        requirement: &'a Requirement,
        source_name: Option<&'a PackageName>,
        urls: &'a Urls,
        locals: &'a Locals,
        git: &'a GitResolver,
    ) -> impl Iterator<Item = Result<Self, ResolveError>> + 'a {
        // Add the package, plus any extra variants.
        std::iter::once(None)
            .chain(requirement.extras.clone().into_iter().map(Some))
            .map(|extra| {
                PubGrubRequirement::from_requirement(requirement, extra, urls, locals, git)
            })
            .filter_map_ok(move |pubgrub_requirement| {
                let PubGrubRequirement { package, version } = pubgrub_requirement;

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
                        })
                    }
                    PubGrubPackageInner::Marker { .. } => Some(PubGrubDependency {
                        package: package.clone(),
                        version: version.clone(),
                    }),
                    PubGrubPackageInner::Extra { name, .. } => {
                        debug_assert!(
                            !source_name.is_some_and(|source_name| source_name == name),
                            "extras not flattened for {name}"
                        );
                        Some(PubGrubDependency {
                            package: package.clone(),
                            version: version.clone(),
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
}

impl PubGrubRequirement {
    /// Convert a [`Requirement`] to a PubGrub-compatible package and range.
    pub(crate) fn from_requirement(
        requirement: &Requirement,
        extra: Option<ExtraName>,
        urls: &Urls,
        locals: &Locals,
        git: &GitResolver,
    ) -> Result<Self, ResolveError> {
        match &requirement.source {
            RequirementSource::Registry { specifier, .. } => {
                // TODO(konsti): We're currently losing the index information here, but we need
                // either pass it to `PubGrubPackage` or the `ResolverProvider` beforehand.
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

                Ok(Self {
                    package: PubGrubPackage::from_package(
                        requirement.name.clone(),
                        extra,
                        requirement.marker.clone(),
                        urls,
                    ),
                    version,
                })
            }
            RequirementSource::Url {
                subdirectory,
                location,
                url,
            } => {
                let Some(expected) = urls.get(&requirement.name) else {
                    return Err(ResolveError::DisallowedUrl(
                        requirement.name.clone(),
                        url.to_string(),
                    ));
                };

                let parsed_url = ParsedUrl::Archive(ParsedArchiveUrl::from_source(
                    location.clone(),
                    subdirectory.clone(),
                ));
                if !Urls::same_resource(&expected.parsed_url, &parsed_url, git) {
                    return Err(ResolveError::ConflictingUrlsTransitive(
                        requirement.name.clone(),
                        expected.verbatim.verbatim().to_string(),
                        url.verbatim().to_string(),
                    ));
                }

                Ok(Self {
                    package: PubGrubPackage::from_url(
                        requirement.name.clone(),
                        extra,
                        requirement.marker.clone(),
                        expected.clone(),
                    ),
                    version: Range::full(),
                })
            }
            RequirementSource::Git {
                repository,
                reference,
                precise,
                url,
                subdirectory,
            } => {
                let Some(expected) = urls.get(&requirement.name) else {
                    return Err(ResolveError::DisallowedUrl(
                        requirement.name.clone(),
                        url.to_string(),
                    ));
                };

                let parsed_url = ParsedUrl::Git(ParsedGitUrl::from_source(
                    repository.clone(),
                    reference.clone(),
                    *precise,
                    subdirectory.clone(),
                ));
                if !Urls::same_resource(&expected.parsed_url, &parsed_url, git) {
                    return Err(ResolveError::ConflictingUrlsTransitive(
                        requirement.name.clone(),
                        expected.verbatim.verbatim().to_string(),
                        url.verbatim().to_string(),
                    ));
                }

                Ok(Self {
                    package: PubGrubPackage::from_url(
                        requirement.name.clone(),
                        extra,
                        requirement.marker.clone(),
                        expected.clone(),
                    ),
                    version: Range::full(),
                })
            }
            RequirementSource::Path {
                url,
                install_path,
                lock_path,
            } => {
                let Some(expected) = urls.get(&requirement.name) else {
                    return Err(ResolveError::DisallowedUrl(
                        requirement.name.clone(),
                        url.to_string(),
                    ));
                };

                let parsed_url = ParsedUrl::Path(ParsedPathUrl::from_source(
                    install_path.clone(),
                    lock_path.clone(),
                    url.to_url(),
                ));
                if !Urls::same_resource(&expected.parsed_url, &parsed_url, git) {
                    return Err(ResolveError::ConflictingUrlsTransitive(
                        requirement.name.clone(),
                        expected.verbatim.verbatim().to_string(),
                        url.verbatim().to_string(),
                    ));
                }

                Ok(Self {
                    package: PubGrubPackage::from_url(
                        requirement.name.clone(),
                        extra,
                        requirement.marker.clone(),
                        expected.clone(),
                    ),
                    version: Range::full(),
                })
            }
            RequirementSource::Directory {
                editable,
                url,
                install_path,
                lock_path,
            } => {
                let Some(expected) = urls.get(&requirement.name) else {
                    return Err(ResolveError::DisallowedUrl(
                        requirement.name.clone(),
                        url.to_string(),
                    ));
                };

                let parsed_url = ParsedUrl::Directory(ParsedDirectoryUrl::from_source(
                    install_path.clone(),
                    lock_path.clone(),
                    *editable,
                    url.to_url(),
                ));
                if !Urls::same_resource(&expected.parsed_url, &parsed_url, git) {
                    return Err(ResolveError::ConflictingUrlsTransitive(
                        requirement.name.clone(),
                        expected.verbatim.verbatim().to_string(),
                        url.verbatim().to_string(),
                    ));
                }

                Ok(Self {
                    package: PubGrubPackage::from_url(
                        requirement.name.clone(),
                        extra,
                        requirement.marker.clone(),
                        expected.clone(),
                    ),
                    version: Range::full(),
                })
            }
        }
    }
}

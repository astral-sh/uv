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

#[derive(Debug)]
pub struct PubGrubDependencies(Vec<(PubGrubPackage, Range<Version>)>);

impl PubGrubDependencies {
    /// Generate a set of PubGrub dependencies from a set of requirements.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_requirements(
        flattened_requirements: &[&Requirement],
        source_name: Option<&PackageName>,
        urls: &Urls,
        locals: &Locals,
        git: &GitResolver,
    ) -> Result<Self, ResolveError> {
        let mut dependencies = Vec::new();
        for requirement in flattened_requirements {
            // Add the package, plus any extra variants.
            for result in std::iter::once(PubGrubRequirement::from_requirement(
                requirement,
                None,
                urls,
                locals,
                git,
            ))
            .chain(requirement.extras.clone().into_iter().map(|extra| {
                PubGrubRequirement::from_requirement(requirement, Some(extra), urls, locals, git)
            })) {
                let PubGrubRequirement { package, version } = result?;

                match &*package {
                    PubGrubPackageInner::Package { name, .. } => {
                        // Detect self-dependencies.
                        if source_name.is_some_and(|source_name| source_name == name) {
                            warn!("{name} has a dependency on itself");
                            continue;
                        }

                        dependencies.push((package.clone(), version.clone()));
                    }
                    PubGrubPackageInner::Marker { .. } => {
                        dependencies.push((package.clone(), version.clone()));
                    }
                    PubGrubPackageInner::Extra { name, .. } => {
                        debug_assert!(
                            !source_name.is_some_and(|source_name| source_name == name),
                            "extras not flattened for {name}"
                        );
                        dependencies.push((package.clone(), version.clone()));
                    }
                    _ => {}
                }
            }
        }
        Ok(Self(dependencies))
    }

    /// Add a [`PubGrubPackage`] and [`PubGrubVersion`] range into the dependencies.
    pub(crate) fn push(&mut self, package: PubGrubPackage, version: Range<Version>) {
        self.0.push((package, version));
    }

    /// Iterate over the dependencies.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &(PubGrubPackage, Range<Version>)> {
        self.0.iter()
    }
}

/// Convert a [`PubGrubDependencies`] to a [`DependencyConstraints`].
impl From<PubGrubDependencies> for Vec<(PubGrubPackage, Range<Version>)> {
    fn from(dependencies: PubGrubDependencies) -> Self {
        dependencies.0
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

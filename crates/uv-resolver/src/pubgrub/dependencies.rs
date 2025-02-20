use std::borrow::Cow;
use std::iter;

use either::Either;
use pubgrub::Ranges;

use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pypi_types::{
    Conflicts, ParsedArchiveUrl, ParsedDirectoryUrl, ParsedGitUrl, ParsedPathUrl, ParsedUrl,
    Requirement, RequirementSource, VerbatimParsedUrl,
};

use crate::pubgrub::{PubGrubPackage, PubGrubPackageInner};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PubGrubDependency {
    pub(crate) package: PubGrubPackage,
    pub(crate) version: Ranges<Version>,

    /// This field is set if the [`Requirement`] had a URL. We still use a URL from [`Urls`]
    /// even if this field is None where there is an override with a URL or there is a different
    /// requirement or constraint for the same package that has a URL.
    pub(crate) url: Option<VerbatimParsedUrl>,
}

impl PubGrubDependency {
    pub(crate) fn from_requirement<'a>(
        conflicts: &Conflicts,
        requirement: Cow<'a, Requirement>,
        dev: Option<&'a GroupName>,
        source_name: Option<&'a PackageName>,
    ) -> impl Iterator<Item = Self> + 'a {
        let iter = if !requirement.extras.is_empty() {
            // This is crazy subtle, but if any of the extras in the
            // requirement are part of a declared conflict, then we
            // specifically need (at time of writing) to include the
            // base package as a dependency. This results in both
            // the base package and the extra package being sibling
            // dependencies at the point in which forks are created
            // base on conflicting extras. If the base package isn't
            // present at that point, then it's impossible for the
            // fork that excludes all conflicting extras to reach
            // the non-extra dependency, which may be necessary for
            // correctness.
            //
            // But why do we not include the base package in the first
            // place? Well, that's part of an optimization[1].
            //
            // [1]: https://github.com/astral-sh/uv/pull/9540
            let base = if requirement
                .extras
                .iter()
                .any(|extra| conflicts.contains(&requirement.name, extra))
            {
                Either::Left(iter::once((None, None)))
            } else {
                Either::Right(iter::empty())
            };
            Either::Left(Either::Left(
                base.chain(
                    requirement
                        .extras
                        .clone()
                        .into_iter()
                        .map(|extra| (Some(extra), None)),
                ),
            ))
        } else if !requirement.groups.is_empty() {
            let base = if requirement
                .groups
                .iter()
                .any(|group| conflicts.contains(&requirement.name, group))
            {
                Either::Left(iter::once((None, None)))
            } else {
                Either::Right(iter::empty())
            };
            Either::Left(Either::Right(
                base.chain(
                    requirement
                        .groups
                        .clone()
                        .into_iter()
                        .map(|group| (None, Some(group))),
                ),
            ))
        } else {
            Either::Right(iter::once((None, None)))
        };

        // Add the package, plus any extra variants.
        iter.map(move |(extra, group)| {
            PubGrubRequirement::from_requirement(&requirement, extra, group)
        })
        .map(move |requirement| {
            let PubGrubRequirement {
                package,
                version,
                url,
            } = requirement;
            match &*package {
                PubGrubPackageInner::Package { .. } => PubGrubDependency {
                    package,
                    version,
                    url,
                },
                PubGrubPackageInner::Marker { .. } => PubGrubDependency {
                    package,
                    version,
                    url,
                },
                PubGrubPackageInner::Extra { name, .. } => {
                    // Detect self-dependencies.
                    if dev.is_none() {
                        debug_assert!(
                            source_name.is_none_or(|source_name| source_name != name),
                            "extras not flattened for {name}"
                        );
                    }
                    PubGrubDependency {
                        package,
                        version,
                        url,
                    }
                }
                PubGrubPackageInner::Dev { name, .. } => {
                    // Detect self-dependencies.
                    if dev.is_none() {
                        debug_assert!(
                            source_name.is_none_or(|source_name| source_name != name),
                            "group not flattened for {name}"
                        );
                    }
                    PubGrubDependency {
                        package,
                        version,
                        url,
                    }
                }
                PubGrubPackageInner::Root(_) => unreachable!("root package in dependencies"),
                PubGrubPackageInner::Python(_) => {
                    unreachable!("python package in dependencies")
                }
            }
        })
    }
}

/// A PubGrub-compatible package and version range.
#[derive(Debug, Clone)]
pub(crate) struct PubGrubRequirement {
    pub(crate) package: PubGrubPackage,
    pub(crate) version: Ranges<Version>,
    pub(crate) url: Option<VerbatimParsedUrl>,
}

impl PubGrubRequirement {
    /// Convert a [`Requirement`] to a PubGrub-compatible package and range, while returning the URL
    /// on the [`Requirement`], if any.
    pub(crate) fn from_requirement(
        requirement: &Requirement,
        extra: Option<ExtraName>,
        group: Option<GroupName>,
    ) -> Self {
        let (verbatim_url, parsed_url) = match &requirement.source {
            RequirementSource::Registry { specifier, .. } => {
                return Self::from_registry_requirement(specifier, extra, group, requirement);
            }
            RequirementSource::Url {
                subdirectory,
                location,
                ext,
                url,
            } => {
                let parsed_url = ParsedUrl::Archive(ParsedArchiveUrl::from_source(
                    location.clone(),
                    subdirectory.clone(),
                    *ext,
                ));
                (url, parsed_url)
            }
            RequirementSource::Git {
                git,
                url,
                subdirectory,
            } => {
                let parsed_url =
                    ParsedUrl::Git(ParsedGitUrl::from_source(git.clone(), subdirectory.clone()));
                (url, parsed_url)
            }
            RequirementSource::Path {
                ext,
                url,
                install_path,
            } => {
                let parsed_url = ParsedUrl::Path(ParsedPathUrl::from_source(
                    install_path.clone(),
                    *ext,
                    url.to_url(),
                ));
                (url, parsed_url)
            }
            RequirementSource::Directory {
                editable,
                r#virtual,
                url,
                install_path,
            } => {
                let parsed_url = ParsedUrl::Directory(ParsedDirectoryUrl::from_source(
                    install_path.clone(),
                    *editable,
                    *r#virtual,
                    url.to_url(),
                ));
                (url, parsed_url)
            }
        };

        Self {
            package: PubGrubPackage::from_package(
                requirement.name.clone(),
                extra,
                group,
                requirement.marker,
            ),
            version: Ranges::full(),
            url: Some(VerbatimParsedUrl {
                parsed_url,
                verbatim: verbatim_url.clone(),
            }),
        }
    }

    fn from_registry_requirement(
        specifier: &VersionSpecifiers,
        extra: Option<ExtraName>,
        group: Option<GroupName>,
        requirement: &Requirement,
    ) -> PubGrubRequirement {
        Self {
            package: PubGrubPackage::from_package(
                requirement.name.clone(),
                extra,
                group,
                requirement.marker,
            ),
            url: None,
            version: Ranges::from(specifier.clone()),
        }
    }
}

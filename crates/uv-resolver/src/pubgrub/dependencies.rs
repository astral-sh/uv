use std::borrow::Cow;
use std::iter;

use either::Either;
use pubgrub::Ranges;

use uv_distribution_types::{Requirement, RequirementSource};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pypi_types::{
    ConflictItemRef, Conflicts, ParsedArchiveUrl, ParsedDirectoryUrl, ParsedGitUrl, ParsedPathUrl,
    ParsedUrl, VerbatimParsedUrl,
};

use crate::pubgrub::{PubGrubPackage, PubGrubPackageInner};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PubGrubDependency {
    pub(crate) package: PubGrubPackage,
    pub(crate) version: Ranges<Version>,

    /// When the parent that created this dependency is a "normal" package
    /// (non-extra non-group), this corresponds to its name.
    ///
    /// This is used to create project-level `ConflictItemRef` for a specific
    /// package. In effect, this lets us "delay" filtering of project
    /// dependencies when a conflict is declared between the project and a
    /// group.
    ///
    /// The main problem with dealing with project level conflicts is that if you
    /// declare a conflict between a package and a group, we represent that
    /// group as a dependency of that package. So if you filter out the package
    /// in a fork due to a conflict, you also filter out the group. Therefore,
    /// we introduce this parent field to enable "delayed" filtering.
    pub(crate) parent: Option<PackageName>,

    /// This field is set if the [`Requirement`] had a URL. We still use a URL from [`Urls`]
    /// even if this field is None where there is an override with a URL or there is a different
    /// requirement or constraint for the same package that has a URL.
    pub(crate) url: Option<VerbatimParsedUrl>,
}

impl PubGrubDependency {
    pub(crate) fn from_requirement<'a>(
        conflicts: &Conflicts,
        requirement: Cow<'a, Requirement>,
        group_name: Option<&'a GroupName>,
        parent_package: Option<&'a PubGrubPackage>,
    ) -> impl Iterator<Item = Self> + 'a {
        let parent_name = parent_package.and_then(|package| package.name_no_root());
        let is_normal_parent = parent_package
            .map(|pp| pp.extra().is_none() && pp.group().is_none())
            .unwrap_or(false);
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
            Either::Left(Either::Left(base.chain(
                Box::into_iter(requirement.extras.clone()).map(|extra| (Some(extra), None)),
            )))
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
            Either::Left(Either::Right(base.chain(
                Box::into_iter(requirement.groups.clone()).map(|group| (None, Some(group))),
            )))
        } else {
            Either::Right(iter::once((None, None)))
        };

        // Add the package, plus any extra variants.
        iter.map(move |(extra, group)| {
            let pubgrub_requirement =
                PubGrubRequirement::from_requirement(&requirement, extra, group);
            let PubGrubRequirement {
                package,
                version,
                url,
            } = pubgrub_requirement;
            match &*package {
                PubGrubPackageInner::Package { .. } => Self {
                    package,
                    version,
                    parent: if is_normal_parent {
                        parent_name.cloned()
                    } else {
                        None
                    },
                    url,
                },
                PubGrubPackageInner::Marker { .. } => Self {
                    package,
                    version,
                    parent: if is_normal_parent {
                        parent_name.cloned()
                    } else {
                        None
                    },
                    url,
                },
                PubGrubPackageInner::Extra { name, .. } => {
                    if group_name.is_none() {
                        debug_assert!(
                            parent_name.is_none_or(|parent_name| parent_name != name),
                            "extras not flattened for {name}"
                        );
                    }
                    Self {
                        package,
                        version,
                        parent: None,
                        url,
                    }
                }
                PubGrubPackageInner::Group { name, .. } => {
                    if group_name.is_none() {
                        debug_assert!(
                            parent_name.is_none_or(|parent_name| parent_name != name),
                            "group not flattened for {name}"
                        );
                    }
                    Self {
                        package,
                        version,
                        parent: None,
                        url,
                    }
                }
                PubGrubPackageInner::Root(_) => unreachable!("Root package in dependencies"),
                PubGrubPackageInner::Python(_) => {
                    unreachable!("Python package in dependencies")
                }
                PubGrubPackageInner::System(_) => unreachable!("System package in dependencies"),
            }
        })
    }

    /// Extracts a possible conflicting item from this dependency.
    ///
    /// If this package can't possibly be classified as conflicting, then this
    /// returns `None`.
    pub(crate) fn conflicting_item(&self) -> Option<ConflictItemRef<'_>> {
        self.package.conflicting_item()
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
    ) -> Self {
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

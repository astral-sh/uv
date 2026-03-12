use std::borrow::Cow;
use std::iter;

use either::Either;
use pubgrub::Ranges;

use uv_distribution_types::{IndexMetadata, Requirement, RequirementSource};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::RequirementOrigin;
use uv_pypi_types::{
    ConflictItemRef, Conflicts, ParsedArchiveUrl, ParsedDirectoryUrl, ParsedGitUrl, ParsedPathUrl,
    ParsedUrl, VerbatimParsedUrl,
};

use crate::pubgrub::{PubGrubPackage, PubGrubPackageInner};

/// The source constraint carried by a single dependency edge.
///
/// Most dependency edges are source-agnostic and use [`DependencySource::Unspecified`]. Direct
/// URLs and group-scoped explicit indexes use a concrete source so fork construction can keep
/// that source information attached to the edge that introduced it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum DependencySource {
    /// The dependency does not carry an edge-local source constraint.
    #[default]
    Unspecified,
    /// The dependency was introduced by a direct URL-like requirement.
    Url(Box<VerbatimParsedUrl>),
    /// The dependency was introduced by a requirement pinned to an explicit index.
    ExplicitIndex(IndexMetadata),
}

impl DependencySource {
    /// Derive the edge-local source constraint from a requirement.
    ///
    /// Registry requirements only carry a source here when they are tied to a group-scoped
    /// explicit index. Direct URL-like requirements always preserve their verbatim URL.
    pub(crate) fn from_requirement(requirement: &Requirement) -> Self {
        Self::from_requirement_source(&requirement.source, requirement.origin.as_ref())
    }

    /// Derive the edge-local source constraint from a requirement source and its origin.
    ///
    /// Group-scoped explicit indexes only survive when the caller provides the originating group.
    /// Callers that only have a `RequirementSource` should pass `None`, which preserves URL-like
    /// sources but treats registry requirements as source-agnostic.
    pub(crate) fn from_requirement_source(
        source: &RequirementSource,
        origin: Option<&RequirementOrigin>,
    ) -> Self {
        match source {
            RequirementSource::Registry { index, .. }
                if matches!(origin, Some(RequirementOrigin::Group(_, Some(_), _))) =>
            {
                index
                    .clone()
                    .map(Self::ExplicitIndex)
                    .unwrap_or(Self::Unspecified)
            }
            RequirementSource::Registry { .. } => Self::Unspecified,
            RequirementSource::Url { .. }
            | RequirementSource::Git { .. }
            | RequirementSource::Path { .. }
            | RequirementSource::Directory { .. } => source
                .to_verbatim_parsed_url()
                .map(Box::new)
                .map(Self::Url)
                .unwrap_or(Self::Unspecified),
        }
    }

    /// Return the direct URL attached to this source, if any.
    pub(crate) fn verbatim_url(&self) -> Option<&VerbatimParsedUrl> {
        match self {
            Self::Url(url) => Some(url.as_ref()),
            Self::Unspecified | Self::ExplicitIndex(_) => None,
        }
    }

    /// Return the explicit index attached to this source, if any.
    pub(crate) fn explicit_index(&self) -> Option<&IndexMetadata> {
        match self {
            Self::ExplicitIndex(index) => Some(index),
            Self::Unspecified | Self::Url(_) => None,
        }
    }

    /// Returns `true` when this dependency source carries no edge-local source constraint.
    pub(crate) fn is_unspecified(&self) -> bool {
        matches!(self, Self::Unspecified)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::str::FromStr;

    use uv_distribution_types::{IndexFormat, IndexMetadata, IndexUrl};
    use uv_normalize::{GroupName, PackageName};
    use uv_pep440::VersionSpecifiers;
    use uv_pep508::MarkerTree;

    use super::*;

    #[test]
    fn requirement_source_needs_origin_for_group_explicit_index() {
        let index = IndexMetadata {
            url: IndexUrl::parse("https://test.pypi.org/simple", None).unwrap(),
            format: IndexFormat::Simple,
        };
        let requirement = Requirement {
            name: PackageName::from_str("demo").unwrap(),
            extras: Box::default(),
            groups: Box::default(),
            marker: MarkerTree::TRUE,
            source: RequirementSource::Registry {
                specifier: VersionSpecifiers::empty(),
                index: Some(index.clone()),
                conflict: None,
            },
            origin: Some(RequirementOrigin::Group(
                PathBuf::from("pyproject.toml"),
                Some(PackageName::from_str("project").unwrap()),
                GroupName::from_str("dev").unwrap(),
            )),
        };

        assert_eq!(
            DependencySource::from_requirement(&requirement),
            DependencySource::ExplicitIndex(index.clone())
        );
        assert_eq!(
            DependencySource::from_requirement_source(&requirement.source, None),
            DependencySource::Unspecified
        );
        assert_eq!(
            DependencySource::from_requirement_source(
                &requirement.source,
                requirement.origin.as_ref()
            ),
            DependencySource::ExplicitIndex(index)
        );
    }
}

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

    /// The direct source constraint attached to this dependency edge.
    ///
    /// This is only populated when the edge itself needs source identity, e.g. for direct URLs
    /// or group-scoped explicit indexes. Manifest-wide URL and index constraints are still applied
    /// separately via `Urls` and `Indexes`.
    pub(crate) source: DependencySource,
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
                source,
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
                    source,
                },
                PubGrubPackageInner::Marker { .. } => Self {
                    package,
                    version,
                    parent: if is_normal_parent {
                        parent_name.cloned()
                    } else {
                        None
                    },
                    source,
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
                        source,
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
                        source,
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

    /// Returns `true` if this dependency targets the given package name and carries the given
    /// edge-local source constraint.
    pub(crate) fn matches_name_and_source(
        &self,
        name: &PackageName,
        source: &DependencySource,
    ) -> bool {
        self.package.name() == Some(name) && &self.source == source
    }

    /// Returns `true` if this dependency is the plain base package for the given name, without an
    /// extra, group, or edge-local source constraint.
    pub(crate) fn is_unsourced_base_for(&self, name: &PackageName) -> bool {
        self.package.name() == Some(name)
            && self.package.extra().is_none()
            && self.package.group().is_none()
            && self.source.is_unspecified()
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
    pub(crate) source: DependencySource,
}

impl PubGrubRequirement {
    fn package_for_requirement(
        requirement: &Requirement,
        extra: Option<ExtraName>,
        group: Option<GroupName>,
    ) -> PubGrubPackage {
        PubGrubPackage::from_package(requirement.name.clone(), extra, group, requirement.marker)
    }

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
            package: Self::package_for_requirement(requirement, extra, group),
            version: Ranges::full(),
            source: DependencySource::Url(Box::new(VerbatimParsedUrl {
                parsed_url,
                verbatim: verbatim_url.clone(),
            })),
        }
    }

    fn from_registry_requirement(
        specifier: &VersionSpecifiers,
        extra: Option<ExtraName>,
        group: Option<GroupName>,
        requirement: &Requirement,
    ) -> Self {
        Self {
            package: Self::package_for_requirement(requirement, extra, group),
            source: DependencySource::from_requirement(requirement),
            version: Ranges::from(specifier.clone()),
        }
    }
}

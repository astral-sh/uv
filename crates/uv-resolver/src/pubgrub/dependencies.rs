use std::borrow::Cow;
use std::iter;
use std::sync::Arc;

use either::Either;

use uv_distribution_types::{IndexMetadata, Requirement, RequirementSource};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pypi_types::{ConflictItemRef, Conflicts, VerbatimParsedUrl};

use crate::pubgrub::{PubGrubPackage, PubGrubPackageInner, Range};
use crate::resolver::UnsatisfiableRequirement;

/// The source constraint carried by a single dependency edge.
///
/// Most dependency edges are source-agnostic and use [`DependencySource::Unspecified`]. Direct
/// URLs and explicit indexes use a concrete source so fork construction can keep
/// that source information attached to the edge that introduced it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum DependencySource {
    /// The dependency does not carry an edge-local source constraint.
    #[default]
    Unspecified,
    /// The dependency was introduced by a direct URL-like requirement.
    Url {
        url: Box<VerbatimParsedUrl>,
        trusted: bool,
        hash_requirement: Option<Arc<Requirement>>,
        trusted_hashes: bool,
    },
    /// The dependency was introduced by a requirement pinned to an explicit index.
    ExplicitIndex(IndexMetadata),
}

impl DependencySource {
    /// Derive the edge-local source constraint from a requirement.
    ///
    /// Registry requirements carry an explicitly configured index when present. Direct URL-like
    /// requirements always preserve their verbatim URL.
    fn from_requirement(requirement: &Requirement) -> Self {
        match &requirement.source {
            RequirementSource::Registry { index, .. } => index
                .clone()
                .map(Self::ExplicitIndex)
                .unwrap_or(Self::Unspecified),
            RequirementSource::Url { .. }
            | RequirementSource::GitDirectory { .. }
            | RequirementSource::GitPath { .. }
            | RequirementSource::Path { .. }
            | RequirementSource::Directory { .. } => requirement
                .source
                .to_verbatim_parsed_url()
                .map(Box::new)
                .map(|url| Self::Url {
                    url,
                    trusted: false,
                    hash_requirement: None,
                    trusted_hashes: false,
                })
                .unwrap_or(Self::Unspecified),
        }
    }

    /// Return the direct URL attached to this source, if any.
    pub(crate) fn verbatim_url(&self) -> Option<&VerbatimParsedUrl> {
        match self {
            Self::Url { url, .. } => Some(url.as_ref()),
            Self::Unspecified | Self::ExplicitIndex(_) => None,
        }
    }

    /// Return the explicit index attached to this source, if any.
    pub(crate) fn explicit_index(&self) -> Option<&IndexMetadata> {
        match self {
            Self::ExplicitIndex(index) => Some(index),
            Self::Unspecified | Self::Url { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PubGrubDependency {
    pub(crate) package: PubGrubPackage,
    pub(crate) version: Range<Version>,

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
    /// Direct URLs retain their declaring edge and authority. Explicit indexes also retain their
    /// declaring edge alongside any initial index configuration.
    pub(crate) source: DependencySource,

    /// A first-party declaration that can opt into prereleases, yanks or lowest-direct selection.
    pub(crate) policy: Option<(Arc<Requirement>, bool)>,
}

impl PubGrubDependency {
    /// Convert flattened requirements into PubGrub dependency edges.
    ///
    /// An empty range cannot retain the specifiers that produced it, so return the source
    /// requirement details instead. The resolver attaches them to the parent package as the
    /// reason that package cannot be selected.
    pub(crate) fn from_requirements<'a>(
        conflicts: &Conflicts,
        requirements: impl IntoIterator<Item = Cow<'a, Requirement>>,
        group_name: Option<&'a GroupName>,
        parent_package: Option<&'a PubGrubPackage>,
        authorizes: impl Fn(&Requirement) -> bool,
        trusted_hashes: impl Fn(&Requirement) -> bool,
        policy: Option<bool>,
    ) -> Result<Vec<Self>, UnsatisfiableRequirement> {
        let mut dependencies = Vec::new();
        for requirement in requirements {
            let trusted = authorizes(&requirement);
            let hashes_are_trusted = trusted_hashes(&requirement);
            let hash_requirement = (requirement.source.to_verbatim_parsed_url().is_some()
                && (trusted || hashes_are_trusted))
                .then(|| Arc::new(requirement.as_ref().clone()));
            let policy = policy.map(|lowest| {
                (
                    hash_requirement
                        .clone()
                        .unwrap_or_else(|| Arc::new(requirement.as_ref().clone())),
                    lowest,
                )
            });
            dependencies.extend(
                Self::from_requirement(conflicts, requirement, group_name, parent_package)?.map(
                    |mut dependency| {
                        if let DependencySource::Url {
                            trusted: authority,
                            hash_requirement: hashes,
                            trusted_hashes: hash_authority,
                            ..
                        } = &mut dependency.source
                        {
                            *authority = trusted;
                            hashes.clone_from(&hash_requirement);
                            *hash_authority = hashes_are_trusted;
                        }
                        dependency.policy.clone_from(&policy);
                        dependency
                    },
                ),
            );
        }
        Ok(dependencies)
    }

    fn from_requirement<'a>(
        conflicts: &Conflicts,
        requirement: Cow<'a, Requirement>,
        group_name: Option<&'a GroupName>,
        parent_package: Option<&'a PubGrubPackage>,
    ) -> Result<impl Iterator<Item = Self> + 'a, UnsatisfiableRequirement> {
        if let Some(requirement) = UnsatisfiableRequirement::from_requirement(&requirement) {
            return Err(requirement);
        }

        let parent_name = parent_package.and_then(|package| package.name_no_root());
        let is_normal_parent = parent_package
            .is_some_and(|parent| parent.extra().is_none() && parent.group().is_none());
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
        Ok(iter.map(move |(extra, group)| {
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
                    policy: None,
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
                    policy: None,
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
                        policy: None,
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
                        policy: None,
                    }
                }
                PubGrubPackageInner::Root(_) => unreachable!("Root package in dependencies"),
                PubGrubPackageInner::Python(_) => {
                    unreachable!("Python package in dependencies")
                }
                PubGrubPackageInner::System(_) => unreachable!("System package in dependencies"),
            }
        }))
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
struct PubGrubRequirement {
    package: PubGrubPackage,
    version: Range<Version>,
    source: DependencySource,
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
    fn from_requirement(
        requirement: &Requirement,
        extra: Option<ExtraName>,
        group: Option<GroupName>,
    ) -> Self {
        if let RequirementSource::Registry { specifier, .. } = &requirement.source {
            return Self::from_registry_requirement(specifier, extra, group, requirement);
        }

        Self {
            package: Self::package_for_requirement(requirement, extra, group),
            version: Range::full(),
            source: DependencySource::from_requirement(requirement),
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
            version: Range::from(specifier.clone()),
        }
    }
}

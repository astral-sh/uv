use std::borrow::Cow;
use std::iter;

use either::Either;
use pubgrub::Ranges;

use uv_distribution_types::{IndexMetadata, IndexUrl, Requirement, RequirementSource};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::RequirementOrigin;
use uv_pypi_types::{
    ConflictItem, ConflictItemRef, Conflicts, ParsedArchiveUrl, ParsedDirectoryUrl,
    ParsedGitDirectoryUrl, ParsedGitPathUrl, ParsedPathUrl, ParsedUrl, VerbatimParsedUrl,
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
    /// Derive an edge-local source constraint directly from a requirement source.
    pub(crate) fn from_source(source: &RequirementSource) -> Self {
        match source {
            RequirementSource::Registry { index, .. } => index
                .clone()
                .map(Self::ExplicitIndex)
                .unwrap_or(Self::Unspecified),
            RequirementSource::Url { .. }
            | RequirementSource::GitDirectory { .. }
            | RequirementSource::GitPath { .. }
            | RequirementSource::Path { .. }
            | RequirementSource::Directory { .. } => source
                .to_verbatim_parsed_url()
                .map(Box::new)
                .map(Self::Url)
                .unwrap_or(Self::Unspecified),
        }
    }

    /// Derive the edge-local source constraint from a requirement.
    ///
    /// Registry requirements only carry a source here when they are tied to a group-scoped
    /// explicit index. Direct URL-like requirements always preserve their verbatim URL.
    fn from_requirement(requirement: &Requirement) -> Self {
        match &requirement.source {
            RequirementSource::Registry { .. }
                if !matches!(
                    requirement.origin.as_ref(),
                    Some(RequirementOrigin::Group(_, Some(_), _))
                ) =>
            {
                Self::Unspecified
            }
            source => Self::from_source(source),
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

    /// Returns whether this source matches a resolved URL or index.
    pub(crate) fn matches_resolution(
        &self,
        url: Option<&VerbatimParsedUrl>,
        index: Option<&IndexUrl>,
    ) -> bool {
        match self {
            Self::Unspecified => false,
            Self::Url(source) => url == Some(source.as_ref()),
            Self::ExplicitIndex(source) => index == Some(source.url()),
        }
    }
}

/// Resolution-only context for a dependency affected by conditional sources.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct DependencySourceContext {
    /// Package extras that were scoped only to isolate source identity.
    ///
    /// These package-qualified extras can be translated back to ordinary extra markers when
    /// constructing the resolution graph. Explicit-index conflicts and groups are intentionally
    /// excluded.
    pub(crate) extra_scopes: Box<[ConflictItem]>,

    /// The extra or group whose activation selects this source.
    ///
    /// Source-scoped dependencies are proxy dependencies for the selected source context. They
    /// must participate in declared conflict filtering even though their package identity belongs
    /// to the dependency rather than the project that declared the source.
    pub(crate) conflict: Option<ConflictItem>,

    /// The marker to persist on the corresponding resolution graph edge.
    ///
    /// When this is `None`, the graph uses the package marker. Source-scoped dependencies may need
    /// an override because PubGrub uses a package-qualified marker to isolate source identity,
    /// while the graph edge should retain the original metadata condition.
    pub(crate) edge_marker: Option<uv_pep508::MarkerTree>,

    /// Markers to persist when the dependency resolves through a specific source.
    ///
    /// Unlike [`Self::edge_marker`], these are selected after resolution, when a registry
    /// requirement may have inherited a conditional URL or explicit index from another edge.
    pub(crate) source_edge_markers: Box<[(DependencySource, uv_pep508::MarkerTree)]>,
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

    /// Resolution-only context for conditional source handling.
    pub(crate) source_context: DependencySourceContext,

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
        let source = DependencySource::from_requirement(&requirement);
        Self::from_requirement_with_markers(
            conflicts,
            requirement,
            None,
            DependencySourceContext::default(),
            source,
            group_name,
            parent_package,
        )
    }

    /// Convert a requirement using an already-scoped package marker and edge-local source.
    pub(crate) fn from_requirement_with_marker<'a>(
        conflicts: &Conflicts,
        requirement: Cow<'a, Requirement>,
        marker: uv_pep508::MarkerTree,
        source_context: DependencySourceContext,
        source: DependencySource,
        group_name: Option<&'a GroupName>,
        parent_package: Option<&'a PubGrubPackage>,
    ) -> impl Iterator<Item = Self> + 'a {
        Self::from_requirement_with_markers(
            conflicts,
            requirement,
            Some(marker),
            source_context,
            source,
            group_name,
            parent_package,
        )
    }

    /// Convert a requirement normally, but preserve graph markers independently of solver markers.
    pub(crate) fn from_requirement_with_edge_markers<'a>(
        conflicts: &Conflicts,
        requirement: Cow<'a, Requirement>,
        edge_marker: Option<uv_pep508::MarkerTree>,
        source_edge_markers: Box<[(DependencySource, uv_pep508::MarkerTree)]>,
        group_name: Option<&'a GroupName>,
        parent_package: Option<&'a PubGrubPackage>,
    ) -> impl Iterator<Item = Self> + 'a {
        let source = DependencySource::from_requirement(&requirement);
        Self::from_requirement_with_markers(
            conflicts,
            requirement,
            None,
            DependencySourceContext {
                edge_marker,
                source_edge_markers,
                ..DependencySourceContext::default()
            },
            source,
            group_name,
            parent_package,
        )
    }

    fn from_requirement_with_markers<'a>(
        conflicts: &Conflicts,
        requirement: Cow<'a, Requirement>,
        marker: Option<uv_pep508::MarkerTree>,
        source_context: DependencySourceContext,
        source: DependencySource,
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
            let pubgrub_requirement = if let Some(marker) = marker {
                PubGrubRequirement::from_requirement_with_marker(
                    &requirement,
                    extra,
                    group,
                    marker,
                    source.clone(),
                )
            } else {
                PubGrubRequirement::from_requirement(&requirement, extra, group)
            };
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
                    source_context: source_context.clone(),
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
                    source_context: source_context.clone(),
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
                        source_context: source_context.clone(),
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
                        source_context: source_context.clone(),
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

    /// Returns the conflict items represented by this dependency.
    pub(crate) fn conflicting_items(&self) -> impl Iterator<Item = ConflictItemRef<'_>> {
        self.package.conflicting_item().into_iter().chain(
            self.source_context
                .conflict
                .as_ref()
                .map(|item| item.as_ref()),
        )
    }
}

/// A PubGrub-compatible package and version range.
#[derive(Debug, Clone)]
struct PubGrubRequirement {
    package: PubGrubPackage,
    version: Ranges<Version>,
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
            RequirementSource::GitDirectory {
                git,
                url,
                subdirectory,
            } => {
                let parsed_url = ParsedUrl::GitDirectory(ParsedGitDirectoryUrl::from_source(
                    git.clone(),
                    subdirectory.clone(),
                ));
                (url, parsed_url)
            }
            RequirementSource::GitPath {
                git,
                install_path,
                ext,
                url,
            } => {
                let parsed_url = ParsedUrl::GitPath(ParsedGitPathUrl::from_source(
                    git.clone(),
                    install_path.clone(),
                    *ext,
                ));
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

    fn from_requirement_with_marker(
        requirement: &Requirement,
        extra: Option<ExtraName>,
        group: Option<GroupName>,
        marker: uv_pep508::MarkerTree,
        source: DependencySource,
    ) -> Self {
        let package = PubGrubPackage::from_package_with_marker(
            requirement.name.clone(),
            extra.clone(),
            group.clone(),
            marker,
        );
        let mut requirement = Self::from_requirement(requirement, extra, group);
        requirement.package = package;
        requirement.source = source;
        requirement
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

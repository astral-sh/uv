use std::iter;

use pubgrub::Ranges;
use tracing::warn;

use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pypi_types::{
    ConflictItemRef, ParsedArchiveUrl, ParsedDirectoryUrl, ParsedGitUrl, ParsedPathUrl, ParsedUrl,
    Requirement, RequirementSource, VerbatimParsedUrl,
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
    /// The main problem with deal with project level conflicts is that if you
    /// declare a conflict between a package and a group, we represent that
    /// group as a dependency of that package. So if you filter out the package
    /// in a fork due to a conflict, you also filter out the group. Therefore,
    /// we introduce this parent field to enable "delayed" filtering.
    pub(crate) parent: Option<PackageName>,

    /// The original version specifiers from the requirement.
    pub(crate) specifier: Option<VersionSpecifiers>,

    /// This field is set if the [`Requirement`] had a URL. We still use a URL from [`Urls`]
    /// even if this field is None where there is an override with a URL or there is a different
    /// requirement or constraint for the same package that has a URL.
    pub(crate) url: Option<VerbatimParsedUrl>,
}

impl PubGrubDependency {
    pub(crate) fn from_requirement<'a>(
        requirement: &'a Requirement,
        parent_package: Option<&'a PubGrubPackage>,
    ) -> impl Iterator<Item = Self> + 'a {
        let parent_name = parent_package.and_then(|package| package.name_no_root());
        let is_normal_parent = parent_package
            .map(|pp| pp.extra().is_none() && pp.dev().is_none())
            .unwrap_or(false);
        // Add the package, plus any extra variants.
        iter::once(None)
            .chain(requirement.extras.clone().into_iter().map(Some))
            .map(|extra| PubGrubRequirement::from_requirement(requirement, extra))
            .filter_map(move |requirement| {
                let PubGrubRequirement {
                    package,
                    version,
                    specifier,
                    url,
                } = requirement;
                match &*package {
                    PubGrubPackageInner::Package { name, .. } => {
                        // Detect self-dependencies.
                        if parent_name.is_some_and(|parent_name| parent_name == name) {
                            warn!("{name} has a dependency on itself");
                            return None;
                        }

                        Some(PubGrubDependency {
                            package: package.clone(),
                            version: version.clone(),
                            parent: if is_normal_parent {
                                parent_name.cloned()
                            } else {
                                None
                            },
                            specifier,
                            url,
                        })
                    }
                    PubGrubPackageInner::Marker { .. } => Some(PubGrubDependency {
                        package: package.clone(),
                        version: version.clone(),
                        parent: if is_normal_parent {
                            parent_name.cloned()
                        } else {
                            None
                        },
                        specifier,
                        url,
                    }),
                    PubGrubPackageInner::Extra { name, .. } => {
                        debug_assert!(
                            !parent_name.is_some_and(|parent_name| parent_name == name),
                            "extras not flattened for {name}"
                        );
                        Some(PubGrubDependency {
                            package: package.clone(),
                            version: version.clone(),
                            parent: None,
                            specifier,
                            url,
                        })
                    }
                    _ => None,
                }
            })
    }

    /// Extracts a possible conflicting item from this dependency.
    ///
    /// If this package can't possibly be classified as conflicting, then this
    /// returns `None`.
    pub(crate) fn conflicting_item(&self) -> Option<ConflictItemRef<'_>> {
        if let Some(conflict) = self.package.conflicting_item() {
            return Some(conflict);
        }
        if let Some(ref parent) = self.parent {
            return Some(ConflictItemRef::from(parent));
        }
        None
    }
}

/// A PubGrub-compatible package and version range.
#[derive(Debug, Clone)]
pub(crate) struct PubGrubRequirement {
    pub(crate) package: PubGrubPackage,
    pub(crate) version: Ranges<Version>,
    pub(crate) specifier: Option<VersionSpecifiers>,
    pub(crate) url: Option<VerbatimParsedUrl>,
}

impl PubGrubRequirement {
    /// Convert a [`Requirement`] to a PubGrub-compatible package and range, while returning the URL
    /// on the [`Requirement`], if any.
    pub(crate) fn from_requirement(requirement: &Requirement, extra: Option<ExtraName>) -> Self {
        let (verbatim_url, parsed_url) = match &requirement.source {
            RequirementSource::Registry { specifier, .. } => {
                return Self::from_registry_requirement(specifier, extra, requirement);
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
                requirement.marker.clone(),
            ),
            version: Ranges::full(),
            specifier: None,
            url: Some(VerbatimParsedUrl {
                parsed_url,
                verbatim: verbatim_url.clone(),
            }),
        }
    }

    fn from_registry_requirement(
        specifier: &VersionSpecifiers,
        extra: Option<ExtraName>,
        requirement: &Requirement,
    ) -> PubGrubRequirement {
        Self {
            package: PubGrubPackage::from_package(
                requirement.name.clone(),
                extra,
                requirement.marker.clone(),
            ),
            specifier: Some(specifier.clone()),
            url: None,
            version: Ranges::from(specifier.clone()),
        }
    }
}

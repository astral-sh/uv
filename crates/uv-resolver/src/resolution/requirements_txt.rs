use std::borrow::Cow;
use std::fmt::Display;
use std::path::Path;

use itertools::Itertools;

use distribution_types::{DistributionMetadata, Name, ResolvedDist, Verbatim, VersionOrUrlRef};
use pep440_rs::Version;
use pep508_rs::{split_scheme, MarkerTree, Scheme};
use pypi_types::HashDigest;
use uv_normalize::{ExtraName, PackageName};

use crate::resolution::AnnotatedDist;

#[derive(Debug, Clone)]
/// A pinned package with its resolved distribution and all the extras that were pinned for it.
pub(crate) struct RequirementsTxtDist {
    pub(crate) dist: ResolvedDist,
    pub(crate) version: Version,
    pub(crate) extras: Vec<ExtraName>,
    pub(crate) hashes: Vec<HashDigest>,
    pub(crate) markers: Option<MarkerTree>,
}

impl RequirementsTxtDist {
    /// Convert the [`RequirementsTxtDist`] to a requirement that adheres to the `requirements.txt`
    /// format.
    ///
    /// This typically results in a PEP 508 representation of the requirement, but will write an
    /// unnamed requirement for relative paths, which can't be represented with PEP 508 (but are
    /// supported in `requirements.txt`).
    pub(crate) fn to_requirements_txt(
        &self,
        include_extras: bool,
        include_markers: bool,
    ) -> Cow<str> {
        // If the URL is editable, write it as an editable requirement.
        if self.dist.is_editable() {
            if let VersionOrUrlRef::Url(url) = self.dist.version_or_url() {
                let given = url.verbatim();
                return Cow::Owned(format!("-e {given}"));
            }
        }

        // If the URL is not _definitively_ an absolute `file://` URL, write it as a relative path.
        if self.dist.is_local() {
            if let VersionOrUrlRef::Url(url) = self.dist.version_or_url() {
                let given = url.verbatim();
                let given = match split_scheme(&given) {
                    Some((scheme, path)) => {
                        match Scheme::parse(scheme) {
                            Some(Scheme::File) => {
                                if path
                                    .strip_prefix("//localhost")
                                    .filter(|path| path.starts_with('/'))
                                    .is_some()
                                {
                                    // Always absolute; nothing to do.
                                    None
                                } else if let Some(path) = path.strip_prefix("//") {
                                    // Strip the prefix, to convert, e.g., `file://flask-3.0.3-py3-none-any.whl` to `flask-3.0.3-py3-none-any.whl`.
                                    //
                                    // However, we should allow any of the following:
                                    // - `file:///flask-3.0.3-py3-none-any.whl`
                                    // - `file://C:\Users\user\flask-3.0.3-py3-none-any.whl`
                                    // - `file:///C:\Users\user\flask-3.0.3-py3-none-any.whl`
                                    if !path.starts_with("${PROJECT_ROOT}")
                                        && !Path::new(path).has_root()
                                    {
                                        Some(Cow::Owned(path.to_string()))
                                    } else {
                                        None
                                    }
                                } else {
                                    // Ex) `file:./flask-3.0.3-py3-none-any.whl`
                                    Some(given)
                                }
                            }
                            Some(_) => None,
                            None => {
                                // Ex) `flask @ C:\Users\user\flask-3.0.3-py3-none-any.whl`
                                Some(given)
                            }
                        }
                    }
                    None => {
                        // Ex) `flask @ flask-3.0.3-py3-none-any.whl`
                        Some(given)
                    }
                };
                if let Some(given) = given {
                    return if let Some(markers) = self.markers.as_ref().filter(|_| include_markers)
                    {
                        Cow::Owned(format!("{given} ; {markers}"))
                    } else {
                        given
                    };
                }
            }
        }

        if self.extras.is_empty() || !include_extras {
            if let Some(markers) = self.markers.as_ref().filter(|_| include_markers) {
                Cow::Owned(format!("{} ; {}", self.dist.verbatim(), markers))
            } else {
                self.dist.verbatim()
            }
        } else {
            let mut extras = self.extras.clone();
            extras.sort_unstable();
            extras.dedup();
            if let Some(markers) = self.markers.as_ref().filter(|_| include_markers) {
                Cow::Owned(format!(
                    "{}[{}]{} ; {}",
                    self.name(),
                    extras.into_iter().join(", "),
                    self.version_or_url().verbatim(),
                    markers,
                ))
            } else {
                Cow::Owned(format!(
                    "{}[{}]{}",
                    self.name(),
                    extras.into_iter().join(", "),
                    self.version_or_url().verbatim()
                ))
            }
        }
    }

    pub(crate) fn to_comparator(&self) -> RequirementsTxtComparator {
        if self.dist.is_editable() {
            if let VersionOrUrlRef::Url(url) = self.dist.version_or_url() {
                return RequirementsTxtComparator::Url(url.verbatim());
            }
        }

        if let VersionOrUrlRef::Url(url) = self.version_or_url() {
            RequirementsTxtComparator::Name {
                name: self.name(),
                version: &self.version,
                url: Some(url.verbatim()),
            }
        } else {
            RequirementsTxtComparator::Name {
                name: self.name(),
                version: &self.version,
                url: None,
            }
        }
    }
}

impl From<&AnnotatedDist> for RequirementsTxtDist {
    fn from(annotated: &AnnotatedDist) -> Self {
        Self {
            dist: annotated.dist.clone(),
            version: annotated.version.clone(),
            extras: if let Some(extra) = annotated.extra.clone() {
                vec![extra]
            } else {
                vec![]
            },
            hashes: annotated.hashes.clone(),
            markers: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RequirementsTxtComparator<'a> {
    Url(Cow<'a, str>),
    /// In universal mode, we can have multiple versions for a package, so we track the version and
    /// the URL (for non-index packages) to have a stable sort for those, too.
    Name {
        name: &'a PackageName,
        version: &'a Version,
        url: Option<Cow<'a, str>>,
    },
}

impl Name for RequirementsTxtDist {
    fn name(&self) -> &PackageName {
        self.dist.name()
    }
}

impl DistributionMetadata for RequirementsTxtDist {
    fn version_or_url(&self) -> VersionOrUrlRef {
        self.dist.version_or_url()
    }
}

impl Display for RequirementsTxtDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.dist, f)
    }
}

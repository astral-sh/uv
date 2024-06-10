use std::borrow::Cow;
use std::fmt::Display;
use std::path::Path;

use itertools::Itertools;

use distribution_types::{DistributionMetadata, Name, ResolvedDist, Verbatim, VersionOrUrlRef};
use pep508_rs::{split_scheme, Scheme};
use pypi_types::HashDigest;
use uv_distribution::Metadata;
use uv_normalize::{ExtraName, GroupName, PackageName};

pub use crate::resolution::display::{AnnotationStyle, DisplayResolutionGraph};
pub use crate::resolution::graph::ResolutionGraph;

mod display;
mod graph;

/// A pinned package with its resolved distribution and metadata. The [`ResolvedDist`] refers to a
/// specific distribution (e.g., a specific wheel), while the [`Metadata23`] refers to the metadata
/// for the package-version pair.
#[derive(Debug, Clone)]
pub(crate) struct AnnotatedDist {
    pub(crate) dist: ResolvedDist,
    pub(crate) extra: Option<ExtraName>,
    pub(crate) dev: Option<GroupName>,
    pub(crate) hashes: Vec<HashDigest>,
    pub(crate) metadata: Metadata,
}

impl AnnotatedDist {
    /// Returns `true` if the [`AnnotatedDist`] is a base package (i.e., not an extra or a
    /// dependency group).
    pub(crate) fn is_base(&self) -> bool {
        self.extra.is_none() && self.dev.is_none()
    }
}

impl Name for AnnotatedDist {
    fn name(&self) -> &PackageName {
        self.dist.name()
    }
}

impl DistributionMetadata for AnnotatedDist {
    fn version_or_url(&self) -> VersionOrUrlRef {
        self.dist.version_or_url()
    }
}

impl Display for AnnotatedDist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.dist, f)
    }
}

/// A pinned package with its resolved distribution and all the extras that were pinned for it.
#[derive(Debug, Clone)]
pub(crate) struct RequirementsTxtDist {
    pub(crate) dist: ResolvedDist,
    pub(crate) extras: Vec<ExtraName>,
    pub(crate) hashes: Vec<HashDigest>,
}

impl RequirementsTxtDist {
    /// Convert the [`RequirementsTxtDist`] to a requirement that adheres to the `requirements.txt`
    /// format.
    ///
    /// This typically results in a PEP 508 representation of the requirement, but will write an
    /// unnamed requirement for relative paths, which can't be represented with PEP 508 (but are
    /// supported in `requirements.txt`).
    pub(crate) fn to_requirements_txt(&self, include_extras: bool) -> Cow<str> {
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
                match split_scheme(&given) {
                    Some((scheme, path)) => {
                        match Scheme::parse(scheme) {
                            Some(Scheme::File) => {
                                if path
                                    .strip_prefix("//localhost")
                                    .filter(|path| path.starts_with('/'))
                                    .is_some()
                                {
                                    // Always absolute; nothing to do.
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
                                        return Cow::Owned(path.to_string());
                                    }
                                } else {
                                    // Ex) `file:./flask-3.0.3-py3-none-any.whl`
                                    return given;
                                }
                            }
                            Some(_) => {}
                            None => {
                                // Ex) `flask @ C:\Users\user\flask-3.0.3-py3-none-any.whl`
                                return given;
                            }
                        }
                    }
                    None => {
                        // Ex) `flask @ flask-3.0.3-py3-none-any.whl`
                        return given;
                    }
                }
            }
        }

        if self.extras.is_empty() || !include_extras {
            self.dist.verbatim()
        } else {
            let mut extras = self.extras.clone();
            extras.sort_unstable();
            extras.dedup();
            Cow::Owned(format!(
                "{}[{}]{}",
                self.name(),
                extras.into_iter().join(", "),
                self.version_or_url().verbatim()
            ))
        }
    }

    pub(crate) fn to_comparator(&self) -> RequirementsTxtComparator {
        if self.dist.is_editable() {
            if let VersionOrUrlRef::Url(url) = self.dist.version_or_url() {
                return RequirementsTxtComparator::Url(url.verbatim());
            }
        }

        RequirementsTxtComparator::Name(self.name())
    }
}

impl From<&AnnotatedDist> for RequirementsTxtDist {
    fn from(annotated: &AnnotatedDist) -> Self {
        Self {
            dist: annotated.dist.clone(),
            extras: if let Some(extra) = annotated.extra.clone() {
                vec![extra]
            } else {
                vec![]
            },
            hashes: annotated.hashes.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RequirementsTxtComparator<'a> {
    Url(Cow<'a, str>),
    Name(&'a PackageName),
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

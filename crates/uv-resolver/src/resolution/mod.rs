use std::borrow::Cow;
use std::fmt::Display;
use std::path::Path;

use itertools::Itertools;

use distribution_types::{DistributionMetadata, Name, ResolvedDist, Verbatim, VersionOrUrlRef};
use pypi_types::{HashDigest, Metadata23};
use uv_normalize::{ExtraName, PackageName};

pub use crate::resolution::display::{AnnotationStyle, DisplayResolutionGraph};
pub use crate::resolution::graph::{Diagnostic, ResolutionGraph};

mod display;
mod graph;

/// A pinned package with its resolved distribution and metadata. The [`ResolvedDist`] refers to a
/// specific distribution (e.g., a specific wheel), while the [`Metadata23`] refers to the metadata
/// for the package-version pair.
#[derive(Debug, Clone)]
pub(crate) struct AnnotatedDist {
    pub(crate) dist: ResolvedDist,
    pub(crate) extras: Vec<ExtraName>,
    pub(crate) hashes: Vec<HashDigest>,
    pub(crate) metadata: Metadata23,
}

impl AnnotatedDist {
    /// Convert the [`AnnotatedDist`] to a requirement that adheres to the `requirements.txt`
    /// format.
    ///
    /// This typically results in a PEP 508 representation of the requirement, but will write an
    /// unnamed requirement for relative paths, which can't be represented with PEP 508 (but are
    /// supported in `requirements.txt`).
    pub(crate) fn to_requirements_txt(&self) -> Cow<str> {
        // If the URL is not _definitively_ an absolute `file://` URL, write it as a relative
        // path.
        if let VersionOrUrlRef::Url(url) = self.dist.version_or_url() {
            let given = url.verbatim();
            if !given.strip_prefix("file://").is_some_and(|path| {
                path.starts_with("${PROJECT_ROOT}") || Path::new(path).is_absolute()
            }) {
                return given;
            }
        }

        if self.extras.is_empty() {
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

    /// Return the [`AnnotatedDist`] without any extras.
    pub(crate) fn without_extras(&self) -> Cow<AnnotatedDist> {
        if self.extras.is_empty() {
            Cow::Borrowed(self)
        } else {
            Cow::Owned(AnnotatedDist {
                dist: self.dist.clone(),
                extras: Vec::new(),
                hashes: self.hashes.clone(),
                metadata: self.metadata.clone(),
            })
        }
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

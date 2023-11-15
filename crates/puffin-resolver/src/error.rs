use std::fmt::Formatter;

use pubgrub::range::Range;
use pubgrub::report::Reporter;
use thiserror::Error;
use url::Url;

use pep508_rs::Requirement;
use puffin_distribution::{BuiltDist, SourceDist};
use puffin_normalize::PackageName;

use crate::pubgrub::{PubGrubPackage, PubGrubVersion};
use crate::ResolutionFailureReporter;

#[derive(Error, Debug)]
pub enum ResolveError {
    #[error("Failed to find a version of {0} that satisfies the requirement")]
    NotFound(Requirement),

    #[error("The request stream terminated unexpectedly")]
    StreamTermination,

    #[error(transparent)]
    Client(#[from] puffin_client::Error),

    #[error(transparent)]
    TrySend(#[from] futures::channel::mpsc::SendError),

    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),

    #[error(transparent)]
    PubGrub(#[from] RichPubGrubError),

    #[error("Package metadata name `{metadata}` does not match given name `{given}`")]
    NameMismatch {
        given: PackageName,
        metadata: PackageName,
    },

    #[error("~= operator requires at least two release segments: {0}")]
    InvalidTildeEquals(pep440_rs::VersionSpecifier),

    #[error("Conflicting URLs for package `{0}`:\n- {1}\n- {2}")]
    ConflictingUrls(PackageName, String, String),

    #[error("Conflicting versions for `{0}`: {1}")]
    ConflictingVersions(String, String),

    #[error("Package `{0}` attempted to resolve via URL: {1}. URL dependencies must be expressed as direct requirements or constraints. Consider adding `{0} @ {1}` to your dependencies or constraints file.")]
    DisallowedUrl(PackageName, Url),

    #[error("Failed to fetch wheel metadata from: {filename}")]
    RegistryBuiltDist {
        filename: String,
        // TODO(konstin): Gives this a proper error type
        #[source]
        err: anyhow::Error,
    },

    #[error("Failed to fetch wheel metadata from: {url}")]
    UrlBuiltDist {
        url: Url,
        // TODO(konstin): Gives this a proper error type
        #[source]
        err: anyhow::Error,
    },

    #[error("Failed to build distribution: {filename}")]
    RegistrySourceDist {
        filename: String,
        // TODO(konstin): Gives this a proper error type
        #[source]
        err: anyhow::Error,
    },

    #[error("Failed to build distribution from URL: {url}")]
    UrlSourceDist {
        url: Url,
        // TODO(konstin): Gives this a proper error type
        #[source]
        err: anyhow::Error,
    },
}

impl<T> From<futures::channel::mpsc::TrySendError<T>> for ResolveError {
    fn from(value: futures::channel::mpsc::TrySendError<T>) -> Self {
        value.into_send_error().into()
    }
}

/// A wrapper around [`pubgrub::error::PubGrubError`] that displays a resolution failure report.
#[derive(Debug)]
pub struct RichPubGrubError {
    source: pubgrub::error::PubGrubError<PubGrubPackage, Range<PubGrubVersion>>,
}

impl std::error::Error for RichPubGrubError {}

impl std::fmt::Display for RichPubGrubError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let pubgrub::error::PubGrubError::NoSolution(derivation_tree) = &self.source {
            let report = ResolutionFailureReporter::report(derivation_tree);
            write!(f, "{report}")
        } else {
            write!(f, "{}", self.source)
        }
    }
}

impl From<pubgrub::error::PubGrubError<PubGrubPackage, Range<PubGrubVersion>>> for ResolveError {
    fn from(value: pubgrub::error::PubGrubError<PubGrubPackage, Range<PubGrubVersion>>) -> Self {
        ResolveError::PubGrub(RichPubGrubError { source: value })
    }
}

impl ResolveError {
    pub fn from_source_dist(dist: SourceDist, err: anyhow::Error) -> Self {
        match dist {
            SourceDist::Registry(sdist) => Self::RegistrySourceDist {
                filename: sdist.file.filename.clone(),
                err,
            },
            SourceDist::DirectUrl(sdist) => Self::UrlSourceDist {
                url: sdist.url.clone(),
                err,
            },
            SourceDist::Git(sdist) => Self::UrlSourceDist {
                url: sdist.url.clone(),
                err,
            },
        }
    }

    pub fn from_built_dist(dist: BuiltDist, err: anyhow::Error) -> Self {
        match dist {
            BuiltDist::Registry(wheel) => Self::RegistryBuiltDist {
                filename: wheel.file.filename.clone(),
                err,
            },
            BuiltDist::DirectUrl(wheel) => Self::UrlBuiltDist {
                url: wheel.url.clone(),
                err,
            },
        }
    }
}

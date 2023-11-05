use pubgrub::range::Range;
use thiserror::Error;
use url::Url;

use pep508_rs::Requirement;
use puffin_normalize::PackageName;

use crate::pubgrub::{PubGrubPackage, PubGrubVersion};

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
    PubGrub(#[from] pubgrub::error::PubGrubError<PubGrubPackage, Range<PubGrubVersion>>),

    #[error("Package metadata name `{metadata}` does not match given name `{given}`")]
    NameMismatch {
        given: PackageName,
        metadata: PackageName,
    },

    #[error("~= operator requires at least two release segments: {0}")]
    InvalidTildeEquals(pep440_rs::VersionSpecifier),

    #[error("Conflicting URLs for package `{0}`: {1} and {2}")]
    ConflictingUrls(PackageName, String, String),

    #[error("Package `{0}` attempted to resolve via URL: {1}. URL dependencies must be expressed as direct requirements or constraints. Consider adding `{0} @ {1}` to your dependencies or constraints file.")]
    DisallowedUrl(PackageName, Url),

    #[error("Failed to build distribution: {filename}")]
    RegistryDistribution {
        filename: String,
        // TODO(konstin): Gives this a proper error type
        #[source]
        err: anyhow::Error,
    },

    #[error("Failed to build distribution: {url}")]
    UrlDistribution {
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

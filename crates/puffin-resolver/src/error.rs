use std::fmt::Formatter;

use pubgrub::range::Range;
use pubgrub::report::{DefaultStringReporter, Reporter};
use thiserror::Error;
use url::Url;

use distribution_types::{BuiltDist, SourceDist};
use pep508_rs::Requirement;
use puffin_distribution::DistributionDatabaseError;
use puffin_normalize::PackageName;

use crate::pubgrub::{PubGrubPackage, PubGrubVersion};
use crate::PubGrubReportFormatter;

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

    #[error(transparent)]
    DistributionType(#[from] distribution_types::Error),

    #[error("Failed to download {0}")]
    Fetch(Box<BuiltDist>, #[source] DistributionDatabaseError),

    #[error("Failed to download and build {0}")]
    FetchAndBuild(Box<SourceDist>, #[source] DistributionDatabaseError),
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
            let formatter = PubGrubReportFormatter;
            let report = DefaultStringReporter::report_with_formatter(derivation_tree, &formatter);
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

use std::convert::Infallible;
use std::fmt::Formatter;

use pubgrub::range::Range;
use pubgrub::report::{DefaultStringReporter, DerivationTree, Reporter};
use rustc_hash::FxHashMap;
use thiserror::Error;
use url::Url;

use distribution_types::{BuiltDist, IndexUrl, PathBuiltDist, PathSourceDist, SourceDist};
use pep508_rs::Requirement;
use puffin_distribution::DistributionDatabaseError;
use puffin_normalize::PackageName;
use puffin_traits::OnceMap;
use pypi_types::BaseUrl;

use crate::candidate_selector::CandidateSelector;
use crate::pubgrub::{PubGrubHints, PubGrubPackage, PubGrubReportFormatter, PubGrubVersion};
use crate::version_map::VersionMap;

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

    #[error("Failed to download: {0}")]
    Fetch(Box<BuiltDist>, #[source] DistributionDatabaseError),

    #[error("Failed to download and build: {0}")]
    FetchAndBuild(Box<SourceDist>, #[source] DistributionDatabaseError),

    #[error("Failed to read: {0}")]
    Read(Box<PathBuiltDist>, #[source] DistributionDatabaseError),

    #[error("Failed to build: {0}")]
    Build(Box<PathSourceDist>, #[source] DistributionDatabaseError),

    #[error(transparent)]
    NoSolution(#[from] NoSolutionError),

    #[error("{package} {version} depends on itself")]
    SelfDependency {
        /// Package whose dependencies we want.
        package: Box<PubGrubPackage>,
        /// Version of the package for which we want the dependencies.
        version: Box<PubGrubVersion>,
    },

    /// Something unexpected happened.
    #[error("{0}")]
    Failure(String),
}

impl<T> From<futures::channel::mpsc::TrySendError<T>> for ResolveError {
    fn from(value: futures::channel::mpsc::TrySendError<T>) -> Self {
        value.into_send_error().into()
    }
}

impl From<pubgrub::error::PubGrubError<PubGrubPackage, Range<PubGrubVersion>, Infallible>>
    for ResolveError
{
    fn from(
        value: pubgrub::error::PubGrubError<PubGrubPackage, Range<PubGrubVersion>, Infallible>,
    ) -> Self {
        match value {
            // These are all never type variant that can never match, but never is experimental
            pubgrub::error::PubGrubError::ErrorChoosingPackageVersion(_)
            | pubgrub::error::PubGrubError::ErrorInShouldCancel(_)
            | pubgrub::error::PubGrubError::ErrorRetrievingDependencies { .. } => {
                unreachable!()
            }
            pubgrub::error::PubGrubError::Failure(inner) => ResolveError::Failure(inner),
            pubgrub::error::PubGrubError::NoSolution(derivation_tree) => {
                ResolveError::NoSolution(NoSolutionError {
                    derivation_tree,
                    available_versions: FxHashMap::default(),
                    selector: None,
                })
            }
            pubgrub::error::PubGrubError::SelfDependency { package, version } => {
                ResolveError::SelfDependency {
                    package: Box::new(package),
                    version: Box::new(version),
                }
            }
        }
    }
}

/// A wrapper around [`pubgrub::error::PubGrubError::NoSolution`] that displays a resolution failure report.
#[derive(Debug)]
pub struct NoSolutionError {
    derivation_tree: DerivationTree<PubGrubPackage, Range<PubGrubVersion>>,
    available_versions: FxHashMap<PubGrubPackage, Vec<PubGrubVersion>>,
    selector: Option<CandidateSelector>,
}

impl std::error::Error for NoSolutionError {}

impl std::fmt::Display for NoSolutionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Write the derivation report.
        let formatter = PubGrubReportFormatter {
            available_versions: &self.available_versions,
        };
        let report =
            DefaultStringReporter::report_with_formatter(&self.derivation_tree, &formatter);
        write!(f, "{report}")?;

        // Include any additional hints.
        if let Some(selector) = &self.selector {
            for hint in PubGrubHints::from_derivation_tree(&self.derivation_tree, selector).iter() {
                write!(f, "\n\n{hint}")?;
            }
        }

        Ok(())
    }
}

impl NoSolutionError {
    /// Update the available versions attached to the error using the given package version index.
    ///
    /// Only packages used in the error's derivation tree will be retrieved.
    #[must_use]
    pub(crate) fn with_available_versions(
        mut self,
        package_versions: &OnceMap<PackageName, (IndexUrl, BaseUrl, VersionMap)>,
    ) -> Self {
        let mut available_versions = FxHashMap::default();
        for package in self.derivation_tree.packages() {
            if let PubGrubPackage::Package(name, ..) = package {
                if let Some(entry) = package_versions.get(name) {
                    let (_, _, version_map) = entry.value();
                    available_versions.insert(
                        package.clone(),
                        version_map
                            .iter()
                            .map(|(version, _)| version.clone())
                            .collect(),
                    );
                }
            }
        }
        self.available_versions = available_versions;
        self
    }

    /// Update the candidate selector attached to the error.
    #[must_use]
    pub(crate) fn with_selector(mut self, selector: CandidateSelector) -> Self {
        self.selector = Some(selector);
        self
    }
}

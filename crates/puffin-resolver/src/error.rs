use std::fmt::Formatter;

use fxhash::FxHashMap;
use pubgrub::range::Range;
use pubgrub::report::{DefaultStringReporter, DerivationTree, Reporter};
use pypi_types::IndexUrl;
use thiserror::Error;
use url::Url;

use distribution_types::{BuiltDist, PathBuiltDist, PathSourceDist, SourceDist};
use pep508_rs::Requirement;
use puffin_distribution::DistributionDatabaseError;
use puffin_normalize::PackageName;
use waitmap::WaitMap;

use crate::pubgrub::{PubGrubPackage, PubGrubVersion};
use crate::version_map::VersionMap;
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

    #[error("Retrieving dependencies of {package} {version} failed")]
    ErrorRetrievingDependencies {
        /// Package whose dependencies we want.
        package: PubGrubPackage,
        /// Version of the package for which we want the dependencies.
        version: PubGrubVersion,
        /// Error raised by the implementer of
        /// [DependencyProvider](crate::solver::DependencyProvider).
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("{package} {version} depends on itself")]
    SelfDependency {
        /// Package whose dependencies we want.
        package: PubGrubPackage,
        /// Version of the package for which we want the dependencies.
        version: PubGrubVersion,
    },

    #[error("Decision making failed")]
    ErrorChoosingPackageVersion(Box<dyn std::error::Error + Send + Sync>),

    #[error("We should cancel")]
    ErrorInShouldCancel(Box<dyn std::error::Error + Send + Sync>),

    /// Something unexpected happened.
    #[error("{0}")]
    Failure(String),
}

impl<T> From<futures::channel::mpsc::TrySendError<T>> for ResolveError {
    fn from(value: futures::channel::mpsc::TrySendError<T>) -> Self {
        value.into_send_error().into()
    }
}

impl From<pubgrub::error::PubGrubError<PubGrubPackage, Range<PubGrubVersion>>> for ResolveError {
    fn from(value: pubgrub::error::PubGrubError<PubGrubPackage, Range<PubGrubVersion>>) -> Self {
        match value {
            pubgrub::error::PubGrubError::ErrorChoosingPackageVersion(inner) => {
                ResolveError::ErrorChoosingPackageVersion(inner)
            }
            pubgrub::error::PubGrubError::ErrorInShouldCancel(inner) => {
                ResolveError::ErrorInShouldCancel(inner)
            }
            pubgrub::error::PubGrubError::ErrorRetrievingDependencies {
                package,
                version,
                source,
            } => ResolveError::ErrorRetrievingDependencies {
                package,
                version,
                source,
            },
            pubgrub::error::PubGrubError::Failure(inner) => ResolveError::Failure(inner),
            pubgrub::error::PubGrubError::NoSolution(derivation_tree) => {
                ResolveError::NoSolution(NoSolutionError {
                    derivation_tree,
                    available_versions: FxHashMap::default(),
                })
            }
            pubgrub::error::PubGrubError::SelfDependency { package, version } => {
                ResolveError::SelfDependency { package, version }
            }
        }
    }
}

/// A wrapper around [`pubgrub::error::PubGrubError::NoSolution`] that displays a resolution failure report.
#[derive(Debug)]
pub struct NoSolutionError {
    derivation_tree: DerivationTree<PubGrubPackage, Range<PubGrubVersion>>,
    available_versions: FxHashMap<PubGrubPackage, Vec<PubGrubVersion>>,
}

impl std::error::Error for NoSolutionError {}

impl std::fmt::Display for NoSolutionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let formatter = PubGrubReportFormatter {
            available_versions: &self.available_versions,
        };
        let report =
            DefaultStringReporter::report_with_formatter(&self.derivation_tree, &formatter);
        write!(f, "{report}")
    }
}

impl NoSolutionError {
    pub fn update_available_versions<'a>(
        mut self,
        package_versions: &'a WaitMap<PackageName, (IndexUrl, VersionMap)>,
    ) -> Self {
        for package in self.derivation_tree.packages() {
            if let PubGrubPackage::Package(name, ..) = package {
                if let Some(entry) = package_versions.get(name) {
                    let (_, version_map) = entry.value();
                    self.available_versions.insert(
                        package.clone(),
                        version_map
                            .iter()
                            .map(|(version, _)| version.clone())
                            .collect(),
                    );
                }
            }
        }
        self
    }
}

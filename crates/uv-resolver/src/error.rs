use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Formatter;
use std::sync::Arc;

use pubgrub::{DefaultStringReporter, DerivationTree, Derived, External, Range, Reporter};
use rustc_hash::FxHashMap;

use distribution_types::{BuiltDist, IndexLocations, InstalledDist, SourceDist};
use pep440_rs::Version;
use pep508_rs::{MarkerTree, Requirement};
use uv_normalize::PackageName;

use crate::candidate_selector::CandidateSelector;
use crate::dependency_provider::UvDependencyProvider;
use crate::fork_urls::ForkUrls;
use crate::pubgrub::{PubGrubPackage, PubGrubReportFormatter, PubGrubSpecifierError};
use crate::python_requirement::PythonRequirement;
use crate::resolver::{IncompletePackage, ResolverMarkers, UnavailablePackage, UnavailableReason};

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("Failed to find a version of `{0}` that satisfies the requirement")]
    NotFound(Requirement),

    #[error(transparent)]
    Client(#[from] uv_client::Error),

    #[error("The channel closed unexpectedly")]
    ChannelClosed,

    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),

    #[error("Attempted to wait on an unregistered task: `{_0}`")]
    UnregisteredTask(String),

    #[error("Package metadata name `{metadata}` does not match given name `{given}`")]
    NameMismatch {
        given: PackageName,
        metadata: PackageName,
    },

    #[error(transparent)]
    PubGrubSpecifier(#[from] PubGrubSpecifierError),

    #[error("Overrides contain conflicting URLs for package `{0}`:\n- {1}\n- {2}")]
    ConflictingOverrideUrls(PackageName, String, String),

    #[error("Requirements contain conflicting URLs for package `{0}`:\n- {}", _1.join("\n- "))]
    ConflictingUrlsUniversal(PackageName, Vec<String>),

    #[error("Requirements contain conflicting URLs for package `{package_name}` in split `{fork_markers}`:\n- {}", urls.join("\n- "))]
    ConflictingUrlsFork {
        package_name: PackageName,
        urls: Vec<String>,
        fork_markers: MarkerTree,
    },

    #[error("Package `{0}` attempted to resolve via URL: {1}. URL dependencies must be expressed as direct requirements or constraints. Consider adding `{0} @ {1}` to your dependencies or constraints file.")]
    DisallowedUrl(PackageName, String),

    #[error("There are conflicting editable requirements for package `{0}`:\n- {1}\n- {2}")]
    ConflictingEditables(PackageName, String, String),

    #[error(transparent)]
    DistributionType(#[from] distribution_types::Error),

    #[error(transparent)]
    ParsedUrl(#[from] pypi_types::ParsedUrlError),

    #[error("Failed to download `{0}`")]
    Fetch(Box<BuiltDist>, #[source] uv_distribution::Error),

    #[error("Failed to download and build `{0}`")]
    FetchAndBuild(Box<SourceDist>, #[source] uv_distribution::Error),

    #[error("Failed to read `{0}`")]
    Read(Box<BuiltDist>, #[source] uv_distribution::Error),

    // TODO(zanieb): Use `thiserror` in `InstalledDist` so we can avoid chaining `anyhow`
    #[error("Failed to read metadata from installed package `{0}`")]
    ReadInstalled(Box<InstalledDist>, #[source] anyhow::Error),

    #[error("Failed to build `{0}`")]
    Build(Box<SourceDist>, #[source] uv_distribution::Error),

    #[error(transparent)]
    NoSolution(#[from] NoSolutionError),

    #[error("{package} {version} depends on itself")]
    SelfDependency {
        /// Package whose dependencies we want.
        package: Box<PubGrubPackage>,
        /// Version of the package for which we want the dependencies.
        version: Box<Version>,
    },

    #[error("Attempted to construct an invalid version specifier")]
    InvalidVersion(#[from] pep440_rs::VersionSpecifierBuildError),

    #[error("In `--require-hashes` mode, all requirements must be pinned upfront with `==`, but found: `{0}`")]
    UnhashedPackage(PackageName),

    /// Something unexpected happened.
    #[error("{0}")]
    Failure(String),
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for ResolveError {
    /// Drop the value we want to send to not leak the private type we're sending.
    /// The tokio error only says "channel closed", so we don't lose information.
    fn from(_value: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Self::ChannelClosed
    }
}

pub(crate) type ErrorTree = DerivationTree<PubGrubPackage, Range<Version>, UnavailableReason>;

/// A wrapper around [`pubgrub::error::NoSolutionError`] that displays a resolution failure report.
#[derive(Debug)]
pub struct NoSolutionError {
    error: pubgrub::NoSolutionError<UvDependencyProvider>,
    available_versions: FxHashMap<PubGrubPackage, BTreeSet<Version>>,
    selector: CandidateSelector,
    python_requirement: PythonRequirement,
    index_locations: IndexLocations,
    unavailable_packages: FxHashMap<PackageName, UnavailablePackage>,
    incomplete_packages: FxHashMap<PackageName, BTreeMap<Version, IncompletePackage>>,
    fork_urls: ForkUrls,
    markers: ResolverMarkers,
}

impl NoSolutionError {
    pub fn header(&self) -> String {
        match &self.markers {
            ResolverMarkers::Universal { .. } | ResolverMarkers::SpecificEnvironment(_) => {
                "No solution found when resolving dependencies:".to_string()
            }
            ResolverMarkers::Fork(markers) => {
                format!("No solution found when resolving dependencies for split ({markers}):")
            }
        }
    }

    pub(crate) fn new(
        error: pubgrub::NoSolutionError<UvDependencyProvider>,
        available_versions: FxHashMap<PubGrubPackage, BTreeSet<Version>>,
        selector: CandidateSelector,
        python_requirement: PythonRequirement,
        index_locations: IndexLocations,
        unavailable_packages: FxHashMap<PackageName, UnavailablePackage>,
        incomplete_packages: FxHashMap<PackageName, BTreeMap<Version, IncompletePackage>>,
        fork_urls: ForkUrls,
        markers: ResolverMarkers,
    ) -> Self {
        Self {
            error,
            available_versions,
            selector,
            python_requirement,
            index_locations,
            unavailable_packages,
            incomplete_packages,
            fork_urls,
            markers,
        }
    }

    /// Given a [`DerivationTree`], collapse any [`External::FromDependencyOf`] incompatibilities
    /// wrap an [`PubGrubPackageInner::Extra`] package.
    pub(crate) fn collapse_proxies(derivation_tree: ErrorTree) -> ErrorTree {
        fn collapse(derivation_tree: ErrorTree) -> Option<ErrorTree> {
            match derivation_tree {
                DerivationTree::Derived(derived) => {
                    match (&*derived.cause1, &*derived.cause2) {
                        (
                            DerivationTree::External(External::FromDependencyOf(package1, ..)),
                            DerivationTree::External(External::FromDependencyOf(package2, ..)),
                        ) if package1.is_proxy() && package2.is_proxy() => None,
                        (
                            DerivationTree::External(External::FromDependencyOf(package, ..)),
                            cause,
                        ) if package.is_proxy() => collapse(cause.clone()),
                        (
                            cause,
                            DerivationTree::External(External::FromDependencyOf(package, ..)),
                        ) if package.is_proxy() => collapse(cause.clone()),
                        (cause1, cause2) => {
                            let cause1 = collapse(cause1.clone());
                            let cause2 = collapse(cause2.clone());
                            match (cause1, cause2) {
                                (Some(cause1), Some(cause2)) => {
                                    Some(DerivationTree::Derived(Derived {
                                        cause1: Arc::new(cause1),
                                        cause2: Arc::new(cause2),
                                        ..derived
                                    }))
                                }
                                (Some(cause), None) | (None, Some(cause)) => Some(cause),
                                _ => None,
                            }
                        }
                    }
                }
                DerivationTree::External(_) => Some(derivation_tree),
            }
        }

        collapse(derivation_tree)
            .expect("derivation tree should contain at least one external term")
    }
}

impl std::error::Error for NoSolutionError {}

impl std::fmt::Display for NoSolutionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Write the derivation report.
        let formatter = PubGrubReportFormatter {
            available_versions: &self.available_versions,
            python_requirement: &self.python_requirement,
        };
        let report = DefaultStringReporter::report_with_formatter(&self.error, &formatter);
        write!(f, "{report}")?;

        // Include any additional hints.
        for hint in formatter.hints(
            &self.error,
            &self.selector,
            &self.index_locations,
            &self.unavailable_packages,
            &self.incomplete_packages,
            &self.fork_urls,
            &self.markers,
        ) {
            write!(f, "\n\n{hint}")?;
        }

        Ok(())
    }
}

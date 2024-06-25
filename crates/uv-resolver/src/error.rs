use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Formatter;
use std::sync::Arc;

use dashmap::DashMap;
use pubgrub::range::Range;
use pubgrub::report::{DefaultStringReporter, DerivationTree, External, Reporter};
use rustc_hash::{FxHashMap, FxHashSet};

use distribution_types::{BuiltDist, IndexLocations, InstalledDist, SourceDist};
use pep440_rs::Version;
use pep508_rs::{MarkerTree, Requirement};
use uv_normalize::PackageName;

use crate::candidate_selector::CandidateSelector;
use crate::dependency_provider::UvDependencyProvider;
use crate::fork_urls::ForkUrls;
use crate::pubgrub::{
    PubGrubPackage, PubGrubPackageInner, PubGrubReportFormatter, PubGrubSpecifierError,
};
use crate::python_requirement::PythonRequirement;
use crate::resolver::{
    FxOnceMap, IncompletePackage, UnavailablePackage, UnavailableReason, VersionsResponse,
};

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
    ConflictingUrls(PackageName, Vec<String>),

    #[error("Requirements contain conflicting URLs for package `{package_name}` in split `{fork_markers}`:\n- {}", urls.join("\n- "))]
    ConflictingUrlsInFork {
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

/// Given a [`DerivationTree`], collapse any [`External::FromDependencyOf`] incompatibilities
/// wrap an [`PubGrubPackageInner::Extra`] package.
fn collapse_proxies(
    derivation_tree: &mut DerivationTree<PubGrubPackage, Range<Version>, UnavailableReason>,
) {
    match derivation_tree {
        DerivationTree::External(_) => {}
        DerivationTree::Derived(derived) => {
            match (
                Arc::make_mut(&mut derived.cause1),
                Arc::make_mut(&mut derived.cause2),
            ) {
                (
                    DerivationTree::External(External::FromDependencyOf(package, ..)),
                    ref mut cause,
                ) if matches!(
                    &**package,
                    PubGrubPackageInner::Extra { .. }
                        | PubGrubPackageInner::Marker { .. }
                        | PubGrubPackageInner::Dev { .. }
                ) =>
                {
                    collapse_proxies(cause);
                    *derivation_tree = cause.clone();
                }
                (
                    ref mut cause,
                    DerivationTree::External(External::FromDependencyOf(package, ..)),
                ) if matches!(
                    &**package,
                    PubGrubPackageInner::Extra { .. }
                        | PubGrubPackageInner::Marker { .. }
                        | PubGrubPackageInner::Dev { .. }
                ) =>
                {
                    collapse_proxies(cause);
                    *derivation_tree = cause.clone();
                }
                _ => {
                    collapse_proxies(Arc::make_mut(&mut derived.cause1));
                    collapse_proxies(Arc::make_mut(&mut derived.cause2));
                }
            }
        }
    }
}

impl ResolveError {
    /// Convert an error from PubGrub to a resolver error.
    ///
    /// [`ForkUrls`] breaks the usual pattern used here since it's part of one the [`SolveState`],
    /// not of the [`ResolverState`], so we have to take it from the fork that errored instead of
    /// being able to add it later.
    pub(crate) fn from_pubgrub_error(
        value: pubgrub::error::PubGrubError<UvDependencyProvider>,
        fork_urls: ForkUrls,
    ) -> Self {
        match value {
            // These are all never type variant that can never match, but never is experimental
            pubgrub::error::PubGrubError::ErrorChoosingPackageVersion(_)
            | pubgrub::error::PubGrubError::ErrorInShouldCancel(_)
            | pubgrub::error::PubGrubError::ErrorRetrievingDependencies { .. } => {
                unreachable!()
            }
            pubgrub::error::PubGrubError::Failure(inner) => Self::Failure(inner),
            pubgrub::error::PubGrubError::NoSolution(mut derivation_tree) => {
                collapse_proxies(&mut derivation_tree);

                Self::NoSolution(NoSolutionError {
                    derivation_tree,
                    // The following should be populated before display for the best error messages
                    available_versions: FxHashMap::default(),
                    selector: None,
                    python_requirement: None,
                    index_locations: None,
                    unavailable_packages: FxHashMap::default(),
                    incomplete_packages: FxHashMap::default(),
                    fork_urls,
                })
            }
            pubgrub::error::PubGrubError::SelfDependency { package, version } => {
                Self::SelfDependency {
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
    derivation_tree: DerivationTree<PubGrubPackage, Range<Version>, UnavailableReason>,
    available_versions: FxHashMap<PubGrubPackage, BTreeSet<Version>>,
    selector: Option<CandidateSelector>,
    python_requirement: Option<PythonRequirement>,
    index_locations: Option<IndexLocations>,
    unavailable_packages: FxHashMap<PackageName, UnavailablePackage>,
    incomplete_packages: FxHashMap<PackageName, BTreeMap<Version, IncompletePackage>>,
    fork_urls: ForkUrls,
}

impl std::error::Error for NoSolutionError {}

impl std::fmt::Display for NoSolutionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Write the derivation report.
        let formatter = PubGrubReportFormatter {
            available_versions: &self.available_versions,
            python_requirement: self.python_requirement.as_ref(),
        };
        let report =
            DefaultStringReporter::report_with_formatter(&self.derivation_tree, &formatter);
        write!(f, "{report}")?;

        // Include any additional hints.
        for hint in formatter.hints(
            &self.derivation_tree,
            &self.selector,
            &self.index_locations,
            &self.unavailable_packages,
            &self.incomplete_packages,
            &self.fork_urls,
        ) {
            write!(f, "\n\n{hint}")?;
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
        visited: &FxHashSet<PackageName>,
        package_versions: &FxOnceMap<PackageName, Arc<VersionsResponse>>,
    ) -> Self {
        let mut available_versions = FxHashMap::default();
        for package in self.derivation_tree.packages() {
            match &**package {
                PubGrubPackageInner::Root { .. } => {}
                PubGrubPackageInner::Python { .. } => {}
                PubGrubPackageInner::Marker { .. } => {}
                PubGrubPackageInner::Extra { .. } => {}
                PubGrubPackageInner::Dev { .. } => {}
                PubGrubPackageInner::Package { name, .. } => {
                    // Avoid including available versions for packages that exist in the derivation
                    // tree, but were never visited during resolution. We _may_ have metadata for
                    // these packages, but it's non-deterministic, and omitting them ensures that
                    // we represent the state of the resolver at the time of failure.
                    if visited.contains(name) {
                        if let Some(response) = package_versions.get(name) {
                            if let VersionsResponse::Found(ref version_maps) = *response {
                                for version_map in version_maps {
                                    available_versions
                                        .entry(package.clone())
                                        .or_insert_with(BTreeSet::new)
                                        .extend(
                                            version_map.iter().map(|(version, _)| version.clone()),
                                        );
                                }
                            }
                        }
                    }
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

    /// Update the index locations attached to the error.
    #[must_use]
    pub(crate) fn with_index_locations(mut self, index_locations: &IndexLocations) -> Self {
        self.index_locations = Some(index_locations.clone());
        self
    }

    /// Update the unavailable packages attached to the error.
    #[must_use]
    pub(crate) fn with_unavailable_packages(
        mut self,
        unavailable_packages: &DashMap<PackageName, UnavailablePackage>,
    ) -> Self {
        let mut new = FxHashMap::default();
        for package in self.derivation_tree.packages() {
            if let PubGrubPackageInner::Package { name, .. } = &**package {
                if let Some(reason) = unavailable_packages.get(name) {
                    new.insert(name.clone(), reason.clone());
                }
            }
        }
        self.unavailable_packages = new;
        self
    }

    /// Update the incomplete packages attached to the error.
    #[must_use]
    pub(crate) fn with_incomplete_packages(
        mut self,
        incomplete_packages: &DashMap<PackageName, DashMap<Version, IncompletePackage>>,
    ) -> Self {
        let mut new = FxHashMap::default();
        for package in self.derivation_tree.packages() {
            if let PubGrubPackageInner::Package { name, .. } = &**package {
                if let Some(versions) = incomplete_packages.get(name) {
                    for entry in versions.iter() {
                        let (version, reason) = entry.pair();
                        new.entry(name.clone())
                            .or_insert_with(BTreeMap::default)
                            .insert(version.clone(), reason.clone());
                    }
                }
            }
        }
        self.incomplete_packages = new;
        self
    }

    /// Update the Python requirements attached to the error.
    #[must_use]
    pub(crate) fn with_python_requirement(
        mut self,
        python_requirement: &PythonRequirement,
    ) -> Self {
        self.python_requirement = Some(python_requirement.clone());
        self
    }
}

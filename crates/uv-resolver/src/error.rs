use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Formatter;
use std::ops::Deref;
use std::sync::Arc;

use dashmap::{DashMap, DashSet};
use indexmap::IndexMap;
use pubgrub::range::Range;
use pubgrub::report::{DefaultStringReporter, DerivationTree, External, Reporter};
use rustc_hash::FxHashMap;

use distribution_types::{
    BuiltDist, IndexLocations, InstalledDist, ParsedUrlError, PathBuiltDist, PathSourceDist,
    SourceDist,
};
use once_map::OnceMap;
use pep440_rs::Version;
use pep508_rs::Requirement;
use uv_normalize::PackageName;

use crate::candidate_selector::CandidateSelector;
use crate::dependency_provider::UvDependencyProvider;
use crate::pubgrub::{PubGrubPackage, PubGrubPython, PubGrubReportFormatter};
use crate::python_requirement::PythonRequirement;
use crate::resolver::{IncompletePackage, UnavailablePackage, VersionsResponse};

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

    #[error("Attempted to wait on an unregistered task")]
    Unregistered,

    #[error("Package metadata name `{metadata}` does not match given name `{given}`")]
    NameMismatch {
        given: PackageName,
        metadata: PackageName,
    },

    #[error("~= operator requires at least two release segments: `{0}`")]
    InvalidTildeEquals(pep440_rs::VersionSpecifier),

    #[error("Requirements contain conflicting URLs for package `{0}`:\n- {1}\n- {2}")]
    ConflictingUrlsDirect(PackageName, String, String),

    #[error("There are conflicting URLs for package `{0}`:\n- {1}\n- {2}")]
    ConflictingUrlsTransitive(PackageName, String, String),

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
    Read(Box<PathBuiltDist>, #[source] uv_distribution::Error),

    // TODO(zanieb): Use `thiserror` in `InstalledDist` so we can avoid chaining `anyhow`
    #[error("Failed to read metadata from installed package `{0}`")]
    ReadInstalled(Box<InstalledDist>, #[source] anyhow::Error),

    #[error("Failed to build `{0}`")]
    Build(Box<PathSourceDist>, #[source] uv_distribution::Error),

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

    // TODO(konsti): Error source
    #[error("Failed to parse requirements")]
    DirectUrl(#[from] Box<ParsedUrlError>),

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
/// wrap an [`PubGrubPackage::Extra`] package.
fn collapse_extra_proxies(derivation_tree: &mut DerivationTree<PubGrubPackage, Range<Version>>) {
    match derivation_tree {
        DerivationTree::External(_) => {}
        DerivationTree::Derived(derived) => {
            match (
                Arc::make_mut(&mut derived.cause1),
                Arc::make_mut(&mut derived.cause2),
            ) {
                (
                    DerivationTree::External(External::FromDependencyOf(
                        PubGrubPackage::Extra(..),
                        ..,
                    )),
                    ref mut cause,
                ) => {
                    collapse_extra_proxies(cause);
                    *derivation_tree = cause.clone();
                }
                (
                    ref mut cause,
                    DerivationTree::External(External::FromDependencyOf(
                        PubGrubPackage::Extra(..),
                        ..,
                    )),
                ) => {
                    collapse_extra_proxies(cause);
                    *derivation_tree = cause.clone();
                }
                _ => {
                    collapse_extra_proxies(Arc::make_mut(&mut derived.cause1));
                    collapse_extra_proxies(Arc::make_mut(&mut derived.cause2));
                }
            }
        }
    }
}

impl From<pubgrub::error::PubGrubError<UvDependencyProvider>> for ResolveError {
    fn from(value: pubgrub::error::PubGrubError<UvDependencyProvider>) -> Self {
        match value {
            // These are all never type variant that can never match, but never is experimental
            pubgrub::error::PubGrubError::ErrorChoosingPackageVersion(_)
            | pubgrub::error::PubGrubError::ErrorInShouldCancel(_)
            | pubgrub::error::PubGrubError::ErrorRetrievingDependencies { .. } => {
                unreachable!()
            }
            pubgrub::error::PubGrubError::Failure(inner) => Self::Failure(inner),
            pubgrub::error::PubGrubError::NoSolution(mut derivation_tree) => {
                collapse_extra_proxies(&mut derivation_tree);

                Self::NoSolution(NoSolutionError {
                    derivation_tree,
                    // The following should be populated before display for the best error messages
                    available_versions: IndexMap::default(),
                    selector: None,
                    python_requirement: None,
                    index_locations: None,
                    unavailable_packages: FxHashMap::default(),
                    incomplete_packages: FxHashMap::default(),
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
    derivation_tree: DerivationTree<PubGrubPackage, Range<Version>>,
    available_versions: IndexMap<PubGrubPackage, BTreeSet<Version>>,
    selector: Option<CandidateSelector>,
    python_requirement: Option<PythonRequirement>,
    index_locations: Option<IndexLocations>,
    unavailable_packages: FxHashMap<PackageName, UnavailablePackage>,
    incomplete_packages: FxHashMap<PackageName, BTreeMap<Version, IncompletePackage>>,
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
        python_requirement: &PythonRequirement,
        visited: &DashSet<PackageName>,
        package_versions: &OnceMap<PackageName, Arc<VersionsResponse>>,
    ) -> Self {
        let mut available_versions = IndexMap::default();
        for package in self.derivation_tree.packages() {
            match package {
                PubGrubPackage::Root(_) => {}
                PubGrubPackage::Python(PubGrubPython::Installed) => {
                    available_versions.insert(
                        package.clone(),
                        BTreeSet::from([python_requirement.installed().deref().clone()]),
                    );
                }
                PubGrubPackage::Python(PubGrubPython::Target) => {
                    available_versions.insert(
                        package.clone(),
                        BTreeSet::from([python_requirement.target().deref().clone()]),
                    );
                }
                PubGrubPackage::Extra(_, _, _) => {}
                PubGrubPackage::Package(name, _, _) => {
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
            if let PubGrubPackage::Package(name, _, _) = package {
                if let Some(entry) = unavailable_packages.get(name) {
                    let reason = entry.value();
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
            if let PubGrubPackage::Package(name, _, _) = package {
                if let Some(entry) = incomplete_packages.get(name) {
                    let versions = entry.value();
                    for entry in versions {
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

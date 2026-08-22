use std::collections::{BTreeMap, BTreeSet, Bound};
use std::fmt::{Debug, Formatter};
use std::ops::Deref;
use std::sync::{Arc, OnceLock};

use indexmap::IndexSet;
use itertools::Itertools;
use owo_colors::OwoColorize;
use pubgrub::{DerivationTree, Derived, External, Map, Ranges, Term};
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::trace;

use uv_distribution_types::{
    DerivationChain, DistErrorKind, IndexCapabilities, IndexLocations, IndexUrl, RequestedDist,
};
use uv_normalize::{ExtraName, InvalidNameError, PackageName};
use uv_pep440::{LowerBound, Version};
use uv_pep508::MarkerEnvironment;
use uv_platform_tags::Tags;
use uv_pypi_types::ParsedUrl;
use uv_redacted::DisplaySafeUrl;
use uv_static::EnvVars;

use crate::candidate_selector::CandidateSelector;
use crate::dependency_provider::UvDependencyProvider;
use crate::fork_indexes::ForkIndexes;
use crate::fork_urls::ForkUrls;
use crate::prerelease::PrereleaseSelection;
use crate::pubgrub::{
    PubGrubHint, PubGrubPackage, PubGrubPackageInner, PubGrubReportFormatter, Range,
    report_derivation_tree,
};
use crate::python_requirement::PythonRequirement;
use crate::resolution::ConflictingDistributionError;
use crate::resolver::{
    MetadataUnavailable, ResolverEnvironment, UnavailablePackage, UnavailableReason,
};
use crate::{InMemoryIndex, Options};

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("Failed to resolve dependencies for package `{1}=={2}`")]
    Dependencies(#[source] Box<Self>, PackageName, Version, DerivationChain),

    #[error(transparent)]
    Client(#[from] uv_client::Error),

    #[error(transparent)]
    Distribution(#[from] uv_distribution::Error),

    #[error("The channel closed unexpectedly")]
    ChannelClosed,

    #[error("Attempted to wait on an unregistered task: `{_0}`")]
    UnregisteredTask(String),

    #[error(
        "Requirements contain conflicting URLs for package `{package_name}`{}:\n- {}",
        if env.marker_environment().is_some() {
            String::new()
        } else {
            format!(" in {env}")
        },
        urls.iter()
            .map(|url| format!("{}{}", DisplaySafeUrl::from(url.clone()), if url.is_editable() { " (editable)" } else { "" }))
            .collect::<Vec<_>>()
            .join("\n- ")
    )]
    ConflictingUrls {
        package_name: PackageName,
        urls: Vec<ParsedUrl>,
        env: ResolverEnvironment,
    },

    #[error(
        "Requirements contain conflicting indexes for package `{package_name}`{}:\n- {}",
        if env.marker_environment().is_some() {
            String::new()
        } else {
            format!(" in {env}")
        },
        indexes.iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n- ")
    )]
    ConflictingIndexesForEnvironment {
        package_name: PackageName,
        indexes: Vec<IndexUrl>,
        env: ResolverEnvironment,
    },

    #[error("Requirements contain conflicting indexes for package `{0}`: `{1}` vs. `{2}`")]
    ConflictingIndexes(PackageName, String, String),

    #[error(
        "Package `{name}` was included as a URL dependency. URL dependencies must be expressed as direct requirements or constraints. Consider adding `{requirement}` to your dependencies or constraints file.",
        name = name.cyan(),
        requirement = format!("{name} @ {url}").cyan(),
    )]
    DisallowedUrl { name: PackageName, url: String },

    #[error(transparent)]
    DistributionType(#[from] uv_distribution_types::Error),

    #[error("{0} `{1}`")]
    Dist(
        DistErrorKind,
        Box<RequestedDist>,
        DerivationChain,
        #[source] Arc<uv_distribution::Error>,
    ),

    #[error(transparent)]
    NoSolution(#[from] Box<NoSolutionError>),

    #[error("Attempted to construct an invalid version specifier")]
    InvalidVersion(#[from] uv_pep440::VersionSpecifierBuildError),

    #[error(
        "In `--require-hashes` mode, all requirements must be pinned upfront with `==`, but found: `{0}`"
    )]
    UnhashedPackage(PackageName),

    #[error("found conflicting distribution in resolution: {0}")]
    ConflictingDistribution(ConflictingDistributionError),

    #[error("Package `{0}` is unavailable")]
    PackageUnavailable(PackageName),

    #[error("Invalid extra value in conflict marker: {reason}: {raw_extra}")]
    InvalidExtraInConflictMarker {
        reason: String,
        raw_extra: ExtraName,
    },

    #[error("Invalid {kind} value in conflict marker: {name_error}")]
    InvalidValueInConflictMarker {
        kind: &'static str,
        #[source]
        name_error: InvalidNameError,
    },
    #[error(
        "The index returned metadata for the wrong package: expected {request} for {expected}, got {request} for {actual}"
    )]
    MismatchedPackageName {
        request: &'static str,
        expected: PackageName,
        actual: PackageName,
    },
}

impl uv_errors::Hint for ResolveError {
    fn hints(&self) -> uv_errors::Hints<'_> {
        match self {
            Self::NoSolution(no_solution) => uv_errors::Hint::hints(no_solution.as_ref()),
            Self::Client(error) => uv_errors::Hint::hints(error),
            Self::Distribution(error) => uv_errors::Hint::hints(error),
            Self::Dependencies(error, ..) => uv_errors::Hint::hints(error.as_ref()),
            _ => uv_errors::Hints::none(),
        }
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for ResolveError {
    /// Drop the value we want to send to not leak the private type we're sending.
    /// The tokio error only says "channel closed", so we don't lose information.
    fn from(_value: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Self::ChannelClosed
    }
}

pub type ErrorTree = DerivationTree<PubGrubPackage, Range<Version>, UnavailableReason>;
type ErrorExternal = External<PubGrubPackage, Range<Version>, UnavailableReason>;
type ErrorDerived = Derived<PubGrubPackage, Range<Version>, UnavailableReason>;
type ErrorTerms = Map<PubGrubPackage, Term<Range<Version>>>;

/// Visit each distinct package in a derivation tree without recursive calls.
///
/// The iteration order is unspecified, matching the set semantics of
/// [`DerivationTree::packages`].
pub(crate) fn derivation_tree_packages(
    derivation_tree: &ErrorTree,
) -> impl Iterator<Item = &PubGrubPackage> {
    let mut packages = FxHashSet::default();
    let mut trees = vec![derivation_tree];

    while let Some(tree) = trees.pop() {
        match tree {
            DerivationTree::External(external) => match external {
                External::FromDependencyOf(package, _, dependency, _) => {
                    packages.insert(package);
                    packages.insert(dependency);
                }
                External::NoVersions(package, _)
                | External::NotRoot(package, _)
                | External::Custom(package, _, _) => {
                    packages.insert(package);
                }
            },
            DerivationTree::Derived(derived) => {
                packages.extend(derived.terms.keys());
                trees.push(&derived.cause1);
                trees.push(&derived.cause2);
            }
        }
    }

    packages.into_iter()
}

/// Drop an exclusively owned derivation tree without recursing through its children.
///
/// Shared [`Arc`] children are left for their remaining owners; once the last owner is processed,
/// [`Arc::try_unwrap`] exposes the child for iterative destruction.
pub(crate) fn drop_derivation_tree(derivation_tree: ErrorTree) {
    let mut trees = vec![derivation_tree];

    while let Some(tree) = trees.pop() {
        if let DerivationTree::Derived(derived) = tree {
            if let Ok(cause1) = Arc::try_unwrap(derived.cause1) {
                trees.push(cause1);
            }
            if let Ok(cause2) = Arc::try_unwrap(derived.cause2) {
                trees.push(cause2);
            }
        }
    }
}

/// Own a derivation tree whose destruction must not recurse through the process stack.
///
/// The `Option` allows [`Drop`] to take ownership of the tree and applies the same iterative
/// teardown during normal returns and unwinding.
#[derive(Clone)]
struct StackSafeErrorTree(Option<ErrorTree>);

impl StackSafeErrorTree {
    fn new(derivation_tree: ErrorTree) -> Self {
        Self(Some(derivation_tree))
    }

    fn into_inner(mut self) -> ErrorTree {
        self.0.take().expect("derivation tree is only taken once")
    }
}

impl Deref for StackSafeErrorTree {
    type Target = ErrorTree;

    fn deref(&self) -> &Self::Target {
        self.0
            .as_ref()
            .expect("derivation tree is only taken during drop")
    }
}

impl Drop for StackSafeErrorTree {
    fn drop(&mut self) {
        if let Some(derivation_tree) = self.0.take() {
            drop_derivation_tree(derivation_tree);
        }
    }
}

impl Debug for StackSafeErrorTree {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        debug_derivation_tree(self, f)
    }
}

/// Preserve PubGrub's [`Debug`] representation without recursive formatting.
fn debug_derivation_tree(
    derivation_tree: &ErrorTree,
    formatter: &mut Formatter<'_>,
) -> std::fmt::Result {
    enum Frame<'a> {
        Tree(&'a ErrorTree),
        Text(&'static str),
    }

    let mut frames = vec![Frame::Tree(derivation_tree)];

    while let Some(frame) = frames.pop() {
        match frame {
            Frame::Tree(DerivationTree::External(external)) => {
                write!(formatter, "External({external:?})")?;
            }
            Frame::Tree(DerivationTree::Derived(derived)) => {
                write!(
                    formatter,
                    "Derived(Derived {{ terms: {:?}, shared_id: {:?}, cause1: ",
                    derived.terms, derived.shared_id
                )?;
                frames.push(Frame::Text(" })"));
                frames.push(Frame::Tree(&derived.cause2));
                frames.push(Frame::Text(", cause2: "));
                frames.push(Frame::Tree(&derived.cause1));
            }
            Frame::Text(text) => formatter.write_str(text)?,
        }
    }

    Ok(())
}

struct DerivedMetadata {
    terms: ErrorTerms,
    shared_id: Option<usize>,
}

enum TreeTask {
    Visit(StackSafeErrorTree),
    Rebuild(DerivedMetadata),
}

fn schedule_derived(tasks: &mut Vec<TreeTask>, derived: ErrorDerived) {
    let Derived {
        terms,
        shared_id,
        cause1,
        cause2,
    } = derived;
    tasks.push(TreeTask::Rebuild(DerivedMetadata { terms, shared_id }));
    tasks.push(TreeTask::Visit(StackSafeErrorTree::new(
        Arc::unwrap_or_clone(cause2),
    )));
    tasks.push(TreeTask::Visit(StackSafeErrorTree::new(
        Arc::unwrap_or_clone(cause1),
    )));
}

fn transform_derivation_tree(
    derivation_tree: ErrorTree,
    mut transform_external: impl FnMut(ErrorExternal) -> Option<ErrorTree>,
    mut transform_derived: impl FnMut(
        DerivedMetadata,
        Option<ErrorTree>,
        Option<ErrorTree>,
    ) -> Option<ErrorTree>,
) -> Option<ErrorTree> {
    let mut tasks = vec![TreeTask::Visit(StackSafeErrorTree::new(derivation_tree))];
    let mut results: Vec<Option<StackSafeErrorTree>> = Vec::new();

    while let Some(task) = tasks.pop() {
        match task {
            TreeTask::Visit(tree) => match tree.into_inner() {
                DerivationTree::External(external) => {
                    results.push(transform_external(external).map(StackSafeErrorTree::new));
                }
                DerivationTree::Derived(derived) => schedule_derived(&mut tasks, derived),
            },
            TreeTask::Rebuild(metadata) => {
                let cause2 = results
                    .pop()
                    .expect("every derived tree has a second transformed cause")
                    .map(StackSafeErrorTree::into_inner);
                let cause1 = results
                    .pop()
                    .expect("every derived tree has a first transformed cause")
                    .map(StackSafeErrorTree::into_inner);
                results
                    .push(transform_derived(metadata, cause1, cause2).map(StackSafeErrorTree::new));
            }
        }
    }

    results
        .pop()
        .expect("the root derivation tree produces one transformed result")
        .map(StackSafeErrorTree::into_inner)
}

fn map_derivation_tree(
    derivation_tree: ErrorTree,
    mut transform_external: impl FnMut(ErrorExternal) -> ErrorTree,
    mut transform_derived: impl FnMut(DerivedMetadata, ErrorTree, ErrorTree) -> ErrorTree,
) -> ErrorTree {
    transform_derivation_tree(
        derivation_tree,
        |external| Some(transform_external(external)),
        |metadata, cause1, cause2| {
            Some(transform_derived(
                metadata,
                cause1.expect("map transformations retain the first cause"),
                cause2.expect("map transformations retain the second cause"),
            ))
        },
    )
    .expect("map transformations retain the root")
}

fn derived_tree(metadata: DerivedMetadata, cause1: ErrorTree, cause2: ErrorTree) -> ErrorTree {
    DerivationTree::Derived(Derived {
        terms: metadata.terms,
        shared_id: metadata.shared_id,
        cause1: Arc::new(cause1),
        cause2: Arc::new(cause2),
    })
}

/// Narrow a version set onto a non-empty list of known versions.
fn narrow_to_known(set: &Range<Version>, versions: &[Version]) -> Range<Version> {
    if versions.iter().all(|version| set.contains(version)) {
        Range::full()
    } else {
        set.narrow_versions(versions)
    }
}

/// A wrapper around [`pubgrub::error::NoSolutionError`] that displays a resolution failure report.
pub struct NoSolutionError {
    error: StackSafeErrorTree,
    index: InMemoryIndex,
    /// The versions that were available for each package after `exclude-newer` filtering.
    ///
    /// For versions available before filtering, see [`NoSolutionError::available_versions`].
    included_versions: FxHashMap<PackageName, BTreeSet<Version>>,
    /// The versions available for each package.
    ///
    /// These version sets are not filtered by `exclude-newer`. See
    /// [`NoSolutionError::included_versions`] instead if filtered versions are needed.
    ///
    /// These versions are filtered by [`EnvVars::UV_TEST_AVAILABLE_VERSION_CUTOFF`] for
    /// deterministic output in tests.
    available_versions: FxHashMap<PackageName, BTreeSet<Version>>,
    available_indexes: FxHashMap<PackageName, BTreeSet<IndexUrl>>,
    selector: CandidateSelector,
    python_requirement: PythonRequirement,
    index_locations: IndexLocations,
    index_capabilities: IndexCapabilities,
    unavailable_packages: FxHashMap<PackageName, UnavailablePackage>,
    incomplete_packages: FxHashMap<PackageName, BTreeMap<Version, MetadataUnavailable>>,
    fork_urls: ForkUrls,
    fork_indexes: ForkIndexes,
    env: ResolverEnvironment,
    current_environment: MarkerEnvironment,
    tags: Option<Tags>,
    workspace_members: BTreeSet<PackageName>,
    options: Options,
    /// Cached report and hints, computed once on first access.
    cached: OnceLock<(String, IndexSet<PubGrubHint>)>,
}

impl NoSolutionError {
    /// Create a new [`NoSolutionError`] from a [`pubgrub::NoSolutionError`].
    pub(crate) fn new(
        error: pubgrub::NoSolutionError<UvDependencyProvider>,
        index: InMemoryIndex,
        included_versions: FxHashMap<PackageName, BTreeSet<Version>>,
        available_versions: FxHashMap<PackageName, BTreeSet<Version>>,
        available_indexes: FxHashMap<PackageName, BTreeSet<IndexUrl>>,
        selector: CandidateSelector,
        python_requirement: PythonRequirement,
        index_locations: IndexLocations,
        index_capabilities: IndexCapabilities,
        unavailable_packages: FxHashMap<PackageName, UnavailablePackage>,
        incomplete_packages: FxHashMap<PackageName, BTreeMap<Version, MetadataUnavailable>>,
        fork_urls: ForkUrls,
        fork_indexes: ForkIndexes,
        env: ResolverEnvironment,
        current_environment: MarkerEnvironment,
        tags: Option<Tags>,
        workspace_members: BTreeSet<PackageName>,
        options: Options,
    ) -> Self {
        Self {
            error: StackSafeErrorTree::new(error),
            index,
            included_versions,
            available_versions,
            available_indexes,
            selector,
            python_requirement,
            index_locations,
            index_capabilities,
            unavailable_packages,
            incomplete_packages,
            fork_urls,
            fork_indexes,
            env,
            current_environment,
            tags,
            workspace_members,
            options,
            cached: OnceLock::new(),
        }
    }

    /// Get the cached report and hints, computing them on first access.
    fn cached(&self) -> &(String, IndexSet<PubGrubHint>) {
        self.cached.get_or_init(|| self.compute_report_and_hints())
    }

    /// Given a [`DerivationTree`], collapse any [`External::FromDependencyOf`] incompatibilities
    /// wrap an [`PubGrubPackageInner::Extra`] package.
    pub(crate) fn collapse_proxies(derivation_tree: ErrorTree) -> ErrorTree {
        fn is_proxy(tree: &ErrorTree) -> bool {
            matches!(
                tree,
                DerivationTree::External(External::FromDependencyOf(package, ..))
                    if package.is_proxy()
            )
        }

        transform_derivation_tree(
            derivation_tree,
            |external| Some(DerivationTree::External(external)),
            |metadata, cause1, cause2| match (cause1, cause2) {
                (Some(cause1), Some(cause2)) if is_proxy(&cause1) && is_proxy(&cause2) => None,
                (Some(cause), other) if is_proxy(&cause) => other,
                (other, Some(cause)) if is_proxy(&cause) => other,
                (Some(cause1), Some(cause2)) => Some(derived_tree(metadata, cause1, cause2)),
                (Some(cause), None) | (None, Some(cause)) => Some(cause),
                (None, None) => None,
            },
        )
        .expect("derivation tree should contain at least one external term")
    }

    /// Simplifies the version ranges on any incompatibilities to remove the `[max]` sentinel.
    ///
    /// The `[max]` sentinel is used to represent the maximum local version of a package, to
    /// implement PEP 440 semantics for local version equality. For example, `1.0.0+foo` needs to
    /// satisfy `==1.0.0`.
    pub(crate) fn collapse_local_version_segments(derivation_tree: ErrorTree) -> ErrorTree {
        transform_derivation_tree(
            derivation_tree,
            |external| match external {
                external @ External::NotRoot(_, _) => Some(DerivationTree::External(external)),
                External::NoVersions(package, versions) => {
                    if versions.is_local_version_complement() {
                        return None;
                    }

                    let versions = versions.without_local_version_sentinels();
                    Some(DerivationTree::External(External::NoVersions(
                        package, versions,
                    )))
                }
                External::FromDependencyOf(package1, versions1, package2, versions2) => {
                    let versions1 = versions1.without_local_version_sentinels();
                    let versions2 = versions2.without_local_version_sentinels();
                    Some(DerivationTree::External(External::FromDependencyOf(
                        package1, versions1, package2, versions2,
                    )))
                }
                External::Custom(package, versions, reason) => {
                    let versions = versions.without_local_version_sentinels();
                    Some(DerivationTree::External(External::Custom(
                        package, versions, reason,
                    )))
                }
            },
            |mut metadata, cause1, cause2| {
                metadata.terms = metadata
                    .terms
                    .into_iter()
                    .map(|(package, term)| {
                        let term = match term {
                            Term::Positive(versions) => {
                                Term::Positive(versions.without_local_version_sentinels())
                            }
                            Term::Negative(versions) => {
                                Term::Negative(versions.without_local_version_sentinels())
                            }
                        };
                        (package, term)
                    })
                    .collect();

                match (cause1, cause2) {
                    (Some(cause1), Some(cause2)) => Some(derived_tree(metadata, cause1, cause2)),
                    (Some(cause), None) | (None, Some(cause)) => Some(cause),
                    (None, None) => None,
                }
            },
        )
        .expect("derivation tree should contain at least one term")
    }

    /// Shrinks widened version sets in the derivation tree back onto the known versions.
    ///
    /// The resolver widens version sets to the largest interval containing the same known
    /// versions ([`Ranges::widen_versions`]), both on the depending side of a dependency
    /// incompatibility and for a version that cannot be used, keeping version sets small during
    /// resolution. The widened bounds are misleading in error messages, e.g., `a>1.5.2,<2.0.0`
    /// when `a 1.5.3` is the only version in that interval. Shrink the depending side, the
    /// unavailable set, and positive terms back: a set containing all known versions of a package
    /// becomes the full range ("all versions of a"); otherwise, bounded ends are narrowed to
    /// inclusive bounds on the known versions they contain while unbounded ends are preserved,
    /// except on an unavailable set, which never claims a version outside the listing. Where a
    /// package has one version, a set that rules it out is reported as that version rather than as
    /// all of them. Dependency requests (the depended-on side and negative terms) are shown as
    /// requested.
    pub(crate) fn narrow_widened_sets(
        derivation_tree: ErrorTree,
        known_versions: &FxHashMap<PackageName, Arc<[Version]>>,
    ) -> ErrorTree {
        // The known versions of a package, with the lowest and the highest. An empty list carries
        // no information, so it is treated as absent.
        let listed = |package: &PubGrubPackage| -> Option<(&[Version], &Version, &Version)> {
            let versions = package
                .name_no_root()
                .and_then(|name| known_versions.get(name))?;
            Some((versions, versions.first()?, versions.last()?))
        };

        let narrow = |package: &PubGrubPackage, set: Range<Version>| -> Range<Version> {
            let Some((versions, _, _)) = listed(package) else {
                return set;
            };
            narrow_to_known(&set, versions)
        };

        // A single version's unavailability says nothing about versions outside the listing, so
        // an unbounded end of its widened set must not be reported as a claim about them.
        let narrow_unavailable =
            |package: &PubGrubPackage, set: Range<Version>| -> Range<Version> {
                let Some((versions, lowest, highest)) = listed(package) else {
                    return set;
                };
                // A rejection of the one version a package has reports that version.
                if let [version] = versions
                    && set.contains(version)
                {
                    return Range::singleton(version.clone());
                }
                let narrowed = narrow_to_known(&set, versions);
                // A claim about every known version is reported as such, not as its bounds.
                if narrowed == Range::full() {
                    return narrowed;
                }
                let envelope = Range::from_range_bounds(lowest.clone()..=highest.clone());
                let clamped = narrowed.intersection(&envelope);
                // A rejected version outside the listing has no envelope to report it in.
                if clamped == Range::empty() {
                    narrowed
                } else {
                    clamped
                }
            };

        // A conclusion about a package with one version reports that version where its causes do,
        // so that the step does not reach past them.
        let narrow_conclusion = |package: &PubGrubPackage,
                                 set: Range<Version>,
                                 cause1: &ErrorTree,
                                 cause2: &ErrorTree|
         -> Range<Version> {
            let Some(([version], _, _)) = listed(package) else {
                return narrow(package, set);
            };
            let single = Range::singleton(version.clone());
            if set.contains(version)
                && reported_versions(package, cause1).union(&reported_versions(package, cause2))
                    == single
            {
                return single;
            }
            narrow(package, set)
        };

        map_derivation_tree(
            derivation_tree,
            |external| match external {
                External::FromDependencyOf(package1, versions1, package2, versions2) => {
                    // A rejected version is recorded as a dependency on Python, so the widened
                    // side is an exclusion and stops at the listing like any other.
                    let versions1 = if matches!(&*package2, PubGrubPackageInner::Python(_)) {
                        narrow_unavailable(&package1, versions1)
                    } else {
                        narrow(&package1, versions1)
                    };
                    DerivationTree::External(External::FromDependencyOf(
                        package1, versions1, package2, versions2,
                    ))
                }
                External::Custom(package, versions, reason) => {
                    let versions = match &reason {
                        UnavailableReason::Version(_) => narrow_unavailable(&package, versions),
                        UnavailableReason::Package(_) => narrow(&package, versions),
                    };
                    DerivationTree::External(External::Custom(package, versions, reason))
                }
                external => DerivationTree::External(external),
            },
            |mut metadata, cause1, cause2| {
                metadata.terms = metadata
                    .terms
                    .into_iter()
                    .map(|(package, term)| {
                        let term = match term {
                            Term::Positive(versions) => Term::Positive(narrow_conclusion(
                                &package, versions, &cause1, &cause2,
                            )),
                            term @ Term::Negative(_) => term,
                        };
                        (package, term)
                    })
                    .collect();

                derived_tree(metadata, cause1, cause2)
            },
        )
    }

    /// Given a [`DerivationTree`], identify the largest required Python version that is missing.
    pub fn find_requires_python(&self) -> LowerBound {
        let mut minimum = LowerBound::default();
        let mut trees = vec![&*self.error];

        while let Some(derivation_tree) = trees.pop() {
            match derivation_tree {
                DerivationTree::Derived(derived) => {
                    trees.push(&derived.cause2);
                    trees.push(&derived.cause1);
                }
                DerivationTree::External(External::FromDependencyOf(.., package, version)) => {
                    if let PubGrubPackageInner::Python(_) = &**package {
                        if let Some((lower, ..)) = version.bounding_range() {
                            let lower = LowerBound::new(lower.cloned());
                            if lower > minimum {
                                minimum = lower;
                            }
                        }
                    }
                }
                DerivationTree::External(_) => {}
            }
        }

        minimum
    }

    /// Return the [`ResolverEnvironment`] that caused the failure.
    pub fn environment(&self) -> &ResolverEnvironment {
        &self.env
    }

    /// Get the packages that are involved in this error.
    pub fn packages(&self) -> impl Iterator<Item = &PackageName> {
        derivation_tree_packages(&self.error)
            .filter_map(|p| p.name())
            .unique()
    }

    /// Generate the report and hints for this resolution failure.
    ///
    /// Returns the formatted report string and structured [`PubGrubHint`] values.
    /// The result is cached so repeated calls (e.g., from both `Display` and
    /// explicit hint collection) don't recompute the derivation tree.
    /// Return the formatted report string.
    pub fn report(&self) -> &str {
        &self.cached().0
    }

    /// Return the computed PubGrub hints.
    fn pubgrub_hints(&self) -> &IndexSet<PubGrubHint> {
        &self.cached().1
    }

    /// Compute the reduced derivation tree, formatted report string, and hints.
    fn compute_report_and_hints(&self) -> (String, IndexSet<PubGrubHint>) {
        let formatter = PubGrubReportFormatter {
            included_versions: &self.included_versions,
            available_versions: &self.available_versions,
            python_requirement: &self.python_requirement,
            workspace_members: &self.workspace_members,
            tags: self.tags.as_ref(),
        };

        // Transform the error tree for reporting
        let mut tree = simplify_derivation_tree_markers(
            self.error.clone().into_inner(),
            &self.python_requirement,
        );
        let should_display_tree = std::env::var_os(EnvVars::UV_INTERNAL__SHOW_DERIVATION_TREE)
            .is_some()
            || tracing::enabled!(tracing::Level::TRACE);

        if should_display_tree {
            display_tree(&tree, "Resolver derivation tree before reduction");
        }

        tree = collapse_no_versions_of_workspace_members(tree, &self.workspace_members);

        if self.workspace_members.len() == 1 {
            let project = self.workspace_members.iter().next().unwrap();
            tree = drop_root_dependency_on_project(tree, project);
        }

        tree = collapse_unavailable_versions(tree);
        tree = collapse_redundant_depends_on_no_versions(tree);

        tree = simplify_derivation_tree_ranges(
            tree,
            &self.included_versions,
            &self.selector,
            &self.env,
        );

        // These need to be applied _after_ simplification of the ranges
        tree = restate_available_versions(tree, &self.included_versions);
        tree = collapse_redundant_no_versions(tree);

        loop {
            let (collapsed, changed) = collapse_redundant_no_versions_tree(tree);
            tree = collapsed;
            if !changed {
                break;
            }
        }

        if should_display_tree {
            display_tree(&tree, "Resolver derivation tree after reduction");
        }

        let report = report_derivation_tree(&tree, &formatter);

        let inherited_exclude_newer_ranges = FxHashMap::default();
        let mut hints = IndexSet::default();
        formatter.generate_hints(
            &tree,
            &self.index,
            &self.selector,
            &self.index_locations,
            &self.index_capabilities,
            &self.available_indexes,
            &self.unavailable_packages,
            &self.incomplete_packages,
            &self.fork_urls,
            &self.fork_indexes,
            &self.env,
            &self.current_environment,
            self.tags.as_ref(),
            &self.workspace_members,
            &self.options,
            &inherited_exclude_newer_ranges,
            &mut hints,
        );

        (report, hints)
    }
}

impl std::fmt::Debug for NoSolutionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Include every field except `index` (no Debug) and `cached` (derived).
        let Self {
            error,
            index: _,
            included_versions,
            available_versions,
            available_indexes,
            selector,
            python_requirement,
            index_locations,
            index_capabilities,
            unavailable_packages,
            incomplete_packages,
            fork_urls,
            fork_indexes,
            env,
            current_environment,
            tags,
            workspace_members,
            options,
            cached: _,
        } = self;
        f.debug_struct("NoSolutionError")
            .field("error", error)
            .field("included_versions", included_versions)
            .field("available_versions", available_versions)
            .field("available_indexes", available_indexes)
            .field("selector", selector)
            .field("python_requirement", python_requirement)
            .field("index_locations", index_locations)
            .field("index_capabilities", index_capabilities)
            .field("unavailable_packages", unavailable_packages)
            .field("incomplete_packages", incomplete_packages)
            .field("fork_urls", fork_urls)
            .field("fork_indexes", fork_indexes)
            .field("env", env)
            .field("current_environment", current_environment)
            .field("tags", tags)
            .field("workspace_members", workspace_members)
            .field("options", options)
            .finish()
    }
}

impl std::error::Error for NoSolutionError {}

impl uv_errors::Hint for NoSolutionError {
    fn hints(&self) -> uv_errors::Hints<'_> {
        self.pubgrub_hints()
            .iter()
            .map(ToString::to_string)
            .collect()
    }
}

impl uv_errors::Hint for Box<NoSolutionError> {
    fn hints(&self) -> uv_errors::Hints<'_> {
        self.as_ref().hints()
    }
}

impl std::fmt::Display for NoSolutionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Write only the derivation report. Hints are available separately
        // via `hints()` and rendered by the caller.
        write!(f, "{}", self.report())
    }
}

#[expect(clippy::print_stderr)]
fn display_tree(
    error: &DerivationTree<PubGrubPackage, Range<Version>, UnavailableReason>,
    name: &str,
) {
    let mut lines = Vec::new();
    display_tree_inner(error, &mut lines);
    lines.reverse();

    if std::env::var_os(EnvVars::UV_INTERNAL__SHOW_DERIVATION_TREE).is_some() {
        eprintln!("{name}\n{}", lines.join("\n"));
    } else {
        trace!("{name}\n{}", lines.join("\n"));
    }
}

fn display_tree_inner(
    error: &DerivationTree<PubGrubPackage, Range<Version>, UnavailableReason>,
    lines: &mut Vec<String>,
) {
    enum Frame<'a> {
        Tree(&'a ErrorTree, usize),
        Terms(&'a ErrorTerms, usize),
    }

    let mut frames = vec![Frame::Tree(error, 0)];
    while let Some(frame) = frames.pop() {
        match frame {
            Frame::Tree(DerivationTree::Derived(derived), depth) => {
                frames.push(Frame::Terms(&derived.terms, depth));
                frames.push(Frame::Tree(&derived.cause2, depth + 1));
                frames.push(Frame::Tree(&derived.cause1, depth + 1));
            }
            Frame::Tree(DerivationTree::External(external), depth) => {
                let prefix = "  ".repeat(depth);
                match external {
                    External::FromDependencyOf(
                        package,
                        version,
                        dependency,
                        dependency_version,
                    ) => {
                        lines.push(format!(
                            "{prefix}{package}{version} depends on {dependency}{dependency_version}"
                        ));
                    }
                    External::Custom(package, versions, reason) => match reason {
                        UnavailableReason::Package(_) => {
                            lines.push(format!("{prefix}{package} {reason}"));
                        }
                        UnavailableReason::Version(_) => {
                            lines.push(format!("{prefix}{package}{versions} {reason}"));
                        }
                    },
                    External::NoVersions(package, versions) => {
                        lines.push(format!("{prefix}no versions of {package}{versions}"));
                    }
                    External::NotRoot(package, versions) => {
                        lines.push(format!("{prefix}not root {package}{versions}"));
                    }
                }
            }
            Frame::Terms(terms, depth) => {
                let prefix = "  ".repeat(depth);
                for (package, term) in terms {
                    match term {
                        Term::Positive(versions) => {
                            lines.push(format!("{prefix}term {package}{versions}"));
                        }
                        Term::Negative(versions) => {
                            lines.push(format!("{prefix}term not {package}{versions}"));
                        }
                    }
                }
            }
        }
    }
}

/// Given a [`DerivationTree`], restate the availability of a package where a derivation reaches
/// past the versions its causes rule out.
///
/// A derivation rules out the union of what its causes rule out, but the reported version sets are
/// simplified onto the versions that exist, which drops the versions that do not. The derivation
/// then reaches past its causes over a range that holds nothing, and the report has to say so, as it
/// does for the ranges the resolver itself finds empty. The version sets are left as they are; only
/// the statement that carries them is added, and only when every version it adds is one that the
/// package does not have.
fn restate_available_versions(
    tree: ErrorTree,
    included_versions: &FxHashMap<PackageName, BTreeSet<Version>>,
) -> ErrorTree {
    map_derivation_tree(
        tree,
        DerivationTree::External,
        |metadata, cause1, cause2| {
            let Some((package, unlisted)) =
                unlisted_versions(&metadata.terms, &cause1, &cause2, included_versions)
            else {
                return derived_tree(metadata, cause1, cause2);
            };

            let statement = || {
                DerivationTree::External(External::NoVersions(package.clone(), unlisted.clone()))
            };
            let carry = |cause: ErrorTree| {
                let carried = ruled_out_versions(&package, &cause).union(&unlisted);
                DerivationTree::Derived(Derived {
                    terms: Map::from_iter([(package.clone(), Term::Positive(carried))]),
                    shared_id: None,
                    cause1: Arc::new(cause),
                    cause2: Arc::new(statement()),
                })
            };

            // The statement carries a step that rules the package out, so it goes on the cause that
            // does the ruling out. Where neither does, the step reaches past its causes on its own
            // and the statement carries the step itself.
            let first = ruled_out_versions(&package, &cause1);
            let second = ruled_out_versions(&package, &cause2);
            if first.is_empty() && second.is_empty() {
                let concluded = metadata
                    .terms
                    .get(&package)
                    .cloned()
                    .unwrap_or_else(|| Term::Positive(unlisted.clone()));
                let mut inner = metadata;
                let shared_id = inner.shared_id.take();
                inner.terms = Map::from_iter([(
                    package.clone(),
                    Term::Positive(
                        reported_versions(&package, &cause1)
                            .union(&reported_versions(&package, &cause2)),
                    ),
                )]);
                return DerivationTree::Derived(Derived {
                    terms: Map::from_iter([(package.clone(), concluded)]),
                    shared_id,
                    cause1: Arc::new(derived_tree(inner, cause1, cause2)),
                    cause2: Arc::new(statement()),
                });
            }
            if starts_lower(&first, &second) {
                derived_tree(metadata, carry(cause1), cause2)
            } else {
                derived_tree(metadata, cause1, carry(cause2))
            }
        },
    )
}

/// The versions a derivation rules out without either cause mentioning them, when they are versions
/// the package does not have.
///
/// A derivation reaches past its causes in two places: the conclusion it draws about a package, and
/// the requirement it resolves for a package it drops from its terms.
fn unlisted_versions(
    terms: &ErrorTerms,
    cause1: &ErrorTree,
    cause2: &ErrorTree,
    included_versions: &FxHashMap<PackageName, BTreeSet<Version>>,
) -> Option<(PubGrubPackage, Range<Version>)> {
    // What the causes establish about the package carries its conclusion, but only what they rule
    // out can carry a requirement: a dependency of the package is not a statement that it cannot be
    // used.
    let concluded = concluded_package(terms).map(|(package, concluded)| {
        let reported =
            reported_versions(package, cause1).union(&reported_versions(package, cause2));
        (package, concluded.clone(), reported)
    });
    let resolved = cause_packages(cause1)
        .into_iter()
        .chain(cause_packages(cause2))
        .filter(|package| !terms.contains_key(*package))
        .map(|package| {
            let required =
                required_versions(package, cause1).union(&required_versions(package, cause2));
            let ruled_out =
                ruled_out_versions(package, cause1).union(&ruled_out_versions(package, cause2));
            (package, required, ruled_out)
        });

    for (package, covered, reported) in concluded.into_iter().chain(resolved) {
        // A requirement that no cause rules out any of is unavailable outright rather than across a
        // range, which [`collapse_redundant_depends_on_no_versions`] reports without the listing.
        if reported.is_empty() {
            continue;
        }
        let unlisted = covered.intersection(&reported.complement());
        if unlisted.is_empty() {
            continue;
        }
        // Only a version the package does not have can go unmentioned; anything else is a
        // derivation its causes really do not support.
        let Some(versions) = package
            .name_no_root()
            .and_then(|name| included_versions.get(name))
        else {
            continue;
        };
        if versions.iter().any(|version| unlisted.contains(version)) {
            continue;
        }
        return Some((package.clone(), unlisted));
    }
    None
}

/// Whether the first version set starts below the second, treating an empty set as starting above
/// every other.
fn starts_lower(first: &Range<Version>, second: &Range<Version>) -> bool {
    let lowest = |versions: &Range<Version>| {
        versions.bounding_range().map(|(lower, _)| match lower {
            Bound::Unbounded => None,
            Bound::Included(version) | Bound::Excluded(version) => Some(version.clone()),
        })
    };
    match (lowest(first), lowest(second)) {
        // An unbounded start sorts below every version.
        (Some(first), Some(second)) => first <= second,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// The package a derivation concludes about, when it concludes about one alone.
fn concluded_package(terms: &ErrorTerms) -> Option<(&PubGrubPackage, &Range<Version>)> {
    let mut terms = terms.iter();
    let (package, Term::Positive(versions)) = terms.next()? else {
        return None;
    };
    terms.next().is_none().then_some((package, versions))
}

/// The packages a cause states something about.
fn cause_packages(cause: &ErrorTree) -> Vec<&PubGrubPackage> {
    match cause {
        DerivationTree::Derived(derived) => derived.terms.keys().collect(),
        DerivationTree::External(
            External::Custom(package, ..)
            | External::NoVersions(package, ..)
            | External::NotRoot(package, ..),
        ) => vec![package],
        DerivationTree::External(External::FromDependencyOf(package, _, dependency, _)) => {
            vec![package, dependency]
        }
    }
}

/// The versions of `package` that a cause requires to remain usable.
fn required_versions(package: &PubGrubPackage, cause: &ErrorTree) -> Range<Version> {
    match cause {
        DerivationTree::Derived(derived) => match derived.terms.get(package) {
            Some(Term::Negative(versions)) => versions.clone(),
            _ => Range::empty(),
        },
        DerivationTree::External(External::FromDependencyOf(_, _, required, versions))
            if required == package =>
        {
            versions.clone()
        }
        DerivationTree::External(_) => Range::empty(),
    }
}

/// The versions of `package` that a cause states cannot be used.
fn ruled_out_versions(package: &PubGrubPackage, cause: &ErrorTree) -> Range<Version> {
    match cause {
        DerivationTree::Derived(derived) => match concluded_package(&derived.terms) {
            Some((concluded, versions)) if concluded == package => versions.clone(),
            _ => Range::empty(),
        },
        DerivationTree::External(External::Custom(ruled_out, versions, _))
            if ruled_out == package =>
        {
            versions.clone()
        }
        DerivationTree::External(_) => Range::empty(),
    }
}

/// The versions of `package` that a cause is reported as establishing something about.
fn reported_versions(package: &PubGrubPackage, cause: &ErrorTree) -> Range<Version> {
    match cause {
        DerivationTree::Derived(derived) => match derived.terms.get(package) {
            Some(Term::Positive(versions)) => versions.clone(),
            _ => Range::empty(),
        },
        DerivationTree::External(
            External::Custom(reported, versions, _)
            | External::NoVersions(reported, versions)
            | External::FromDependencyOf(reported, versions, ..),
        ) if reported == package => versions.clone(),
        DerivationTree::External(_) => Range::empty(),
    }
}

fn can_drop_no_versions(
    package: &PubGrubPackage,
    versions: &Range<Version>,
    other: &ErrorTree,
    parent_terms: &ErrorTerms,
) -> bool {
    let package_terms = if let DerivationTree::Derived(derived) = other {
        derived.terms.get(package)
    } else {
        parent_terms.get(package)
    };
    let Some(Term::Positive(term)) = package_terms else {
        return false;
    };
    let versions = versions.complement();

    // Retain exclusions of a single version because they produce useful messages like
    // "only foo==1.0.0 is available". Otherwise, the clause is redundant when the conclusion
    // covers either all versions or exactly the remaining range.
    versions.as_singleton().is_none() && (*term == Range::full() || *term == versions)
}

fn collapse_redundant_no_versions(tree: ErrorTree) -> ErrorTree {
    map_derivation_tree(
        tree,
        DerivationTree::External,
        |metadata, cause1, cause2| {
            if let DerivationTree::External(External::NoVersions(package, versions)) = &cause1
                && can_drop_no_versions(package, versions, &cause2, &metadata.terms)
            {
                return cause2;
            }

            if let DerivationTree::External(External::NoVersions(package, versions)) = &cause2
                && can_drop_no_versions(package, versions, &cause1, &metadata.terms)
            {
                return cause1;
            }

            derived_tree(metadata, cause1, cause2)
        },
    )
}

/// Given a [`DerivationTree`], collapse any derived trees with two `NoVersions` nodes for the same
/// package. For example, if we have a tree like:
///
/// ```text
/// term Python>=3.7.9
///   no versions of Python>=3.7.9, <3.8
///   no versions of Python>=3.8
/// ```
///
/// We can simplify this to:
///
/// ```text
/// no versions of Python>=3.7.9
/// ```
///
/// This function returns a `bool` indicating if a change was made. This allows for repeated calls,
/// e.g., the following tree contains nested redundant trees:
///
/// ```text
/// term Python>=3.10
///   no versions of Python>=3.11, <3.12
///   term Python>=3.10, <3.11 | >=3.12
///     no versions of Python>=3.12
///     no versions of Python>=3.10, <3.11
/// ```
///
/// We can simplify this to:
///
/// ```text
/// no versions of Python>=3.10
/// ```
///
/// This appears to be common with the way the resolver currently models Python version
/// incompatibilities.
fn collapse_redundant_no_versions_tree(tree: ErrorTree) -> (ErrorTree, bool) {
    let mut changed = false;
    let tree = map_derivation_tree(
        tree,
        DerivationTree::External,
        |metadata, cause1, cause2| {
            if let (
                DerivationTree::External(External::NoVersions(package, versions)),
                DerivationTree::External(External::NoVersions(other_package, other_versions)),
            ) = (&cause1, &cause2)
                && package == other_package
                && let Some(Term::Positive(term)) = metadata.terms.get(package)
                && versions.subset_of(term)
                && other_versions.subset_of(term)
            {
                changed = true;
                DerivationTree::External(External::NoVersions(package.clone(), term.clone()))
            } else {
                derived_tree(metadata, cause1, cause2)
            }
        },
    );
    (tree, changed)
}

fn is_workspace_member(
    package: &PubGrubPackage,
    workspace_members: &BTreeSet<PackageName>,
) -> bool {
    let (PubGrubPackageInner::Package { name, .. }
    | PubGrubPackageInner::Extra { name, .. }
    | PubGrubPackageInner::Group { name, .. }) = &**package
    else {
        return false;
    };
    workspace_members.contains(name)
}

/// Given a [`DerivationTree`], collapse any `NoVersion` incompatibilities for workspace members
/// to avoid saying things like "only <workspace-member>==0.1.0 is available".
fn collapse_no_versions_of_workspace_members(
    tree: ErrorTree,
    workspace_members: &BTreeSet<PackageName>,
) -> ErrorTree {
    map_derivation_tree(
        tree,
        DerivationTree::External,
        |metadata, cause1, cause2| {
            if let DerivationTree::External(External::NoVersions(package, _)) = &cause1
                && is_workspace_member(package, workspace_members)
            {
                return cause2;
            }
            if let DerivationTree::External(External::NoVersions(package, _)) = &cause2
                && is_workspace_member(package, workspace_members)
            {
                return cause1;
            }
            derived_tree(metadata, cause1, cause2)
        },
    )
}

fn collapse_redundant_dependency_child(
    tree: ErrorTree,
    package: &PubGrubPackage,
    versions: &Range<Version>,
) -> ErrorTree {
    let DerivationTree::Derived(derived) = &tree else {
        return tree;
    };
    let dependency_clause = match (&*derived.cause1, &*derived.cause2) {
        (
            DerivationTree::External(External::NoVersions(no_versions_package, _)),
            dependency_clause @ DerivationTree::External(External::FromDependencyOf(
                _,
                _,
                dependency_package,
                dependency_versions,
            )),
        )
        | (
            dependency_clause @ DerivationTree::External(External::FromDependencyOf(
                _,
                _,
                dependency_package,
                dependency_versions,
            )),
            DerivationTree::External(External::NoVersions(no_versions_package, _)),
        ) if no_versions_package == dependency_package
            && package == no_versions_package
            && versions.subset_of(dependency_versions) =>
        {
            Some(dependency_clause.clone())
        }
        _ => None,
    };

    dependency_clause.unwrap_or(tree)
}

/// Given a [`DerivationTree`], collapse `NoVersions` incompatibilities that are redundant children
/// of a dependency. For example, if we have a tree like:
///
/// ```text
/// A>=1,<2 depends on B
///     A has no versions >1,<2
///     C depends on A>=1,<2
/// ```
///
/// We can simplify this to `C depends on A>=1 and A>=1 depends on B so C depends on B` without
/// explaining that there are no other versions of A. This requires the range of A in "A depends
/// on B" to be a subset of the range in "C depends on A". For example, in a tree like:
///
/// ```text
/// A>=1,<3 depends on B
///     A has no versions >2,<3
///     C depends on A>=2,<3
/// ```
///
/// We cannot apply the same simplification because `A>=1,<3` is not a subset of `A>=2,<3`.
fn collapse_redundant_depends_on_no_versions(tree: ErrorTree) -> ErrorTree {
    map_derivation_tree(
        tree,
        DerivationTree::External,
        |metadata, cause1, cause2| {
            if let DerivationTree::External(External::FromDependencyOf(package, versions, _, _)) =
                &cause1
            {
                let cause2 = collapse_redundant_dependency_child(cause2, package, versions);
                return derived_tree(metadata, cause1, cause2);
            }
            if let DerivationTree::External(External::FromDependencyOf(package, versions, _, _)) =
                &cause2
            {
                let cause1 = collapse_redundant_dependency_child(cause1, package, versions);
                return derived_tree(metadata, cause1, cause2);
            }
            derived_tree(metadata, cause1, cause2)
        },
    )
}

/// Simplifies the markers on pubgrub packages in the given derivation tree
/// according to the given Python requirement.
///
/// For example, when there's a dependency like `foo ; python_version >= '3.11'` and
/// `requires-python = '>=3.11'`, this removes the redundant `python_version >= '3.11'` marker from
/// the error message.
fn simplify_derivation_tree_markers(
    tree: ErrorTree,
    python_requirement: &PythonRequirement,
) -> ErrorTree {
    map_derivation_tree(
        tree,
        |mut external| {
            match &mut external {
                External::NotRoot(package, _) | External::NoVersions(package, _) => {
                    package.simplify_markers(python_requirement);
                }
                External::FromDependencyOf(package1, _, package2, _) => {
                    package1.simplify_markers(python_requirement);
                    package2.simplify_markers(python_requirement);
                }
                External::Custom(package, _, _) => package.simplify_markers(python_requirement),
            }
            DerivationTree::External(external)
        },
        |mut metadata, cause1, cause2| {
            metadata.terms = metadata
                .terms
                .into_iter()
                .map(|(mut package, term)| {
                    package.simplify_markers(python_requirement);
                    (package, term)
                })
                .collect();
            derived_tree(metadata, cause1, cause2)
        },
    )
}

fn merge_unavailable_versions(
    package: &PubGrubPackage,
    versions: &Range<Version>,
    reason: &UnavailableReason,
    other: &ErrorTree,
) -> Option<ErrorTree> {
    let DerivationTree::Derived(derived) = other else {
        return None;
    };
    let merge = |cause: &ErrorTree| {
        let DerivationTree::External(External::Custom(other_package, other_versions, other_reason)) =
            cause
        else {
            return None;
        };
        (package == other_package && reason == other_reason).then(|| other_versions.union(versions))
    };

    // Keep the two cases separate to preserve the ordering of the causes.
    let (unchanged_cause, merged_versions, merged_is_cause2) =
        if let Some(merged_versions) = merge(&derived.cause2) {
            (derived.cause1.clone(), merged_versions, true)
        } else {
            let merged_versions = merge(&derived.cause1)?;
            (derived.cause2.clone(), merged_versions, false)
        };

    let merged_cause = Arc::new(DerivationTree::External(External::Custom(
        package.clone(),
        merged_versions.clone(),
        reason.clone(),
    )));
    let (cause1, cause2) = if merged_is_cause2 {
        (unchanged_cause, merged_cause)
    } else {
        (merged_cause, unchanged_cause)
    };

    let mut terms = derived.terms.clone();
    if let Some(Term::Positive(range)) = terms.get_mut(package) {
        *range = merged_versions;
    }
    Some(DerivationTree::Derived(Derived {
        terms,
        shared_id: derived.shared_id,
        cause1,
        cause2,
    }))
}

/// Combine two unavailabilities of one package that are the causes of the same derivation.
///
/// A derivation whose two causes both say that a package cannot be used concludes their union. If
/// they say it for the same reason, they are one statement about a larger set and the derivation
/// collapses into it, as [`merge_unavailable_versions`] does for a cause nested one level deeper.
/// Otherwise the two statements stay separate and only the conclusion is widened to cover them
/// both.
///
/// Two unavailabilities are siblings rather than nested when their version sets are contiguous.
fn merge_unavailable_siblings(derived: &ErrorDerived) -> Option<ErrorTree> {
    let DerivationTree::External(External::Custom(package, versions, reason)) = &*derived.cause1
    else {
        return None;
    };
    let DerivationTree::External(External::Custom(other_package, other_versions, other_reason)) =
        &*derived.cause2
    else {
        return None;
    };
    if package != other_package {
        return None;
    }

    // Only rewrite a derivation that concludes about this package alone.
    let mut terms = derived.terms.iter();
    let Some((term_package, Term::Positive(term_versions))) = terms.next() else {
        return None;
    };
    if terms.next().is_some() || term_package != package {
        return None;
    }

    let versions = versions.union(other_versions);
    if reason == other_reason {
        return Some(DerivationTree::External(External::Custom(
            package.clone(),
            versions,
            reason.clone(),
        )));
    }
    if versions.subset_of(term_versions) {
        return None;
    }
    Some(DerivationTree::Derived(Derived {
        terms: Map::from_iter([(
            package.clone(),
            Term::Positive(term_versions.union(&versions)),
        )]),
        shared_id: derived.shared_id,
        cause1: derived.cause1.clone(),
        cause2: derived.cause2.clone(),
    }))
}

/// Given a [`DerivationTree`], collapse incompatibilities for versions of a package that are
/// unavailable for the same reason to avoid repeating the same message for every unavailable
/// version.
fn collapse_unavailable_versions(tree: ErrorTree) -> ErrorTree {
    map_derivation_tree(
        tree,
        DerivationTree::External,
        |metadata, cause1, cause2| {
            let tree = if let DerivationTree::External(External::Custom(package, versions, reason)) =
                &cause1
                && let Some(tree) = merge_unavailable_versions(package, versions, reason, &cause2)
            {
                tree
            } else if let DerivationTree::External(External::Custom(package, versions, reason)) =
                &cause2
                && let Some(tree) = merge_unavailable_versions(package, versions, reason, &cause1)
            {
                tree
            } else {
                derived_tree(metadata, cause1, cause2)
            };

            // Merging a cause can also leave two unavailabilities of one package side by side.
            if let DerivationTree::Derived(derived) = &tree
                && let Some(merged) = merge_unavailable_siblings(derived)
            {
                return merged;
            }
            tree
        },
    )
}

fn is_root_dependency_on_project(external: &ErrorExternal, project: &PackageName) -> bool {
    let External::FromDependencyOf(package, _, dependency, _) = external else {
        return false;
    };
    if !matches!(&**package, PubGrubPackageInner::Root(_)) {
        return false;
    }
    matches!(&**dependency, PubGrubPackageInner::Package { name, .. } if name == project)
}

/// Given a [`DerivationTree`], drop dependency incompatibilities from the root to the project.
///
/// This effectively changes the root to the workspace member in a single-project workspace,
/// avoiding an extra level of indirection like "your project requires your project".
///
/// A direct dependency incompatibility is also a traversal boundary: if it is not the
/// root-to-project edge, leave that subtree unchanged. After removing a matching edge, continue
/// through only the opposite cause.
fn drop_root_dependency_on_project(tree: ErrorTree, project: &PackageName) -> ErrorTree {
    let mut tasks = vec![TreeTask::Visit(StackSafeErrorTree::new(tree))];
    let mut results = Vec::new();

    while let Some(task) = tasks.pop() {
        match task {
            TreeTask::Visit(tree) => match tree.into_inner() {
                DerivationTree::External(external) => {
                    results.push(StackSafeErrorTree::new(DerivationTree::External(external)));
                }
                DerivationTree::Derived(derived) => {
                    let first_dependency = match derived.cause1.as_ref() {
                        DerivationTree::External(external @ External::FromDependencyOf(..)) => {
                            Some((external, true))
                        }
                        _ => match derived.cause2.as_ref() {
                            DerivationTree::External(external @ External::FromDependencyOf(..)) => {
                                Some((external, false))
                            }
                            _ => None,
                        },
                    };

                    if let Some((external, dependency_is_cause1)) = first_dependency {
                        if is_root_dependency_on_project(external, project) {
                            let other = if dependency_is_cause1 {
                                Arc::unwrap_or_clone(derived.cause2)
                            } else {
                                Arc::unwrap_or_clone(derived.cause1)
                            };
                            tasks.push(TreeTask::Visit(StackSafeErrorTree::new(other)));
                        } else {
                            results.push(StackSafeErrorTree::new(DerivationTree::Derived(derived)));
                        }
                    } else {
                        schedule_derived(&mut tasks, derived);
                    }
                }
            },
            TreeTask::Rebuild(metadata) => {
                let cause2 = results
                    .pop()
                    .expect("every derived tree has a second reduced cause")
                    .into_inner();
                let cause1 = results
                    .pop()
                    .expect("every derived tree has a first reduced cause")
                    .into_inner();
                results.push(StackSafeErrorTree::new(derived_tree(
                    metadata, cause1, cause2,
                )));
            }
        }
    }

    results
        .pop()
        .expect("the root derivation tree produces one reduced result")
        .into_inner()
}

/// A prefix match, e.g., `==2.4.*`, which is desugared to a range like `>=2.4.dev0,<2.5.dev0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrefixMatch<'a> {
    version: &'a Version,
}

impl<'a> PrefixMatch<'a> {
    /// Determine whether a given range is equivalent to a prefix match (e.g., `==2.4.*`).
    ///
    /// Prefix matches are desugared to (e.g.) `>=2.4.dev0,<2.5.dev0`, but we want to render them
    /// as `==2.4.*` in error messages.
    pub(crate) fn from_range(lower: Bound<&'a Version>, upper: Bound<&'a Version>) -> Option<Self> {
        let Bound::Included(lower) = lower else {
            return None;
        };
        let Bound::Excluded(upper) = upper else {
            return None;
        };
        if lower.is_pre() || lower.is_post() || lower.is_local() {
            return None;
        }
        if upper.is_pre() || upper.is_post() || upper.is_local() {
            return None;
        }
        if lower.dev() != Some(0) {
            return None;
        }
        if upper.dev() != Some(0) {
            return None;
        }
        if lower.release().len() != upper.release().len() {
            return None;
        }

        // All segments should be the same, except the last one, which should be incremented.
        let num_segments = lower.release().len();
        for (i, (lower, upper)) in lower
            .release()
            .iter()
            .zip(upper.release().iter())
            .enumerate()
        {
            if i == num_segments - 1 {
                if lower + 1 != *upper {
                    return None;
                }
            } else {
                if lower != upper {
                    return None;
                }
            }
        }

        Some(PrefixMatch { version: lower })
    }
}

impl std::fmt::Display for PrefixMatch<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "=={}.*", self.version.only_release())
    }
}

#[derive(Debug)]
pub struct NoSolutionHeader {
    /// The [`ResolverEnvironment`] that caused the failure.
    env: ResolverEnvironment,
    /// The additional context for the resolution failure.
    context: Option<&'static str>,
}

impl NoSolutionHeader {
    /// Create a new [`NoSolutionHeader`] with the given [`ResolverEnvironment`].
    pub fn new(env: ResolverEnvironment) -> Self {
        Self { env, context: None }
    }

    /// Set the context for the resolution failure.
    #[must_use]
    pub fn with_context(mut self, context: &'static str) -> Self {
        self.context = Some(context);
        self
    }
}

impl std::fmt::Display for NoSolutionHeader {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match (self.context, self.env.end_user_fork_display()) {
            (None, None) => write!(f, "No solution found when resolving dependencies:"),
            (Some(context), None) => write!(
                f,
                "No solution found when resolving {context} dependencies:"
            ),
            (None, Some(split)) => write!(
                f,
                "No solution found when resolving dependencies for {split}:"
            ),
            (Some(context), Some(split)) => write!(
                f,
                "No solution found when resolving {context} dependencies for {split}:"
            ),
        }
    }
}

/// Given a [`DerivationTree`], simplify version ranges using the included versions for each
/// package.
fn simplify_derivation_tree_ranges(
    tree: ErrorTree,
    included_versions: &FxHashMap<PackageName, BTreeSet<Version>>,
    candidate_selector: &CandidateSelector,
    resolver_environment: &ResolverEnvironment,
) -> ErrorTree {
    map_derivation_tree(
        tree,
        |mut external| {
            match &mut external {
                External::FromDependencyOf(package1, versions1, package2, versions2) => {
                    if let Some(simplified) = simplify_range(
                        versions1,
                        package1,
                        included_versions,
                        candidate_selector,
                        resolver_environment,
                    ) {
                        *versions1 = simplified;
                    }
                    if let Some(simplified) = simplify_range(
                        versions2,
                        package2,
                        included_versions,
                        candidate_selector,
                        resolver_environment,
                    ) {
                        *versions2 = simplified;
                    }
                }
                External::NoVersions(package, versions) => {
                    if let Some(simplified) = simplify_range(
                        versions,
                        package,
                        included_versions,
                        candidate_selector,
                        resolver_environment,
                    ) {
                        *versions = simplified;
                    }
                }
                External::Custom(package, versions, _) => {
                    if let Some(simplified) = simplify_range(
                        versions,
                        package,
                        included_versions,
                        candidate_selector,
                        resolver_environment,
                    ) {
                        *versions = simplified;
                    }
                }
                External::NotRoot(..) => {}
            }
            DerivationTree::External(external)
        },
        |mut metadata, cause1, cause2| {
            metadata.terms = metadata
                .terms
                .into_iter()
                .map(|(package, term)| {
                    let term = match term {
                        Term::Positive(versions) => Term::Positive(
                            simplify_range(
                                &versions,
                                &package,
                                included_versions,
                                candidate_selector,
                                resolver_environment,
                            )
                            .unwrap_or(versions),
                        ),
                        Term::Negative(versions) => Term::Negative(
                            simplify_range(
                                &versions,
                                &package,
                                included_versions,
                                candidate_selector,
                                resolver_environment,
                            )
                            .unwrap_or(versions),
                        ),
                    };
                    (package, term)
                })
                .collect();
            derived_tree(metadata, cause1, cause2)
        },
    )
}

/// Helper function to simplify a version range using included versions for a package.
///
/// If the range cannot be simplified, `None` is returned.
fn simplify_range(
    range: &Range<Version>,
    package: &PubGrubPackage,
    included_versions: &FxHashMap<PackageName, BTreeSet<Version>>,
    candidate_selector: &CandidateSelector,
    resolver_environment: &ResolverEnvironment,
) -> Option<Range<Version>> {
    // If there's not a package name or included versions, we can't simplify anything
    let name = package.name()?;
    let versions = included_versions.get(name)?;

    // If this is a full range, there's nothing to simplify
    if range.encoded_versions() == &Ranges::full() {
        return None;
    }

    // If there's only one version available and it's in the range, return just that version
    if let Some(version) = versions.iter().next() {
        if versions.len() == 1 && range.contains(version) {
            return Some(Range::singleton(version.clone()));
        }
    }

    // Check if pre-releases are allowed
    let prereleases_not_allowed = candidate_selector
        .prerelease_strategy()
        .selection(name, resolver_environment)
        == PrereleaseSelection::Disallow;

    let any_prerelease = range.iter().any(|(start, end)| {
        let is_pre1 = match start {
            Bound::Included(version) => version.any_prerelease(),
            Bound::Excluded(version) => version.any_prerelease(),
            Bound::Unbounded => false,
        };
        let is_pre2 = match end {
            Bound::Included(version) => version.any_prerelease(),
            Bound::Excluded(version) => version.any_prerelease(),
            Bound::Unbounded => false,
        };
        is_pre1 || is_pre2
    });

    // Simplify the range, as implemented in PubGrub
    Some(Range::from_versions(range.simplify(
        versions.iter().filter(|version| {
            // If there are pre-releases in the range segments, we need to include pre-releases
            if any_prerelease {
                return true;
            }

            // If pre-releases are not allowed, filter out pre-releases
            if prereleases_not_allowed && version.any_prerelease() {
                return false;
            }

            // Otherwise, include the version
            true
        }),
    )))
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;
    use crate::resolver::UnavailableVersion;

    fn deep_derivation_tree() -> ErrorTree {
        let package = PubGrubPackage::from(PubGrubPackageInner::Root(None));
        let leaf = ErrorTree::External(External::NotRoot(package, Version::new([1_u64])));
        let mut tree = leaf.clone();

        for _ in 0..100_000 {
            tree = ErrorTree::Derived(Derived {
                terms: pubgrub::Map::default(),
                shared_id: None,
                cause1: Arc::new(tree),
                cause2: Arc::new(leaf.clone()),
            });
        }

        tree
    }

    fn pubgrub_package(name: &str) -> PubGrubPackage {
        PubGrubPackage::from(PubGrubPackageInner::Package {
            name: package_name(name),
            extra: None,
            group: None,
            marker: uv_pep508::MarkerTree::TRUE,
        })
    }

    fn package_name(name: &str) -> PackageName {
        name.parse().expect("valid package name")
    }

    fn version(version: &str) -> Version {
        version.parse().expect("valid version")
    }

    fn known_versions(name: &str, versions: &[&str]) -> FxHashMap<PackageName, Arc<[Version]>> {
        let versions: Arc<[Version]> = versions.iter().copied().map(version).collect();
        FxHashMap::from_iter([(package_name(name), versions)])
    }

    fn unavailable(package: &PubGrubPackage, versions: Range<Version>) -> ErrorTree {
        ErrorTree::External(External::Custom(
            package.clone(),
            versions,
            UnavailableReason::Version(UnavailableVersion::InvalidMetadata),
        ))
    }

    fn narrow_unavailable(
        package: &PubGrubPackage,
        versions: Range<Version>,
        known_versions: &FxHashMap<PackageName, Arc<[Version]>>,
    ) -> Range<Version> {
        let narrowed =
            NoSolutionError::narrow_widened_sets(unavailable(package, versions), known_versions);
        let ErrorTree::External(External::Custom(_, versions, _)) = narrowed else {
            panic!("expected a custom incompatibility");
        };
        versions
    }

    /// A widened unavailable set is narrowed back onto the known versions.
    #[test]
    fn narrows_widened_unavailable_versions() {
        let package = pubgrub_package("numpy");
        let known_versions = known_versions("numpy", &["1.0", "2.0", "3.0"]);

        // A gap-widened rejection reports the single version it excludes.
        let widened = Range::singleton(version("2.0")).widen_versions(&[
            version("1.0"),
            version("2.0"),
            version("3.0"),
        ]);
        assert_eq!(widened.to_string(), ">1.0, <3.0");
        assert_eq!(
            narrow_unavailable(&package, widened, &known_versions),
            Range::singleton(version("2.0"))
        );

        // At the top of a listing the widened set is unbounded, but the rejection says nothing
        // about versions that are not published yet.
        let widened = Range::from_range_bounds(version("2.0")..);
        assert_eq!(
            narrow_unavailable(&package, widened, &known_versions),
            Range::from_range_bounds(version("2.0")..=version("3.0"))
        );

        // Merged rejections covering every known version report as the full range.
        let merged = Range::from_range_bounds(version("0")..);
        assert_eq!(
            narrow_unavailable(&package, merged, &known_versions),
            Range::full()
        );

        // A rejected version outside the known versions has no envelope to report it in.
        let widened = Range::from_range_bounds(version("4.0")..version("5.0"));
        assert_eq!(
            narrow_unavailable(&package, widened.clone(), &known_versions),
            widened
        );

        // A package without a known version list is reported as recorded.
        let widened = Range::from_range_bounds(version("1.0")..version("3.0"));
        assert_eq!(
            narrow_unavailable(&pubgrub_package("scipy"), widened.clone(), &known_versions),
            widened
        );
    }

    /// A rejection of the one version a package has reports that version.
    #[test]
    fn narrows_widened_unavailable_version_of_one_version() {
        let package = pubgrub_package("numpy");
        let known_versions = known_versions("numpy", &["2.0"]);

        assert_eq!(
            narrow_unavailable(&package, Range::full(), &known_versions),
            Range::singleton(version("2.0"))
        );
    }

    /// A conclusion about a package with one version reports that version where its causes do.
    #[test]
    fn narrows_widened_conclusion_of_one_version() {
        let package = pubgrub_package("numpy");
        let known_versions = known_versions("numpy", &["2.0"]);
        let concluded = |cause: ErrorTree| {
            let tree = ErrorTree::Derived(Derived {
                terms: pubgrub::Map::from_iter([(package.clone(), Term::Positive(Range::full()))]),
                shared_id: None,
                cause1: Arc::new(cause),
                cause2: Arc::new(ErrorTree::External(External::NotRoot(
                    PubGrubPackage::from(PubGrubPackageInner::Root(None)),
                    version("1.0"),
                ))),
            });
            let ErrorTree::Derived(narrowed) =
                NoSolutionError::narrow_widened_sets(tree, &known_versions)
            else {
                panic!("expected a derived incompatibility");
            };
            narrowed.terms.get(&package).cloned()
        };

        // The cause reports the version, so the conclusion stops there too.
        assert_eq!(
            concluded(unavailable(&package, Range::full())),
            Some(Term::Positive(Range::singleton(version("2.0"))))
        );

        // A cause that reports every version of the package leaves the conclusion as it is.
        assert_eq!(
            concluded(ErrorTree::External(External::NoVersions(
                package.clone(),
                Range::full(),
            ))),
            Some(Term::Positive(Range::full()))
        );
    }

    /// Two rejections of the same package for the same reason read as one statement.
    #[test]
    fn collapses_sibling_unavailable_versions() {
        let package = pubgrub_package("numpy");
        let cause1 = unavailable(&package, Range::singleton(version("1.0")));
        let cause2 = unavailable(
            &package,
            Range::from_range_bounds(version("2.0")..=version("3.0")),
        );
        let terms = pubgrub::Map::from_iter([(
            package.clone(),
            Term::Positive(Range::from_range_bounds(version("2.0")..=version("3.0"))),
        )]);
        let tree = ErrorTree::Derived(Derived {
            terms,
            shared_id: None,
            cause1: Arc::new(cause1),
            cause2: Arc::new(cause2),
        });

        let collapsed = collapse_unavailable_versions(tree);
        let ErrorTree::External(External::Custom(_, versions, _)) = collapsed else {
            panic!("expected a custom incompatibility");
        };
        assert_eq!(versions.to_string(), "==1.0 | >=2.0, <=3.0");
    }

    /// A derivation that concludes about more than the unavailable package is left alone.
    #[test]
    fn keeps_sibling_unavailable_versions_concluding_about_another_package() {
        let package = pubgrub_package("numpy");
        let cause1 = unavailable(&package, Range::singleton(version("1.0")));
        let cause2 = unavailable(&package, Range::singleton(version("3.0")));
        let terms = pubgrub::Map::from_iter([
            (package.clone(), Term::Positive(Range::full())),
            (pubgrub_package("scipy"), Term::Positive(Range::full())),
        ]);
        let tree = ErrorTree::Derived(Derived {
            terms,
            shared_id: None,
            cause1: Arc::new(cause1),
            cause2: Arc::new(cause2),
        });

        assert_matches!(collapse_unavailable_versions(tree), ErrorTree::Derived(_));
    }

    #[test]
    fn drops_transformed_derivation_tree_without_recursion() -> std::io::Result<()> {
        let thread = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                let _tree = StackSafeErrorTree::new(deep_derivation_tree());
            })?;

        assert!(thread.join().is_ok());
        Ok(())
    }

    #[test]
    fn derivation_tree_packages_are_unique() {
        let tree = StackSafeErrorTree::new(deep_derivation_tree());
        assert_eq!(derivation_tree_packages(&tree).count(), 1);
    }

    #[test]
    fn collapse_proxies_drops_source_tree_without_recursion() -> std::io::Result<()> {
        let thread = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                let tree = NoSolutionError::collapse_proxies(deep_derivation_tree());
                drop_derivation_tree(tree);
            })?;

        assert!(thread.join().is_ok());
        Ok(())
    }

    #[test]
    fn collapse_local_versions_drops_source_tree_without_recursion() -> std::io::Result<()> {
        let thread = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                let tree = NoSolutionError::collapse_local_version_segments(deep_derivation_tree());
                drop_derivation_tree(tree);
            })?;

        assert!(thread.join().is_ok());
        Ok(())
    }

    #[test]
    fn formats_debug_derivation_tree_without_recursion() -> std::io::Result<()> {
        let thread = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                let tree = StackSafeErrorTree::new(deep_derivation_tree());
                let _formatted = format!("{tree:?}");
            })?;

        assert!(thread.join().is_ok());
        Ok(())
    }

    #[test]
    fn iterative_debug_matches_pubgrub_debug() {
        let package = PubGrubPackage::from(PubGrubPackageInner::Root(None));
        let leaf = ErrorTree::External(External::NotRoot(package, Version::new([1_u64])));
        let tree = StackSafeErrorTree::new(ErrorTree::Derived(Derived {
            terms: pubgrub::Map::default(),
            shared_id: Some(1),
            cause1: Arc::new(leaf.clone()),
            cause2: Arc::new(leaf),
        }));

        let inner = &*tree;
        assert_eq!(format!("{inner:?}"), format!("{tree:?}"));
    }
}

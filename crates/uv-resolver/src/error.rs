use std::collections::{BTreeMap, BTreeSet, Bound};
use std::fmt::Formatter;
use std::sync::{Arc, OnceLock};

use indexmap::IndexSet;
use itertools::Itertools;
use owo_colors::OwoColorize;
use pubgrub::{
    DerivationTree as PubGrubDerivationTree, DerivationTreeId as PubGrubDerivationTreeId,
    DerivationTreeNode as PubGrubDerivationTreeNode, External, Map, Range, Ranges, Term,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::trace;

use uv_distribution_types::{
    DerivationChain, DistErrorKind, IndexCapabilities, IndexLocations, IndexUrl, RequestedDist,
};
use uv_normalize::{ExtraName, InvalidNameError, PackageName};
use uv_pep440::{LocalVersionSlice, LowerBound, Version};
use uv_pep508::MarkerEnvironment;
use uv_platform_tags::Tags;
use uv_pypi_types::ParsedUrl;
use uv_redacted::DisplaySafeUrl;
use uv_static::EnvVars;

use crate::candidate_selector::CandidateSelector;
use crate::fork_indexes::ForkIndexes;
use crate::fork_urls::ForkUrls;
use crate::prerelease::AllowPrerelease;
use crate::pubgrub::{
    PubGrubHint, PubGrubPackage, PubGrubPackageInner, PubGrubReportFormatter,
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

type PubGrubErrorTree = PubGrubDerivationTree<PubGrubPackage, Range<Version>, UnavailableReason>;
pub(crate) type ErrorExternal = External<PubGrubPackage, Range<Version>, UnavailableReason>;
pub(crate) type ErrorTerms = Map<PubGrubPackage, Term<Range<Version>>>;

#[derive(Debug, Clone)]
pub struct ErrorTree {
    arena: Vec<ErrorTreeNode>,
    root: ErrorTreeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ErrorTreeId(usize);

#[derive(Debug, Clone)]
pub(crate) enum ErrorTreeNode {
    External(ErrorExternal),
    Derived(ErrorDerived),
}

#[derive(Debug, Clone)]
pub(crate) struct ErrorDerived {
    pub(crate) terms: ErrorTerms,
    pub(crate) shared_id: Option<usize>,
    pub(crate) cause1: ErrorTreeId,
    pub(crate) cause2: ErrorTreeId,
}

#[derive(Debug, Default)]
pub(crate) struct TreeBuilder {
    arena: Vec<ErrorTreeNode>,
}

impl TreeBuilder {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            arena: Vec::with_capacity(capacity),
        }
    }

    fn alloc(&mut self, node: ErrorTreeNode) -> ErrorTreeId {
        let id = ErrorTreeId(self.arena.len());
        self.arena.push(node);
        id
    }

    pub(crate) fn external(&mut self, external: ErrorExternal) -> ErrorTreeId {
        self.alloc(ErrorTreeNode::External(external))
    }

    pub(crate) fn derived(
        &mut self,
        metadata: DerivedMetadata,
        cause1: ErrorTreeId,
        cause2: ErrorTreeId,
    ) -> ErrorTreeId {
        self.alloc(ErrorTreeNode::Derived(ErrorDerived {
            terms: metadata.terms,
            shared_id: metadata.shared_id,
            cause1,
            cause2,
        }))
    }

    fn copy_from(&mut self, tree: &ErrorTree, root: ErrorTreeId) -> ErrorTreeId {
        let mut order = Vec::new();
        let mut seen = FxHashSet::default();
        let mut stack = vec![root];

        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            order.push(id);
            if let ErrorTreeNode::Derived(derived) = tree.node(id) {
                stack.push(derived.cause1);
                stack.push(derived.cause2);
            }
        }

        let mut remapped: FxHashMap<ErrorTreeId, ErrorTreeId> = FxHashMap::default();
        for old_id in order.into_iter().rev() {
            let node = match tree.node(old_id) {
                ErrorTreeNode::External(external) => ErrorTreeNode::External(external.clone()),
                ErrorTreeNode::Derived(derived) => ErrorTreeNode::Derived(ErrorDerived {
                    terms: derived.terms.clone(),
                    shared_id: derived.shared_id,
                    cause1: remapped[&derived.cause1],
                    cause2: remapped[&derived.cause2],
                }),
            };
            let new_id = self.alloc(node);
            remapped.insert(old_id, new_id);
        }

        remapped[&root]
    }

    fn node(&self, id: ErrorTreeId) -> &ErrorTreeNode {
        &self.arena[id.0]
    }

    fn derived_ref(&self, id: ErrorTreeId) -> Option<&ErrorDerived> {
        match self.node(id) {
            ErrorTreeNode::External(_) => None,
            ErrorTreeNode::Derived(derived) => Some(derived),
        }
    }

    pub(crate) fn finish(self, root: ErrorTreeId) -> ErrorTree {
        ErrorTree {
            arena: self.arena,
            root,
        }
    }
}

impl ErrorTree {
    pub(crate) fn from_pubgrub(derivation_tree: PubGrubErrorTree) -> Self {
        let mut order = Vec::new();
        let mut stack = vec![derivation_tree.root_id()];

        while let Some(id) = stack.pop() {
            order.push(id);
            if let PubGrubDerivationTreeNode::Derived(derived) = derivation_tree.node(id) {
                stack.push(derived.cause1);
                stack.push(derived.cause2);
            }
        }

        let mut builder = TreeBuilder::with_capacity(order.len());
        let mut remapped: FxHashMap<PubGrubDerivationTreeId, ErrorTreeId> = FxHashMap::default();
        for old_id in order.into_iter().rev() {
            let node = match derivation_tree.node(old_id) {
                PubGrubDerivationTreeNode::External(external) => {
                    ErrorTreeNode::External(external.clone())
                }
                PubGrubDerivationTreeNode::Derived(derived) => {
                    ErrorTreeNode::Derived(ErrorDerived {
                        terms: derived.terms.clone(),
                        shared_id: derived.shared_id,
                        cause1: remapped[&derived.cause1],
                        cause2: remapped[&derived.cause2],
                    })
                }
            };
            let new_id = builder.alloc(node);
            remapped.insert(old_id, new_id);
        }

        builder.finish(remapped[&derivation_tree.root_id()])
    }

    #[cfg(test)]
    pub(crate) fn external(external: ErrorExternal) -> Self {
        Self {
            arena: vec![ErrorTreeNode::External(external)],
            root: ErrorTreeId(0),
        }
    }

    #[cfg(test)]
    pub(crate) fn derived_from_parts(
        terms: ErrorTerms,
        shared_id: Option<usize>,
        cause1: Self,
        cause2: Self,
    ) -> Self {
        let mut builder = TreeBuilder::with_capacity(cause1.arena.len() + cause2.arena.len() + 1);
        let cause1 = builder.copy_from(&cause1, cause1.root);
        let cause2 = builder.copy_from(&cause2, cause2.root);
        let root = builder.derived(DerivedMetadata { terms, shared_id }, cause1, cause2);
        builder.finish(root)
    }

    pub(crate) fn root_id(&self) -> ErrorTreeId {
        self.root
    }

    pub(crate) fn root(&self) -> &ErrorTreeNode {
        self.node(self.root)
    }

    pub(crate) fn node(&self, id: ErrorTreeId) -> &ErrorTreeNode {
        &self.arena[id.0]
    }
}

/// Visit each distinct package in a derivation tree without recursive calls.
///
/// The iteration order is unspecified, matching the set semantics of
/// [`DerivationTree::packages`].
pub(crate) fn derivation_tree_packages(
    derivation_tree: &ErrorTree,
) -> impl Iterator<Item = &PubGrubPackage> {
    let mut packages = FxHashSet::default();
    let mut trees = vec![derivation_tree.root_id()];

    while let Some(id) = trees.pop() {
        match derivation_tree.node(id) {
            ErrorTreeNode::External(external) => match external {
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
            ErrorTreeNode::Derived(derived) => {
                packages.extend(derived.terms.keys());
                trees.push(derived.cause1);
                trees.push(derived.cause2);
            }
        }
    }

    packages.into_iter()
}

pub(crate) struct DerivedMetadata {
    pub(crate) terms: ErrorTerms,
    pub(crate) shared_id: Option<usize>,
}

enum TreeTask {
    Visit(ErrorTreeId),
    Rebuild(DerivedMetadata),
}

fn schedule_derived(tasks: &mut Vec<TreeTask>, derived: &ErrorDerived) {
    tasks.push(TreeTask::Rebuild(DerivedMetadata {
        terms: derived.terms.clone(),
        shared_id: derived.shared_id,
    }));
    tasks.push(TreeTask::Visit(derived.cause2));
    tasks.push(TreeTask::Visit(derived.cause1));
}

fn transform_derivation_tree(
    derivation_tree: ErrorTree,
    mut transform_external: impl FnMut(ErrorExternal, &mut TreeBuilder) -> Option<ErrorTreeId>,
    mut transform_derived: impl FnMut(
        DerivedMetadata,
        Option<ErrorTreeId>,
        Option<ErrorTreeId>,
        &mut TreeBuilder,
    ) -> Option<ErrorTreeId>,
) -> Option<ErrorTree> {
    let mut tasks = vec![TreeTask::Visit(derivation_tree.root_id())];
    let mut results: Vec<Option<ErrorTreeId>> = Vec::new();
    let mut builder = TreeBuilder::with_capacity(derivation_tree.arena.len());

    while let Some(task) = tasks.pop() {
        match task {
            TreeTask::Visit(id) => match derivation_tree.node(id).clone() {
                ErrorTreeNode::External(external) => {
                    results.push(transform_external(external, &mut builder));
                }
                ErrorTreeNode::Derived(derived) => schedule_derived(&mut tasks, &derived),
            },
            TreeTask::Rebuild(metadata) => {
                let cause2 = results
                    .pop()
                    .expect("every derived tree has a second transformed cause");
                let cause1 = results
                    .pop()
                    .expect("every derived tree has a first transformed cause");
                results.push(transform_derived(metadata, cause1, cause2, &mut builder));
            }
        }
    }

    let root = results
        .pop()
        .expect("the root derivation tree produces one transformed result")?;
    Some(builder.finish(root))
}

fn map_derivation_tree(
    derivation_tree: ErrorTree,
    mut transform_external: impl FnMut(ErrorExternal, &mut TreeBuilder) -> ErrorTreeId,
    mut transform_derived: impl FnMut(
        DerivedMetadata,
        ErrorTreeId,
        ErrorTreeId,
        &mut TreeBuilder,
    ) -> ErrorTreeId,
) -> ErrorTree {
    transform_derivation_tree(
        derivation_tree,
        |external, builder| Some(transform_external(external, builder)),
        |metadata, cause1, cause2, builder| {
            Some(transform_derived(
                metadata,
                cause1.expect("map transformations retain the first cause"),
                cause2.expect("map transformations retain the second cause"),
                builder,
            ))
        },
    )
    .expect("map transformations retain the root")
}

/// A wrapper around [`pubgrub::error::NoSolutionError`] that displays a resolution failure report.
pub struct NoSolutionError {
    error: ErrorTree,
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
        error: ErrorTree,
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
            error,
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
        fn is_proxy(builder: &TreeBuilder, id: ErrorTreeId) -> bool {
            matches!(
                builder.node(id),
                ErrorTreeNode::External(External::FromDependencyOf(package, ..))
                    if package.is_proxy()
            )
        }

        transform_derivation_tree(
            derivation_tree,
            |external, builder| Some(builder.external(external)),
            |metadata, cause1, cause2, builder| match (cause1, cause2) {
                (Some(cause1), Some(cause2))
                    if is_proxy(builder, cause1) && is_proxy(builder, cause2) =>
                {
                    None
                }
                (Some(cause), other) if is_proxy(builder, cause) => other,
                (other, Some(cause)) if is_proxy(builder, cause) => other,
                (Some(cause1), Some(cause2)) => Some(builder.derived(metadata, cause1, cause2)),
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
            |external, builder| match external {
                external @ External::NotRoot(_, _) => Some(builder.external(external)),
                External::NoVersions(package, versions) => {
                    if SentinelRange::from(&versions).is_complement() {
                        return None;
                    }

                    let versions = SentinelRange::from(&versions).strip();
                    Some(builder.external(External::NoVersions(package, versions)))
                }
                External::FromDependencyOf(package1, versions1, package2, versions2) => {
                    let versions1 = SentinelRange::from(&versions1).strip();
                    let versions2 = SentinelRange::from(&versions2).strip();
                    Some(builder.external(External::FromDependencyOf(
                        package1, versions1, package2, versions2,
                    )))
                }
                External::Custom(package, versions, reason) => {
                    let versions = SentinelRange::from(&versions).strip();
                    Some(builder.external(External::Custom(package, versions, reason)))
                }
            },
            |mut metadata, cause1, cause2, builder| {
                metadata.terms = metadata
                    .terms
                    .into_iter()
                    .map(|(package, term)| {
                        let term = match term {
                            Term::Positive(versions) => {
                                Term::Positive(SentinelRange::from(&versions).strip())
                            }
                            Term::Negative(versions) => {
                                Term::Negative(SentinelRange::from(&versions).strip())
                            }
                        };
                        (package, term)
                    })
                    .collect();

                match (cause1, cause2) {
                    (Some(cause1), Some(cause2)) => Some(builder.derived(metadata, cause1, cause2)),
                    (Some(cause), None) | (None, Some(cause)) => Some(cause),
                    (None, None) => None,
                }
            },
        )
        .expect("derivation tree should contain at least one term")
    }

    /// Given a [`DerivationTree`], identify the largest required Python version that is missing.
    pub fn find_requires_python(&self) -> LowerBound {
        let mut minimum = LowerBound::default();
        let mut trees = vec![self.error.root_id()];

        while let Some(id) = trees.pop() {
            match self.error.node(id) {
                ErrorTreeNode::Derived(derived) => {
                    trees.push(derived.cause2);
                    trees.push(derived.cause1);
                }
                ErrorTreeNode::External(External::FromDependencyOf(.., package, version)) => {
                    if let PubGrubPackageInner::Python(_) = &**package {
                        if let Some((lower, ..)) = version.bounding_range() {
                            let lower = LowerBound::new(lower.cloned());
                            if lower > minimum {
                                minimum = lower;
                            }
                        }
                    }
                }
                ErrorTreeNode::External(_) => {}
            }
        }

        minimum
    }

    /// Initialize a [`NoSolutionHeader`] for this error.
    pub fn header(&self) -> NoSolutionHeader {
        NoSolutionHeader::new(self.env.clone())
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
        let mut tree =
            simplify_derivation_tree_markers(self.error.clone(), &self.python_requirement);
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

        // This needs to be applied _after_ simplification of the ranges
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
fn display_tree(error: &ErrorTree, name: &str) {
    let mut lines = Vec::new();
    display_tree_inner(error, &mut lines);
    lines.reverse();

    if std::env::var_os(EnvVars::UV_INTERNAL__SHOW_DERIVATION_TREE).is_some() {
        eprintln!("{name}\n{}", lines.join("\n"));
    } else {
        trace!("{name}\n{}", lines.join("\n"));
    }
}

fn display_tree_inner(error: &ErrorTree, lines: &mut Vec<String>) {
    enum Frame<'a> {
        Tree(ErrorTreeId, usize),
        Terms(&'a ErrorTerms, usize),
    }

    let mut frames = vec![Frame::Tree(error.root_id(), 0)];
    while let Some(frame) = frames.pop() {
        match frame {
            Frame::Tree(id, depth) => match error.node(id) {
                ErrorTreeNode::Derived(derived) => {
                    frames.push(Frame::Terms(&derived.terms, depth));
                    frames.push(Frame::Tree(derived.cause2, depth + 1));
                    frames.push(Frame::Tree(derived.cause1, depth + 1));
                }
                ErrorTreeNode::External(external) => {
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
            },
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

fn no_versions(node: &ErrorTreeNode) -> Option<(&PubGrubPackage, &Range<Version>)> {
    match node {
        ErrorTreeNode::External(External::NoVersions(package, versions)) => {
            Some((package, versions))
        }
        _ => None,
    }
}

fn from_dependency_of(
    node: &ErrorTreeNode,
) -> Option<(
    &PubGrubPackage,
    &Range<Version>,
    &PubGrubPackage,
    &Range<Version>,
)> {
    match node {
        ErrorTreeNode::External(External::FromDependencyOf(
            package,
            versions,
            dependency,
            dependency_versions,
        )) => Some((package, versions, dependency, dependency_versions)),
        _ => None,
    }
}

fn custom_unavailable(
    node: &ErrorTreeNode,
) -> Option<(&PubGrubPackage, &Range<Version>, &UnavailableReason)> {
    match node {
        ErrorTreeNode::External(External::Custom(package, versions, reason)) => {
            Some((package, versions, reason))
        }
        _ => None,
    }
}

fn can_drop_no_versions(
    package: &PubGrubPackage,
    versions: &Range<Version>,
    other: ErrorTreeId,
    parent_terms: &ErrorTerms,
    builder: &TreeBuilder,
) -> bool {
    let package_terms = if let Some(derived) = builder.derived_ref(other) {
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
        |external, builder| builder.external(external),
        |metadata, cause1, cause2, builder| {
            if let Some((package, versions)) = no_versions(builder.node(cause1))
                && can_drop_no_versions(package, versions, cause2, &metadata.terms, builder)
            {
                return cause2;
            }

            if let Some((package, versions)) = no_versions(builder.node(cause2))
                && can_drop_no_versions(package, versions, cause1, &metadata.terms, builder)
            {
                return cause1;
            }

            builder.derived(metadata, cause1, cause2)
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
        |external, builder| builder.external(external),
        |metadata, cause1, cause2, builder| {
            let replacement =
                if let (Some((package, versions)), Some((other_package, other_versions))) = (
                    no_versions(builder.node(cause1)),
                    no_versions(builder.node(cause2)),
                ) && package == other_package
                    && let Some(Term::Positive(term)) = metadata.terms.get(package)
                    && versions.subset_of(term)
                    && other_versions.subset_of(term)
                {
                    Some((package.clone(), term.clone()))
                } else {
                    None
                };

            if let Some((package, term)) = replacement {
                changed = true;
                builder.external(External::NoVersions(package, term))
            } else {
                builder.derived(metadata, cause1, cause2)
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
        |external, builder| builder.external(external),
        |metadata, cause1, cause2, builder| {
            if let Some((package, _)) = no_versions(builder.node(cause1))
                && is_workspace_member(package, workspace_members)
            {
                return cause2;
            }
            if let Some((package, _)) = no_versions(builder.node(cause2))
                && is_workspace_member(package, workspace_members)
            {
                return cause1;
            }
            builder.derived(metadata, cause1, cause2)
        },
    )
}

fn collapse_redundant_dependency_child_id(
    tree: ErrorTreeId,
    package: &PubGrubPackage,
    versions: &Range<Version>,
    builder: &TreeBuilder,
) -> ErrorTreeId {
    let Some(derived) = builder.derived_ref(tree) else {
        return tree;
    };
    let dependency_clause = match (
        no_versions(builder.node(derived.cause1)),
        from_dependency_of(builder.node(derived.cause2)),
    ) {
        (Some((no_versions_package, _)), Some((_, _, dependency_package, dependency_versions)))
            if no_versions_package == dependency_package
                && package == no_versions_package
                && versions.subset_of(dependency_versions) =>
        {
            Some(derived.cause2)
        }
        _ => match (
            from_dependency_of(builder.node(derived.cause1)),
            no_versions(builder.node(derived.cause2)),
        ) {
            (
                Some((_, _, dependency_package, dependency_versions)),
                Some((no_versions_package, _)),
            ) if no_versions_package == dependency_package
                && package == no_versions_package
                && versions.subset_of(dependency_versions) =>
            {
                Some(derived.cause1)
            }
            _ => None,
        },
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
        |external, builder| builder.external(external),
        |metadata, cause1, cause2, builder| {
            let dependency = from_dependency_of(builder.node(cause1))
                .map(|(package, versions, _, _)| (package.clone(), versions.clone(), true))
                .or_else(|| {
                    from_dependency_of(builder.node(cause2))
                        .map(|(package, versions, _, _)| (package.clone(), versions.clone(), false))
                });

            let Some((package, versions, dependency_is_cause1)) = dependency else {
                return builder.derived(metadata, cause1, cause2);
            };

            if dependency_is_cause1 {
                let cause2 =
                    collapse_redundant_dependency_child_id(cause2, &package, &versions, builder);
                return builder.derived(metadata, cause1, cause2);
            }

            let cause1 =
                collapse_redundant_dependency_child_id(cause1, &package, &versions, builder);
            builder.derived(metadata, cause1, cause2)
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
        |mut external, builder| {
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
            builder.external(external)
        },
        |mut metadata, cause1, cause2, builder| {
            metadata.terms = metadata
                .terms
                .into_iter()
                .map(|(mut package, term)| {
                    package.simplify_markers(python_requirement);
                    (package, term)
                })
                .collect();
            builder.derived(metadata, cause1, cause2)
        },
    )
}

fn merge_unavailable_versions(
    package: &PubGrubPackage,
    versions: &Range<Version>,
    reason: &UnavailableReason,
    other: ErrorTreeId,
    builder: &mut TreeBuilder,
) -> Option<ErrorTreeId> {
    let Some(derived) = builder.derived_ref(other) else {
        return None;
    };
    let cause1 = derived.cause1;
    let cause2 = derived.cause2;
    let shared_id = derived.shared_id;
    let mut terms = derived.terms.clone();

    let merge = |builder: &TreeBuilder, cause: ErrorTreeId| {
        let (other_package, other_versions, other_reason) =
            custom_unavailable(builder.node(cause))?;
        (package == other_package && reason == other_reason).then(|| other_versions.union(versions))
    };

    // Keep the two cases separate to preserve the ordering of the causes.
    let (unchanged_cause, merged_versions, merged_is_cause2) =
        if let Some(merged_versions) = merge(builder, cause2) {
            (cause1, merged_versions, true)
        } else if let Some(merged_versions) = merge(builder, cause1) {
            (cause2, merged_versions, false)
        } else {
            return None;
        };

    let merged_cause = builder.external(External::Custom(
        package.clone(),
        merged_versions.clone(),
        reason.clone(),
    ));
    let (cause1, cause2) = if merged_is_cause2 {
        (unchanged_cause, merged_cause)
    } else {
        (merged_cause, unchanged_cause)
    };

    if let Some(Term::Positive(range)) = terms.get_mut(package) {
        *range = merged_versions;
    }
    Some(builder.derived(DerivedMetadata { terms, shared_id }, cause1, cause2))
}

/// Given a [`DerivationTree`], collapse incompatibilities for versions of a package that are
/// unavailable for the same reason to avoid repeating the same message for every unavailable
/// version.
fn collapse_unavailable_versions(tree: ErrorTree) -> ErrorTree {
    map_derivation_tree(
        tree,
        |external, builder| builder.external(external),
        |metadata, cause1, cause2, builder| {
            let custom = custom_unavailable(builder.node(cause1))
                .map(|(package, versions, reason)| {
                    (package.clone(), versions.clone(), reason.clone(), cause2)
                })
                .or_else(|| {
                    custom_unavailable(builder.node(cause2)).map(|(package, versions, reason)| {
                        (package.clone(), versions.clone(), reason.clone(), cause1)
                    })
                });

            if let Some((package, versions, reason, other)) = custom
                && let Some(tree) =
                    merge_unavailable_versions(&package, &versions, &reason, other, builder)
            {
                return tree;
            }

            builder.derived(metadata, cause1, cause2)
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
    let mut tasks = vec![TreeTask::Visit(tree.root_id())];
    let mut results = Vec::new();
    let mut builder = TreeBuilder::with_capacity(tree.arena.len());

    while let Some(task) = tasks.pop() {
        match task {
            TreeTask::Visit(id) => match tree.node(id).clone() {
                ErrorTreeNode::External(external) => {
                    results.push(builder.external(external));
                }
                ErrorTreeNode::Derived(derived) => {
                    let first_dependency = match tree.node(derived.cause1) {
                        ErrorTreeNode::External(external @ External::FromDependencyOf(..)) => {
                            Some((external, true))
                        }
                        _ => match tree.node(derived.cause2) {
                            ErrorTreeNode::External(external @ External::FromDependencyOf(..)) => {
                                Some((external, false))
                            }
                            _ => None,
                        },
                    };

                    if let Some((external, dependency_is_cause1)) = first_dependency {
                        if is_root_dependency_on_project(external, project) {
                            let other_id = if dependency_is_cause1 {
                                derived.cause2
                            } else {
                                derived.cause1
                            };
                            tasks.push(TreeTask::Visit(other_id));
                        } else {
                            results.push(builder.copy_from(&tree, id));
                        }
                    } else {
                        schedule_derived(&mut tasks, &derived);
                    }
                }
            },
            TreeTask::Rebuild(metadata) => {
                let cause2 = results
                    .pop()
                    .expect("every derived tree has a second reduced cause");
                let cause1 = results
                    .pop()
                    .expect("every derived tree has a first reduced cause");
                results.push(builder.derived(metadata, cause1, cause2));
            }
        }
    }

    let root = results
        .pop()
        .expect("the root derivation tree produces one reduced result");
    builder.finish(root)
}

/// A version range that may include local version sentinels (`+[max]`).
#[derive(Debug)]
pub struct SentinelRange<'range>(&'range Range<Version>);

impl<'range> From<&'range Range<Version>> for SentinelRange<'range> {
    fn from(range: &'range Range<Version>) -> Self {
        Self(range)
    }
}

impl SentinelRange<'_> {
    /// Returns `true` if the range appears to be, e.g., `>=1.0.0, <1.0.0+[max]`.
    pub(crate) fn is_sentinel(&self) -> bool {
        self.0.iter().all(|(lower, upper)| {
            let (Bound::Included(lower), Bound::Excluded(upper)) = (lower, upper) else {
                return false;
            };
            if !lower.local().is_empty() {
                return false;
            }
            if upper.local() != LocalVersionSlice::Max {
                return false;
            }
            *lower == upper.clone().without_local()
        })
    }

    /// Returns `true` if the range appears to be, e.g., `>1.0.0, <1.0.0+[max]` (i.e., a sentinel
    /// range with the non-local version removed).
    fn is_complement(&self) -> bool {
        self.0.iter().all(|(lower, upper)| {
            let (Bound::Excluded(lower), Bound::Excluded(upper)) = (lower, upper) else {
                return false;
            };
            if !lower.local().is_empty() {
                return false;
            }
            if upper.local() != LocalVersionSlice::Max {
                return false;
            }
            *lower == upper.clone().without_local()
        })
    }

    /// Remove local versions sentinels (`+[max]`) from the version ranges.
    pub fn strip(&self) -> Ranges<Version> {
        self.0
            .iter()
            .map(|(lower, upper)| Self::strip_sentinel(lower.clone(), upper.clone()))
            .collect()
    }

    /// Remove local versions sentinels (`+[max]`) from the interval.
    fn strip_sentinel(
        mut lower: Bound<Version>,
        mut upper: Bound<Version>,
    ) -> (Bound<Version>, Bound<Version>) {
        match (&lower, &upper) {
            (Bound::Unbounded, Bound::Unbounded) => {}
            (Bound::Unbounded, Bound::Included(v)) => {
                // `<=1.0.0+[max]` is equivalent to `<=1.0.0`
                if v.local() == LocalVersionSlice::Max {
                    upper = Bound::Included(v.clone().without_local());
                }
            }
            (Bound::Unbounded, Bound::Excluded(v)) => {
                // `<1.0.0+[max]` is equivalent to `<1.0.0`
                if v.local() == LocalVersionSlice::Max {
                    upper = Bound::Excluded(v.clone().without_local());
                }
            }
            (Bound::Included(v), Bound::Unbounded) => {
                // `>=1.0.0+[max]` is equivalent to `>1.0.0`
                if v.local() == LocalVersionSlice::Max {
                    lower = Bound::Excluded(v.clone().without_local());
                }
            }
            (Bound::Included(v), Bound::Included(b)) => {
                // `>=1.0.0+[max]` is equivalent to `>1.0.0`
                if v.local() == LocalVersionSlice::Max {
                    lower = Bound::Excluded(v.clone().without_local());
                }
                // `<=1.0.0+[max]` is equivalent to `<=1.0.0`
                if b.local() == LocalVersionSlice::Max {
                    upper = Bound::Included(b.clone().without_local());
                }
            }
            (Bound::Included(v), Bound::Excluded(b)) => {
                // `>=1.0.0+[max]` is equivalent to `>1.0.0`
                if v.local() == LocalVersionSlice::Max {
                    lower = Bound::Excluded(v.clone().without_local());
                }
                // `<1.0.0+[max]` is equivalent to `<1.0.0`
                if b.local() == LocalVersionSlice::Max {
                    upper = Bound::Included(b.clone().without_local());
                }
            }
            (Bound::Excluded(v), Bound::Unbounded) => {
                // `>1.0.0+[max]` is equivalent to `>1.0.0`
                if v.local() == LocalVersionSlice::Max {
                    lower = Bound::Excluded(v.clone().without_local());
                }
            }
            (Bound::Excluded(v), Bound::Included(b)) => {
                // `>1.0.0+[max]` is equivalent to `>1.0.0`
                if v.local() == LocalVersionSlice::Max {
                    lower = Bound::Excluded(v.clone().without_local());
                }
                // `<=1.0.0+[max]` is equivalent to `<=1.0.0`
                if b.local() == LocalVersionSlice::Max {
                    upper = Bound::Included(b.clone().without_local());
                }
            }
            (Bound::Excluded(v), Bound::Excluded(b)) => {
                // `>1.0.0+[max]` is equivalent to `>1.0.0`
                if v.local() == LocalVersionSlice::Max {
                    lower = Bound::Excluded(v.clone().without_local());
                }
                // `<1.0.0+[max]` is equivalent to `<1.0.0`
                if b.local() == LocalVersionSlice::Max {
                    upper = Bound::Excluded(b.clone().without_local());
                }
            }
        }
        (lower, upper)
    }
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
    pub(crate) fn from_range(lower: &'a Bound<Version>, upper: &'a Bound<Version>) -> Option<Self> {
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
    fn new(env: ResolverEnvironment) -> Self {
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
        |mut external, builder| {
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
            builder.external(external)
        },
        |mut metadata, cause1, cause2, builder| {
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
            builder.derived(metadata, cause1, cause2)
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
    if range == &Range::full() {
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
        .allows(name, resolver_environment)
        != AllowPrerelease::Yes;

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
    Some(range.simplify(versions.iter().filter(|version| {
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
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deep_derivation_tree() -> ErrorTree {
        let package = PubGrubPackage::from(PubGrubPackageInner::Root(None));
        let mut builder = TreeBuilder::default();
        let leaf = builder.external(External::NotRoot(package, Version::new([1_u64])));
        let mut root = leaf;

        for _ in 0..100_000 {
            root = builder.derived(
                DerivedMetadata {
                    terms: pubgrub::Map::default(),
                    shared_id: None,
                },
                root,
                leaf,
            );
        }

        builder.finish(root)
    }

    #[test]
    fn drops_transformed_derivation_tree_without_recursion() -> std::io::Result<()> {
        let thread = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                let _tree = deep_derivation_tree();
            })?;

        assert!(thread.join().is_ok());
        Ok(())
    }

    #[test]
    fn derivation_tree_packages_are_unique() {
        let tree = deep_derivation_tree();
        assert_eq!(derivation_tree_packages(&tree).count(), 1);
    }

    #[test]
    fn collapse_proxies_drops_source_tree_without_recursion() -> std::io::Result<()> {
        let thread = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                let tree = NoSolutionError::collapse_proxies(deep_derivation_tree());
                drop(tree);
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
                drop(tree);
            })?;

        assert!(thread.join().is_ok());
        Ok(())
    }

    #[test]
    fn formats_debug_derivation_tree_without_recursion() -> std::io::Result<()> {
        let thread = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                let tree = deep_derivation_tree();
                let _formatted = format!("{tree:?}");
            })?;

        assert!(thread.join().is_ok());
        Ok(())
    }
}

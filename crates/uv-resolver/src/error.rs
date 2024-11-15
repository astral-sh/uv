use std::collections::{BTreeMap, BTreeSet, Bound};
use std::fmt::Formatter;
use std::sync::Arc;

use indexmap::IndexSet;
use pubgrub::{
    DefaultStringReporter, DerivationTree, Derived, External, Range, Ranges, Reporter, Term,
};
use rustc_hash::FxHashMap;
use tracing::trace;

use uv_distribution_types::{
    BuiltDist, DerivationChain, IndexCapabilities, IndexLocations, IndexUrl, InstalledDist,
    SourceDist,
};
use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{LocalVersionSlice, Version};
use uv_static::EnvVars;

use crate::candidate_selector::CandidateSelector;
use crate::dependency_provider::UvDependencyProvider;
use crate::fork_urls::ForkUrls;
use crate::pubgrub::{PubGrubPackage, PubGrubPackageInner, PubGrubReportFormatter};
use crate::python_requirement::PythonRequirement;
use crate::resolution::ConflictingDistributionError;
use crate::resolver::{
    IncompletePackage, ResolverEnvironment, UnavailablePackage, UnavailableReason,
};
use crate::Options;

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error(transparent)]
    Client(#[from] uv_client::Error),

    #[error(transparent)]
    Distribution(#[from] uv_distribution::Error),

    #[error("The channel closed unexpectedly")]
    ChannelClosed,

    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),

    #[error("Attempted to wait on an unregistered task: `{_0}`")]
    UnregisteredTask(String),

    #[error("Found conflicting extra `{extra}` unconditionally enabled in `{requirement}`")]
    ConflictingExtra {
        // Boxed because `Requirement` is large.
        requirement: Box<uv_pypi_types::Requirement>,
        extra: ExtraName,
    },

    #[error("Overrides contain conflicting URLs for package `{0}`:\n- {1}\n- {2}")]
    ConflictingOverrideUrls(PackageName, String, String),

    #[error(
        "Requirements contain conflicting URLs for package `{package_name}`{}:\n- {}",
        if env.marker_environment().is_some() {
            String::new()
        } else {
            format!(" in {env}")
        },
        urls.join("\n- "),
    )]
    ConflictingUrls {
        package_name: PackageName,
        urls: Vec<String>,
        env: ResolverEnvironment,
    },

    #[error(
        "Requirements contain conflicting indexes for package `{package_name}`{}:\n- {}",
        if env.marker_environment().is_some() {
            String::new()
        } else {
            format!(" in {env}")
        },
        indexes.join("\n- "),
    )]
    ConflictingIndexesForEnvironment {
        package_name: PackageName,
        indexes: Vec<String>,
        env: ResolverEnvironment,
    },

    #[error("Requirements contain conflicting indexes for package `{0}`: `{1}` vs. `{2}`")]
    ConflictingIndexes(PackageName, String, String),

    #[error("Package `{0}` attempted to resolve via URL: {1}. URL dependencies must be expressed as direct requirements or constraints. Consider adding `{0} @ {1}` to your dependencies or constraints file.")]
    DisallowedUrl(PackageName, String),

    #[error(transparent)]
    DistributionType(#[from] uv_distribution_types::Error),

    #[error(transparent)]
    ParsedUrl(#[from] uv_pypi_types::ParsedUrlError),

    #[error("Failed to download `{0}`")]
    Download(
        Box<BuiltDist>,
        DerivationChain,
        #[source] Arc<uv_distribution::Error>,
    ),

    #[error("Failed to download and build `{0}`")]
    DownloadAndBuild(
        Box<SourceDist>,
        DerivationChain,
        #[source] Arc<uv_distribution::Error>,
    ),

    #[error("Failed to read `{0}`")]
    Read(
        Box<BuiltDist>,
        DerivationChain,
        #[source] Arc<uv_distribution::Error>,
    ),

    // TODO(zanieb): Use `thiserror` in `InstalledDist` so we can avoid chaining `anyhow`
    #[error("Failed to read metadata from installed package `{0}`")]
    ReadInstalled(Box<InstalledDist>, DerivationChain, #[source] anyhow::Error),

    #[error("Failed to build `{0}`")]
    Build(
        Box<SourceDist>,
        DerivationChain,
        #[source] Arc<uv_distribution::Error>,
    ),

    #[error(transparent)]
    NoSolution(#[from] NoSolutionError),

    #[error("Attempted to construct an invalid version specifier")]
    InvalidVersion(#[from] uv_pep440::VersionSpecifierBuildError),

    #[error("In `--require-hashes` mode, all requirements must be pinned upfront with `==`, but found: `{0}`")]
    UnhashedPackage(PackageName),

    #[error("found conflicting distribution in resolution: {0}")]
    ConflictingDistribution(ConflictingDistributionError),

    #[error("Package `{0}` is unavailable")]
    PackageUnavailable(PackageName),
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
    available_versions: FxHashMap<PackageName, BTreeSet<Version>>,
    available_indexes: FxHashMap<PackageName, BTreeSet<IndexUrl>>,
    selector: CandidateSelector,
    python_requirement: PythonRequirement,
    index_locations: IndexLocations,
    index_capabilities: IndexCapabilities,
    unavailable_packages: FxHashMap<PackageName, UnavailablePackage>,
    incomplete_packages: FxHashMap<PackageName, BTreeMap<Version, IncompletePackage>>,
    fork_urls: ForkUrls,
    env: ResolverEnvironment,
    workspace_members: BTreeSet<PackageName>,
    options: Options,
}

impl NoSolutionError {
    /// Create a new [`NoSolutionError`] from a [`pubgrub::NoSolutionError`].
    pub(crate) fn new(
        error: pubgrub::NoSolutionError<UvDependencyProvider>,
        available_versions: FxHashMap<PackageName, BTreeSet<Version>>,
        available_indexes: FxHashMap<PackageName, BTreeSet<IndexUrl>>,
        selector: CandidateSelector,
        python_requirement: PythonRequirement,
        index_locations: IndexLocations,
        index_capabilities: IndexCapabilities,
        unavailable_packages: FxHashMap<PackageName, UnavailablePackage>,
        incomplete_packages: FxHashMap<PackageName, BTreeMap<Version, IncompletePackage>>,
        fork_urls: ForkUrls,
        env: ResolverEnvironment,
        workspace_members: BTreeSet<PackageName>,
        options: Options,
    ) -> Self {
        Self {
            error,
            available_versions,
            available_indexes,
            selector,
            python_requirement,
            index_locations,
            index_capabilities,
            unavailable_packages,
            incomplete_packages,
            fork_urls,
            env,
            workspace_members,
            options,
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

    /// Simplifies the version ranges on any incompatibilities to remove the `[max]` sentinel.
    ///
    /// The `[max]` sentinel is used to represent the maximum local version of a package, to
    /// implement PEP 440 semantics for local version equality. For example, `1.0.0+foo` needs to
    /// satisfy `==1.0.0`.
    pub(crate) fn collapse_local_version_segments(derivation_tree: ErrorTree) -> ErrorTree {
        fn strip(derivation_tree: ErrorTree) -> Option<ErrorTree> {
            match derivation_tree {
                DerivationTree::External(External::NotRoot(_, _)) => Some(derivation_tree),
                DerivationTree::External(External::NoVersions(package, versions)) => {
                    if SentinelRange::from(&versions).is_sentinel() {
                        return None;
                    }

                    let versions = SentinelRange::from(&versions).strip();
                    Some(DerivationTree::External(External::NoVersions(
                        package, versions,
                    )))
                }
                DerivationTree::External(External::FromDependencyOf(
                    package1,
                    versions1,
                    package2,
                    versions2,
                )) => {
                    let versions1 = SentinelRange::from(&versions1).strip();
                    let versions2 = SentinelRange::from(&versions2).strip();
                    Some(DerivationTree::External(External::FromDependencyOf(
                        package1, versions1, package2, versions2,
                    )))
                }
                DerivationTree::External(External::Custom(package, versions, reason)) => {
                    let versions = SentinelRange::from(&versions).strip();
                    Some(DerivationTree::External(External::Custom(
                        package, versions, reason,
                    )))
                }
                DerivationTree::Derived(mut derived) => {
                    let cause1 = strip((*derived.cause1).clone());
                    let cause2 = strip((*derived.cause2).clone());
                    match (cause1, cause2) {
                        (Some(cause1), Some(cause2)) => Some(DerivationTree::Derived(Derived {
                            cause1: Arc::new(cause1),
                            cause2: Arc::new(cause2),
                            terms: std::mem::take(&mut derived.terms)
                                .into_iter()
                                .map(|(pkg, term)| {
                                    let term = match term {
                                        Term::Positive(versions) => {
                                            Term::Positive(SentinelRange::from(&versions).strip())
                                        }
                                        Term::Negative(versions) => {
                                            Term::Negative(SentinelRange::from(&versions).strip())
                                        }
                                    };
                                    (pkg, term)
                                })
                                .collect(),
                            shared_id: derived.shared_id,
                        })),
                        (Some(cause), None) | (None, Some(cause)) => Some(cause),
                        _ => None,
                    }
                }
            }
        }

        strip(derivation_tree).expect("derivation tree should contain at least one term")
    }

    /// Initialize a [`NoSolutionHeader`] for this error.
    pub fn header(&self) -> NoSolutionHeader {
        NoSolutionHeader::new(self.env.clone())
    }
}

impl std::error::Error for NoSolutionError {}

impl std::fmt::Display for NoSolutionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Write the derivation report.
        let formatter = PubGrubReportFormatter {
            available_versions: &self.available_versions,
            python_requirement: &self.python_requirement,
            workspace_members: &self.workspace_members,
        };

        // Transform the error tree for reporting
        let mut tree = self.error.clone();
        simplify_derivation_tree_markers(&self.python_requirement, &mut tree);
        let should_display_tree = std::env::var_os(EnvVars::UV_INTERNAL__SHOW_DERIVATION_TREE)
            .is_some()
            || tracing::enabled!(tracing::Level::TRACE);

        if should_display_tree {
            display_tree(&tree, "Resolver derivation tree before reduction");
        }

        collapse_no_versions_of_workspace_members(&mut tree, &self.workspace_members);

        if self.workspace_members.len() == 1 {
            let project = self.workspace_members.iter().next().unwrap();
            drop_root_dependency_on_project(&mut tree, project);
        }

        collapse_unavailable_versions(&mut tree);
        collapse_redundant_depends_on_no_versions(&mut tree);

        if should_display_tree {
            display_tree(&tree, "Resolver derivation tree after reduction");
        }

        let report = DefaultStringReporter::report_with_formatter(&tree, &formatter);
        write!(f, "{report}")?;

        // Include any additional hints.
        let mut additional_hints = IndexSet::default();
        formatter.generate_hints(
            &tree,
            &self.selector,
            &self.index_locations,
            &self.index_capabilities,
            &self.available_indexes,
            &self.unavailable_packages,
            &self.incomplete_packages,
            &self.fork_urls,
            &self.env,
            &self.workspace_members,
            self.options,
            &mut additional_hints,
        );
        for hint in additional_hints {
            write!(f, "\n\n{hint}")?;
        }

        Ok(())
    }
}

#[allow(clippy::print_stderr)]
fn display_tree(
    error: &DerivationTree<PubGrubPackage, Range<Version>, UnavailableReason>,
    name: &str,
) {
    let mut lines = Vec::new();
    display_tree_inner(error, &mut lines, 0);
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
    depth: usize,
) {
    match error {
        DerivationTree::Derived(derived) => {
            display_tree_inner(&derived.cause1, lines, depth + 1);
            display_tree_inner(&derived.cause2, lines, depth + 1);
        }
        DerivationTree::External(external) => {
            let prefix = "  ".repeat(depth).to_string();
            match external {
                External::FromDependencyOf(package, version, dependency, dependency_version) => {
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
    }
}

/// Given a [`DerivationTree`], collapse any `NoVersion` incompatibilities for workspace members
/// to avoid saying things like "only <workspace-member>==0.1.0 is available".
fn collapse_no_versions_of_workspace_members(
    tree: &mut DerivationTree<PubGrubPackage, Range<Version>, UnavailableReason>,
    workspace_members: &BTreeSet<PackageName>,
) {
    match tree {
        DerivationTree::External(_) => {}
        DerivationTree::Derived(derived) => {
            match (
                Arc::make_mut(&mut derived.cause1),
                Arc::make_mut(&mut derived.cause2),
            ) {
                // If we have a node for a package with no versions...
                (DerivationTree::External(External::NoVersions(package, _)), ref mut other)
                | (ref mut other, DerivationTree::External(External::NoVersions(package, _))) => {
                    // First, always recursively visit the other side of the tree
                    collapse_no_versions_of_workspace_members(other, workspace_members);

                    // Then, if the package is a workspace member...
                    let (PubGrubPackageInner::Package { name, .. }
                    | PubGrubPackageInner::Extra { name, .. }
                    | PubGrubPackageInner::Dev { name, .. }) = &**package
                    else {
                        return;
                    };
                    if !workspace_members.contains(name) {
                        return;
                    }

                    // Replace this node with the other tree
                    *tree = other.clone();
                }
                // If not, just recurse
                _ => {
                    collapse_no_versions_of_workspace_members(
                        Arc::make_mut(&mut derived.cause1),
                        workspace_members,
                    );
                    collapse_no_versions_of_workspace_members(
                        Arc::make_mut(&mut derived.cause2),
                        workspace_members,
                    );
                }
            }
        }
    }
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
/// We can simplify this to `C depends A>=1 and A>=1 depends on B so C depends on B` without
/// explaining that there are no other versions of A. This is dependent on range of A in "A depends
/// on" being a subset of range of A in "depends on A". For example, in a tree like:
///
/// ```text
/// A>=1,<3 depends on B
///     A has no versions >2,<3
///     C depends on A>=2,<3
/// ```
///
/// We cannot say `C depends on A>=2 and A>=1 depends on B so C depends on B` because there is a
/// hole in the range — `A>=1,<3` is not a subset of `A>=2,<3`.
fn collapse_redundant_depends_on_no_versions(
    tree: &mut DerivationTree<PubGrubPackage, Range<Version>, UnavailableReason>,
) {
    match tree {
        DerivationTree::External(_) => {}
        DerivationTree::Derived(derived) => {
            // If one node is a dependency incompatibility...
            match (
                Arc::make_mut(&mut derived.cause1),
                Arc::make_mut(&mut derived.cause2),
            ) {
                (
                    DerivationTree::External(External::FromDependencyOf(package, versions, _, _)),
                    ref mut other,
                )
                | (
                    ref mut other,
                    DerivationTree::External(External::FromDependencyOf(package, versions, _, _)),
                ) => {
                    // Check if the other node is the relevant form of subtree...
                    collapse_redundant_depends_on_no_versions_inner(other, package, versions);
                }
                // If not, just recurse
                _ => {
                    collapse_redundant_depends_on_no_versions(Arc::make_mut(&mut derived.cause1));
                    collapse_redundant_depends_on_no_versions(Arc::make_mut(&mut derived.cause2));
                }
            }
        }
    }
}

/// Helper for [`collapse_redundant_depends_on_no_versions`].
fn collapse_redundant_depends_on_no_versions_inner(
    tree: &mut DerivationTree<PubGrubPackage, Range<Version>, UnavailableReason>,
    package: &PubGrubPackage,
    versions: &Range<Version>,
) {
    match tree {
        DerivationTree::External(_) => {}
        DerivationTree::Derived(derived) => {
            // If we're a subtree with dependency and no versions incompatibilities...
            match (&*derived.cause1, &*derived.cause2) {
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
                )
                // And these incompatibilities (and the parent incompatibility) all are referring to
                // the same package...
                if no_versions_package == dependency_package
                    && package == no_versions_package
                // And parent dependency versions are a subset of the versions in this tree...
                    && versions.subset_of(dependency_versions) =>
                {
                    // Enumerating the available versions will be redundant and we can drop the no
                    // versions clause entirely in favor of the dependency clause.
                    *tree = dependency_clause.clone();

                    // Note we are at a leaf of the tree so there's no further recursion to do
                }
                // If not, just recurse
                _ => {
                    collapse_redundant_depends_on_no_versions(Arc::make_mut(&mut derived.cause1));
                    collapse_redundant_depends_on_no_versions(Arc::make_mut(&mut derived.cause2));
                }
            }
        }
    }
}

/// Simplifies the markers on pubgrub packages in the given derivation tree
/// according to the given Python requirement.
///
/// For example, when there's a dependency like `foo ; python_version >=
/// '3.11'` and `requires-python = '>=3.11'`, this simplification will remove
/// the `python_version >= '3.11'` marker since it's implied to be true by
/// the `requires-python` setting. This simplifies error messages by reducing
/// noise.
fn simplify_derivation_tree_markers(
    python_requirement: &PythonRequirement,
    tree: &mut DerivationTree<PubGrubPackage, Range<Version>, UnavailableReason>,
) {
    match tree {
        DerivationTree::External(External::NotRoot(ref mut pkg, _)) => {
            pkg.simplify_markers(python_requirement);
        }
        DerivationTree::External(External::NoVersions(ref mut pkg, _)) => {
            pkg.simplify_markers(python_requirement);
        }
        DerivationTree::External(External::FromDependencyOf(ref mut pkg1, _, ref mut pkg2, _)) => {
            pkg1.simplify_markers(python_requirement);
            pkg2.simplify_markers(python_requirement);
        }
        DerivationTree::External(External::Custom(ref mut pkg, _, _)) => {
            pkg.simplify_markers(python_requirement);
        }
        DerivationTree::Derived(derived) => {
            derived.terms = std::mem::take(&mut derived.terms)
                .into_iter()
                .map(|(mut pkg, term)| {
                    pkg.simplify_markers(python_requirement);
                    (pkg, term)
                })
                .collect();
            simplify_derivation_tree_markers(
                python_requirement,
                Arc::make_mut(&mut derived.cause1),
            );
            simplify_derivation_tree_markers(
                python_requirement,
                Arc::make_mut(&mut derived.cause2),
            );
        }
    }
}

/// Given a [`DerivationTree`], collapse incompatibilities for versions of a package that are
/// unavailable for the same reason to avoid repeating the same message for every unavailable
/// version.
fn collapse_unavailable_versions(
    tree: &mut DerivationTree<PubGrubPackage, Range<Version>, UnavailableReason>,
) {
    match tree {
        DerivationTree::External(_) => {}
        DerivationTree::Derived(derived) => {
            match (
                Arc::make_mut(&mut derived.cause1),
                Arc::make_mut(&mut derived.cause2),
            ) {
                // If we have a node for unavailable package versions
                (
                    DerivationTree::External(External::Custom(package, versions, reason)),
                    ref mut other,
                )
                | (
                    ref mut other,
                    DerivationTree::External(External::Custom(package, versions, reason)),
                ) => {
                    // First, recursively collapse the other side of the tree
                    collapse_unavailable_versions(other);

                    // If it's not a derived tree, nothing to do.
                    let DerivationTree::Derived(Derived {
                        terms,
                        shared_id,
                        cause1,
                        cause2,
                    }) = other
                    else {
                        return;
                    };

                    // If the other tree has an unavailable package...
                    match (&**cause1, &**cause2) {
                        // Note the following cases are the same, but we need two matches to retain
                        // the ordering of the causes
                        (
                            _,
                            DerivationTree::External(External::Custom(
                                other_package,
                                other_versions,
                                other_reason,
                            )),
                        ) => {
                            // And the package and reason are the same...
                            if package == other_package && reason == other_reason {
                                // Collapse both into a new node, with a union of their ranges
                                *tree = DerivationTree::Derived(Derived {
                                    terms: terms.clone(),
                                    shared_id: *shared_id,
                                    cause1: cause1.clone(),
                                    cause2: Arc::new(DerivationTree::External(External::Custom(
                                        package.clone(),
                                        versions.union(other_versions),
                                        reason.clone(),
                                    ))),
                                });
                            }
                        }
                        (
                            DerivationTree::External(External::Custom(
                                other_package,
                                other_versions,
                                other_reason,
                            )),
                            _,
                        ) => {
                            // And the package and reason are the same...
                            if package == other_package && reason == other_reason {
                                // Collapse both into a new node, with a union of their ranges
                                *tree = DerivationTree::Derived(Derived {
                                    terms: terms.clone(),
                                    shared_id: *shared_id,
                                    cause1: Arc::new(DerivationTree::External(External::Custom(
                                        package.clone(),
                                        versions.union(other_versions),
                                        reason.clone(),
                                    ))),
                                    cause2: cause2.clone(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                // If not, just recurse
                _ => {
                    collapse_unavailable_versions(Arc::make_mut(&mut derived.cause1));
                    collapse_unavailable_versions(Arc::make_mut(&mut derived.cause2));
                }
            }
        }
    }
}

/// Given a [`DerivationTree`], drop dependency incompatibilities from the root
/// to the project.
///
/// Intended to effectively change the root to a workspace member in single project
/// workspaces, avoiding a level of indirection like "And because your project
/// requires your project, we can conclude that your projects's requirements are
/// unsatisfiable."
fn drop_root_dependency_on_project(
    tree: &mut DerivationTree<PubGrubPackage, Range<Version>, UnavailableReason>,
    project: &PackageName,
) {
    match tree {
        DerivationTree::External(_) => {}
        DerivationTree::Derived(derived) => {
            match (
                Arc::make_mut(&mut derived.cause1),
                Arc::make_mut(&mut derived.cause2),
            ) {
                // If one node is a dependency incompatibility...
                (
                    DerivationTree::External(External::FromDependencyOf(package, _, dependency, _)),
                    ref mut other,
                )
                | (
                    ref mut other,
                    DerivationTree::External(External::FromDependencyOf(package, _, dependency, _)),
                ) => {
                    // And the parent is the root package...
                    if !matches!(&**package, PubGrubPackageInner::Root(_)) {
                        return;
                    }

                    // And the dependency is the project...
                    let PubGrubPackageInner::Package { name, .. } = &**dependency else {
                        return;
                    };
                    if name != project {
                        return;
                    }

                    // Recursively collapse the other side of the tree
                    drop_root_dependency_on_project(other, project);

                    // Then, replace this node with the other tree
                    *tree = other.clone();
                }
                // If not, just recurse
                _ => {
                    drop_root_dependency_on_project(Arc::make_mut(&mut derived.cause1), project);
                    drop_root_dependency_on_project(Arc::make_mut(&mut derived.cause2), project);
                }
            }
        }
    }
}

/// A version range that may include local version sentinels (`+[max]`).
#[derive(Debug)]
pub struct SentinelRange<'range>(&'range Range<Version>);

impl<'range> From<&'range Range<Version>> for SentinelRange<'range> {
    fn from(range: &'range Range<Version>) -> Self {
        Self(range)
    }
}

impl<'range> SentinelRange<'range> {
    /// Returns `true` if the range appears to be, e.g., `>1.0.0, <1.0.0+[max]`.
    pub fn is_sentinel(&self) -> bool {
        self.0.iter().all(|(lower, upper)| {
            let (Bound::Excluded(lower), Bound::Excluded(upper)) = (lower, upper) else {
                return false;
            };
            if lower.local() == LocalVersionSlice::Max {
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

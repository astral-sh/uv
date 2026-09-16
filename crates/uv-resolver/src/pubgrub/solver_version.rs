use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display};
use std::sync::Arc;

use pubgrub::{DerivationTree, Derived, External, SetRelation, Term, VersionSet};
use rustc_hash::{FxHashMap, FxHashSet};
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::error::ErrorTree;
use crate::pubgrub::{PubGrubPackage, Range};
use crate::resolver::UnavailableReason;

/// The identity of a direct resource, interned by the resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SourceId(pub(crate) usize);

/// The identity of an explicitly pinned registry, interned by the resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct IndexId(pub(crate) usize);

/// The origin whose metadata and artifacts belong to a solver candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum SolverSource {
    Registry,
    Index(IndexId),
    Url(SourceId),
}

impl SolverSource {
    pub(crate) fn is_registry(self) -> bool {
        match self {
            Self::Registry | Self::Index(_) => true,
            Self::Url(_) => false,
        }
    }
}

/// A source and PEP 440 version chosen together by PubGrub.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SolverVersion {
    pub(crate) source: SolverSource,
    pub(crate) version: Version,
}

impl SolverVersion {
    pub(crate) fn new(source: SolverSource, version: Version) -> Self {
        Self { source, version }
    }

    pub(crate) fn registry(version: Version) -> Self {
        Self::new(SolverSource::Registry, version)
    }
}

impl Display for SolverVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.version.fmt(f)
    }
}

/// A set of candidates that can describe URLs before their identity is known.
///
/// The default direct range applies to every URL, including resources not yet discovered. The
/// explicit entries are exceptions to that range, making complementation independent of discovery.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CandidateSet {
    registry: Range<Version>,
    indexed: Range<Version>,
    indexes: BTreeMap<IndexId, Range<Version>>,
    direct: Range<Version>,
    urls: BTreeMap<SourceId, Range<Version>>,
}

impl CandidateSet {
    pub(crate) fn all(versions: Range<Version>) -> Self {
        Self {
            registry: versions.clone(),
            indexed: versions.clone(),
            indexes: BTreeMap::new(),
            direct: versions,
            urls: BTreeMap::new(),
        }
    }

    pub(crate) fn source(source: SolverSource, versions: Range<Version>) -> Self {
        match source {
            SolverSource::Registry => Self {
                registry: versions,
                indexed: Range::empty(),
                indexes: BTreeMap::new(),
                direct: Range::empty(),
                urls: BTreeMap::new(),
            },
            SolverSource::Index(index) => Self {
                registry: Range::empty(),
                indexed: Range::empty(),
                indexes: if versions == Range::empty() {
                    BTreeMap::new()
                } else {
                    BTreeMap::from([(index, versions)])
                },
                direct: Range::empty(),
                urls: BTreeMap::new(),
            },
            SolverSource::Url(source) => Self {
                registry: Range::empty(),
                indexed: Range::empty(),
                indexes: BTreeMap::new(),
                direct: Range::empty(),
                urls: if versions == Range::empty() {
                    BTreeMap::new()
                } else {
                    BTreeMap::from([(source, versions)])
                },
            },
        }
    }

    /// Version requirements without a possible first-party URL still accept any selected registry.
    pub(crate) fn registries(versions: Range<Version>) -> Self {
        Self {
            registry: versions.clone(),
            indexed: versions,
            indexes: BTreeMap::new(),
            direct: Range::empty(),
            urls: BTreeMap::new(),
        }
    }

    /// Require an independently authorized URL while its concrete Git reference is being compared.
    pub(crate) fn urls(versions: Range<Version>) -> Self {
        Self {
            registry: Range::empty(),
            indexed: Range::empty(),
            indexes: BTreeMap::new(),
            direct: versions,
            urls: BTreeMap::new(),
        }
    }

    /// An explicit index constrains registry selection; an independently declared URL can take precedence.
    pub(crate) fn index_or_url(index: IndexId, versions: Range<Version>) -> Self {
        let mut candidates = Self::source(SolverSource::Index(index), versions.clone());
        candidates.direct = versions;
        candidates
    }

    pub(crate) fn for_source(&self, source: SolverSource) -> &Range<Version> {
        match source {
            SolverSource::Registry => &self.registry,
            SolverSource::Index(index) => self.indexes.get(&index).unwrap_or(&self.indexed),
            SolverSource::Url(source) => self.urls.get(&source).unwrap_or(&self.direct),
        }
    }

    /// Select a concrete index constrained by a dependency edge; never speculate on an unseen index.
    pub(crate) fn index(&self) -> Option<IndexId> {
        self.indexes.iter().find_map(|(index, versions)| {
            (*versions != Range::empty() && self.indexed == Range::empty()).then_some(*index)
        })
    }

    pub(crate) fn has_indexes(&self) -> bool {
        self.indexed != Range::empty()
            || self.indexes.values().any(|range| *range != Range::empty())
    }

    pub(crate) fn has_urls(&self) -> bool {
        self.direct != Range::empty() || self.urls.values().any(|range| *range != Range::empty())
    }

    pub(crate) fn allows_unseen_url(&self) -> bool {
        self.direct != Range::empty()
    }

    /// Return the direct identities when this set excludes the registry and all other URLs.
    pub(crate) fn only_urls(&self) -> impl Iterator<Item = SourceId> + '_ {
        self.urls.iter().filter_map(|(source, versions)| {
            (self.registry == Range::empty()
                && self.indexed == Range::empty()
                && self.indexes.values().all(|range| *range == Range::empty())
                && self.direct == Range::empty()
                && *versions != Range::empty())
            .then_some(*source)
        })
    }

    /// Return the explicit registries when the normal search and other indexes are excluded.
    pub(crate) fn only_indexes(&self) -> impl Iterator<Item = IndexId> + '_ {
        self.indexes.iter().filter_map(|(index, versions)| {
            (self.registry == Range::empty()
                && self.indexed == Range::empty()
                && *versions != Range::empty())
            .then_some(*index)
        })
    }

    /// Remove source identity for the existing PEP 440 diagnostic formatter.
    pub(crate) fn project(&self) -> Range<Version> {
        self.indexes.values().chain(self.urls.values()).fold(
            self.registry.union(&self.indexed).union(&self.direct),
            |range, versions| range.union(versions),
        )
    }

    /// The concrete source required by a declaration or supplying a candidate's metadata. A
    /// possible, unassigned URL can coexist with a named index without becoming a reporting source.
    fn report_source(&self) -> Option<SolverSource> {
        if self.registry != Range::empty() {
            return None;
        }
        let indexes = self.indexes.iter().filter_map(|(index, versions)| {
            (self.indexed == Range::empty() && *versions != Range::empty())
                .then_some(SolverSource::Index(*index))
        });
        let urls = self.urls.iter().filter_map(|(source, versions)| {
            (self.direct == Range::empty() && *versions != Range::empty())
                .then_some(SolverSource::Url(*source))
        });
        let mut sources = indexes.chain(urls);
        let source = sources.next()?;
        sources.next().is_none().then_some(source)
    }

    fn combine(
        &self,
        other: &Self,
        operation: impl Fn(&Range<Version>, &Range<Version>) -> Range<Version>,
    ) -> Self {
        let registry = operation(&self.registry, &other.registry);
        let indexed = operation(&self.indexed, &other.indexed);
        let direct = operation(&self.direct, &other.direct);
        let index_sources: BTreeSet<_> = self
            .indexes
            .keys()
            .chain(other.indexes.keys())
            .copied()
            .collect();
        let indexes = index_sources
            .into_iter()
            .filter_map(|index| {
                let versions = operation(
                    self.for_source(SolverSource::Index(index)),
                    other.for_source(SolverSource::Index(index)),
                );
                (versions != indexed).then_some((index, versions))
            })
            .collect();
        let sources: BTreeSet<_> = self.urls.keys().chain(other.urls.keys()).copied().collect();
        let urls = sources
            .into_iter()
            .filter_map(|source| {
                let versions = operation(
                    self.for_source(SolverSource::Url(source)),
                    other.for_source(SolverSource::Url(source)),
                );
                (versions != direct).then_some((source, versions))
            })
            .collect();
        Self {
            registry,
            indexed,
            indexes,
            direct,
            urls,
        }
    }

    /// Compare every source that can distinguish the sets without materializing a candidate set.
    /// The two default ranges also represent all indexes and URLs that are not yet known.
    #[inline]
    fn all_sources_match(
        &self,
        other: &Self,
        mut matches: impl FnMut(&Range<Version>, &Range<Version>) -> bool,
    ) -> bool {
        matches(&self.registry, &other.registry)
            && matches(&self.indexed, &other.indexed)
            && matches(&self.direct, &other.direct)
            && self.indexes.iter().all(|(index, versions)| {
                matches(versions, other.for_source(SolverSource::Index(*index)))
            })
            && other.indexes.iter().all(|(index, versions)| {
                self.indexes.contains_key(index) || matches(&self.indexed, versions)
            })
            && self.urls.iter().all(|(source, versions)| {
                matches(versions, other.for_source(SolverSource::Url(*source)))
            })
            && other.urls.iter().all(|(source, versions)| {
                self.urls.contains_key(source) || matches(&self.direct, versions)
            })
    }
}

impl VersionSet for CandidateSet {
    type V = SolverVersion;

    fn empty() -> Self {
        Self::all(Range::empty())
    }

    fn full() -> Self {
        Self::all(Range::full())
    }

    fn singleton(candidate: Self::V) -> Self {
        Self::source(candidate.source, Range::singleton(candidate.version))
    }

    fn complement(&self) -> Self {
        Self {
            registry: self.registry.complement(),
            indexed: self.indexed.complement(),
            indexes: self
                .indexes
                .iter()
                .map(|(index, versions)| (*index, versions.complement()))
                .collect(),
            direct: self.direct.complement(),
            urls: self
                .urls
                .iter()
                .map(|(source, versions)| (*source, versions.complement()))
                .collect(),
        }
    }

    fn intersection(&self, other: &Self) -> Self {
        self.combine(other, Range::intersection)
    }

    fn union(&self, other: &Self) -> Self {
        self.combine(other, Range::union)
    }

    fn difference(&self, other: &Self) -> Self {
        self.combine(other, Range::difference)
    }

    fn contains(&self, candidate: &Self::V) -> bool {
        self.for_source(candidate.source)
            .contains(&candidate.version)
    }

    #[inline]
    fn is_disjoint(&self, other: &Self) -> bool {
        self.all_sources_match(other, VersionSet::is_disjoint)
    }

    #[inline]
    fn subset_of(&self, other: &Self) -> bool {
        self.all_sources_match(other, VersionSet::subset_of)
    }

    #[inline]
    fn relation(&self, other: &Self) -> SetRelation {
        let mut subset = true;
        let mut disjoint = true;
        self.all_sources_match(other, |versions, other_versions| {
            // An empty component is both a subset and disjoint; it cannot determine the result.
            if *versions != Range::empty() {
                match versions.relation(other_versions) {
                    SetRelation::Subset => disjoint = false,
                    SetRelation::Disjoint => subset = false,
                    SetRelation::Overlapping => {
                        subset = false;
                        disjoint = false;
                    }
                }
            }
            subset || disjoint
        });
        if subset {
            SetRelation::Subset
        } else if disjoint {
            SetRelation::Disjoint
        } else {
            SetRelation::Overlapping
        }
    }
}

impl Display for CandidateSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.project().fmt(f)
    }
}

type SolverTree = DerivationTree<PubGrubPackage, CandidateSet, UnavailableReason>;
type SolverDerived = Derived<PubGrubPackage, CandidateSet, UnavailableReason>;

/// Find the direct resources and named indexes involved in a failed proof. PubGrub can backtrack
/// before returning the proof, so the parents declaring those sources may no longer be selected.
pub(crate) fn report_sources(error: &SolverTree) -> FxHashMap<PackageName, SolverSource> {
    let mut sources = FxHashMap::default();
    let mut record = |package: &PubGrubPackage, candidates: &CandidateSet| {
        if let Some(name) = package.name_no_root()
            && let Some(source) = candidates.report_source()
        {
            let previous = sources.entry(name.clone()).or_insert(source);
            if let SolverSource::Index(_) = *previous
                && let SolverSource::Url(_) = source
            {
                *previous = source;
            }
        }
    };
    let mut pending = vec![error];
    let mut seen = FxHashSet::default();
    while let Some(tree) = pending.pop() {
        if !seen.insert(std::ptr::from_ref(tree)) {
            continue;
        }
        match tree {
            SolverTree::External(External::FromDependencyOf(
                package,
                versions,
                dependency,
                requirements,
            )) => {
                record(package, versions);
                record(dependency, requirements);
            }
            SolverTree::External(External::Custom(package, versions, _)) => {
                record(package, versions);
            }
            SolverTree::External(External::NotRoot(..) | External::NoVersions(..)) => {}
            SolverTree::Derived(derived) => {
                pending.push(&derived.cause2);
                pending.push(&derived.cause1);
            }
        }
    }
    sources
}

/// Convert a shared source-aware derivation to the PEP 440 report without recursive traversal.
///
/// A missing-version statement about another source does not establish that the version is missing
/// from the source used in this fork. Remove those statements before dropping the source identity.
pub(crate) fn project_error(
    error: SolverTree,
    source: impl Fn(&PubGrubPackage) -> SolverSource,
) -> ErrorTree {
    let projected = project_tree(&error, |package| Some(source(package)))
        .or_else(|| project_tree(&error, |_| None))
        .expect("the root derivation was projected");

    let mut pending = vec![Arc::new(error)];
    while let Some(tree) = pending.pop() {
        if let Ok(SolverTree::Derived(derived)) = Arc::try_unwrap(tree) {
            pending.push(derived.cause1);
            pending.push(derived.cause2);
        }
    }
    Arc::try_unwrap(projected).expect("the projected root is not shared")
}

/// Project a proof onto the selected source, or onto all sources if a fork has no selected proof.
fn project_tree(
    error: &SolverTree,
    source: impl Fn(&PubGrubPackage) -> Option<SolverSource>,
) -> Option<Arc<ErrorTree>> {
    enum Frame<'a> {
        Tree(&'a SolverTree),
        Derived(&'a SolverTree, &'a SolverDerived),
    }

    let project = |package: &PubGrubPackage, versions: &CandidateSet| {
        source(package).map_or_else(
            || versions.project(),
            |source| versions.for_source(source).clone(),
        )
    };
    let mut tasks = vec![Frame::Tree(error)];
    let mut results = FxHashMap::<*const SolverTree, Option<Arc<ErrorTree>>>::default();
    while let Some(task) = tasks.pop() {
        match task {
            Frame::Tree(tree) => {
                if results.contains_key(&std::ptr::from_ref(tree)) {
                    continue;
                }
                match tree {
                    SolverTree::External(external) => {
                        let external = match external {
                            External::NotRoot(package, version) => {
                                Some(External::NotRoot(package.clone(), version.version.clone()))
                            }
                            External::NoVersions(package, versions) => {
                                let versions = project(package, versions);
                                (versions != Range::empty())
                                    .then(|| External::NoVersions(package.clone(), versions))
                            }
                            External::FromDependencyOf(
                                package,
                                versions,
                                dependency,
                                requirements,
                            ) => {
                                let versions = project(package, versions);
                                (versions != Range::empty()).then(|| {
                                    External::FromDependencyOf(
                                        package.clone(),
                                        versions,
                                        dependency.clone(),
                                        project(dependency, requirements),
                                    )
                                })
                            }
                            External::Custom(package, versions, reason) => {
                                let versions = project(package, versions);
                                (versions != Range::empty()).then(|| {
                                    External::Custom(package.clone(), versions, reason.clone())
                                })
                            }
                        };
                        results.insert(
                            std::ptr::from_ref(tree),
                            external.map(|external| Arc::new(ErrorTree::External(external))),
                        );
                    }
                    SolverTree::Derived(derived) => {
                        tasks.push(Frame::Derived(tree, derived));
                        tasks.push(Frame::Tree(&derived.cause2));
                        tasks.push(Frame::Tree(&derived.cause1));
                    }
                }
            }
            Frame::Derived(tree, derived) => {
                let projected = match (
                    results[&Arc::as_ptr(&derived.cause1)].clone(),
                    results[&Arc::as_ptr(&derived.cause2)].clone(),
                ) {
                    (Some(cause1), Some(cause2)) => {
                        let terms = derived
                            .terms
                            .iter()
                            .map(|(package, term)| {
                                let term = match term {
                                    Term::Positive(versions) => {
                                        Term::Positive(project(package, versions))
                                    }
                                    Term::Negative(versions) => {
                                        Term::Negative(project(package, versions))
                                    }
                                };
                                (package.clone(), term)
                            })
                            .collect();
                        Some(Arc::new(ErrorTree::Derived(Derived {
                            terms,
                            shared_id: derived.shared_id,
                            cause1,
                            cause2,
                        })))
                    }
                    (Some(cause), None) | (None, Some(cause)) => Some(cause),
                    (None, None) => None,
                };
                results.insert(std::ptr::from_ref(tree), projected);
            }
        }
    }

    results.remove(&std::ptr::from_ref(error)).flatten()
}

#[cfg(test)]
mod tests {
    use pubgrub::{SetRelation, VersionSet};
    use uv_pep440::Version;

    use super::{CandidateSet, IndexId, SolverSource, SolverVersion, SourceId};
    use crate::pubgrub::Range;

    #[test]
    fn undiscovered_sources_participate_in_set_operations() {
        let one = Version::new([1]);
        let two = Version::new([2]);
        let known = SolverSource::Url(SourceId(0));
        let undiscovered = SolverSource::Url(SourceId(1));
        let known_index = SolverSource::Index(IndexId(0));
        let undiscovered_index = SolverSource::Index(IndexId(1));
        let all_one = CandidateSet::all(Range::singleton(one.clone()));
        let all_two = CandidateSet::all(Range::singleton(two.clone()));
        let registry_one = CandidateSet::singleton(SolverVersion::registry(one.clone()));
        let known_one = CandidateSet::singleton(SolverVersion::new(known, one.clone()));
        let index_one = CandidateSet::singleton(SolverVersion::new(known_index, one.clone()));
        let index_or_url = CandidateSet::index_or_url(IndexId(0), Range::singleton(one.clone()));
        let registries = CandidateSet::registries(Range::singleton(one.clone()));

        assert!(all_one.contains(&SolverVersion::new(undiscovered, one.clone())));
        assert!(all_one.contains(&SolverVersion::new(undiscovered_index, one.clone())));
        assert!(registries.contains(&SolverVersion::new(undiscovered_index, one.clone())));
        assert!(!registries.contains(&SolverVersion::new(undiscovered, one.clone())));
        assert!(index_or_url.contains(&SolverVersion::new(undiscovered, one.clone())));
        assert!(!index_or_url.contains(&SolverVersion::new(undiscovered_index, one.clone())));
        assert_eq!(index_or_url.intersection(&registries), index_one);
        assert_eq!(
            index_one.union(&index_one.complement()),
            CandidateSet::full()
        );
        assert!(
            index_one
                .complement()
                .contains(&SolverVersion::new(undiscovered_index, one.clone()))
        );
        assert_eq!(all_one.intersection(&all_two), CandidateSet::empty());
        assert!(!known_one.contains(&SolverVersion::new(undiscovered, one.clone())));
        assert!(
            known_one
                .complement()
                .contains(&SolverVersion::new(undiscovered, one))
        );
        assert!(
            registry_one
                .complement()
                .contains(&SolverVersion::new(known, two))
        );
        assert_eq!(
            known_one.union(&known_one.complement()),
            CandidateSet::full()
        );
        assert_eq!(registry_one.difference(&known_one), registry_one);
    }

    #[test]
    fn source_relationships_match_materialized_sets() {
        let one = Range::singleton(Version::new([1]));
        let two = Range::singleton(Version::new([2]));
        let lower = Range::strictly_lower_than(Version::new([2]));
        let higher = Range::strictly_higher_than(Version::new([1]));
        let index = SolverSource::Index(IndexId(0));
        let second_index = SolverSource::Index(IndexId(1));
        let url = SolverSource::Url(SourceId(0));
        let second_url = SolverSource::Url(SourceId(1));
        let mut sets = vec![
            CandidateSet::empty(),
            CandidateSet::full(),
            CandidateSet::all(one.clone()),
            CandidateSet::all(lower.clone()),
            CandidateSet::registries(higher.clone()),
            CandidateSet::urls(two.clone()),
            CandidateSet::source(SolverSource::Registry, one.clone()),
            CandidateSet::source(index, one.clone()),
            CandidateSet::source(second_index, two.clone()),
            CandidateSet::source(url, lower.clone()),
            CandidateSet::source(second_url, higher.clone()),
            CandidateSet::index_or_url(IndexId(1), two.clone()),
            CandidateSet::source(index, lower.clone())
                .union(&CandidateSet::source(second_index, one.clone()))
                .union(&CandidateSet::source(url, higher.clone()))
                .union(&CandidateSet::source(second_url, two.clone())),
        ];
        sets.extend(sets.clone().iter().map(CandidateSet::complement));
        for left in &sets {
            for right in &sets {
                let intersection = left.intersection(right);
                let disjoint = intersection == CandidateSet::empty();
                let subset = intersection == *left;
                let relation = if subset {
                    SetRelation::Subset
                } else if disjoint {
                    SetRelation::Disjoint
                } else {
                    SetRelation::Overlapping
                };
                assert_eq!(left.is_disjoint(right), disjoint);
                assert_eq!(left.subset_of(right), subset);
                assert_eq!(left.relation(right), relation);
            }
        }
    }
}

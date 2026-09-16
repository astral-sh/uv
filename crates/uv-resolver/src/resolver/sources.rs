use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use pubgrub::{Id, State, VersionSet};
use rustc_hash::{FxHashMap, FxHashSet};
use uv_distribution_types::{DerivationChain, Requirement};
use uv_normalize::PackageName;
use uv_pep440::{MIN_VERSION, Version};
use uv_pypi_types::{ParsedDirectoryUrl, ParsedUrl, VerbatimParsedUrl};
use uv_types::{HashStrategy, HashStrategyError};

use crate::dependency_provider::UvDependencyProvider;
use crate::pubgrub::{CandidateSet, PubGrubPackage, SolverSource, SolverVersion, SourceId};
use crate::resolver::urls::Urls;

/// Optional source-search restrictions. They never introduce packages that the real root did not
/// require: a missing package satisfies every restriction.
#[derive(Clone, Default, Debug, PartialEq, Eq, Hash)]
pub(super) struct SourceAssumptions(BTreeMap<PubGrubPackage, CandidateSet>);

impl SourceAssumptions {
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&PubGrubPackage, &CandidateSet)> {
        self.0.iter()
    }

    pub(super) fn restrict(&mut self, package: PubGrubPackage, allowed: &CandidateSet) -> bool {
        let current = self
            .0
            .get(&package)
            .cloned()
            .unwrap_or_else(CandidateSet::full);
        let narrowed = current.intersection(allowed);
        if narrowed == current {
            return false;
        }
        self.0.insert(package, narrowed);
        true
    }

    /// Cover every alternative to a stalled assignment by retaining each earlier choice when the
    /// package is present and excluding the current choice. The optional prefixes can overlap, but
    /// cannot exclude an assignment that differs from the stalled branch.
    pub(super) fn alternatives(
        &self,
        choices: impl IntoIterator<Item = (PubGrubPackage, SolverVersion)>,
    ) -> Vec<Self> {
        let mut prefix = self.clone();
        let mut alternatives = Vec::new();
        for (package, candidate) in choices {
            let selected = CandidateSet::singleton(candidate);
            let mut alternative = prefix.clone();
            if alternative.restrict(package.clone(), &selected.complement()) {
                alternatives.push(alternative);
            }
            prefix.restrict(package, &selected);
        }
        alternatives
    }
}

/// Dependencies retained with the exact candidate that declared them.
#[derive(Clone, Default)]
pub(super) struct SourceDependencies {
    dependencies: FxHashMap<(Id<PubGrubPackage>, SolverVersion), Vec<SolvedDependency>>,
    order: FxHashMap<(Id<PubGrubPackage>, SolverVersion), usize>,
    chains: FxHashMap<(Id<PubGrubPackage>, SolverVersion), DerivationChain>,
}

#[derive(Clone)]
pub(super) struct SolvedDependency {
    pub(super) package: Id<PubGrubPackage>,
    pub(super) candidates: CandidateSet,
    pub(super) declaration: Option<UrlDeclaration>,
    pub(super) policy: Option<(Arc<Requirement>, bool)>,
}

#[derive(Clone)]
pub(super) struct UrlDeclaration {
    pub(super) source: SourceId,
    pub(super) url: VerbatimParsedUrl,
    pub(super) trusted: bool,
    pub(super) hash_requirement: Option<Arc<Requirement>>,
    pub(super) trusted_hashes: bool,
}

/// Outgoing URL and first-party candidate-policy possibilities from metadata the resolver has
/// already inspected, including inactive extras. This cannot authorize a source or candidate itself.
#[derive(Clone)]
pub(super) enum SourcePotential {
    Metadata {
        version: Version,
        dependencies: Arc<[Requirement]>,
        policies: Arc<[Requirement]>,
    },
    Unavailable,
}

impl SourceDependencies {
    pub(super) fn set_chain(
        &mut self,
        id: Id<PubGrubPackage>,
        candidate: SolverVersion,
        chain: DerivationChain,
    ) {
        self.chains.entry((id, candidate)).or_insert(chain);
    }

    pub(super) fn chain(
        &self,
        id: Id<PubGrubPackage>,
        candidate: &SolverVersion,
    ) -> Option<&DerivationChain> {
        self.chains.get(&(id, candidate.clone()))
    }

    pub(super) fn insert(
        &mut self,
        package: Id<PubGrubPackage>,
        candidate: SolverVersion,
        dependencies: Vec<SolvedDependency>,
    ) {
        let key = (package, candidate);
        let next = self.order.len();
        self.order.entry(key.clone()).or_insert(next);
        self.dependencies.insert(key, dependencies);
    }

    /// Find exact trusted declarations corresponding to a native dependency proof.
    pub(super) fn trusted_declarations(
        &self,
        state: &State<UvDependencyProvider>,
        parent: &PubGrubPackage,
        candidates: &CandidateSet,
        dependency: &PackageName,
        source: SourceId,
    ) -> Vec<(usize, Id<PubGrubPackage>, SolverVersion)> {
        self.dependencies
            .iter()
            .filter_map(|((id, candidate), dependencies)| {
                if state.package_store[*id] != *parent || !candidates.contains(candidate) {
                    return None;
                }
                dependencies
                    .iter()
                    .any(|entry| {
                        state.package_store[entry.package].name_no_root() == Some(dependency)
                            && entry.declaration.as_ref().is_some_and(|declaration| {
                                declaration.trusted && declaration.source == source
                            })
                    })
                    .then(|| {
                        (
                            self.order[&(*id, candidate.clone())],
                            *id,
                            candidate.clone(),
                        )
                    })
            })
            .collect()
    }

    /// Find the URLs authorized by paths from the real root under the current solver decisions.
    ///
    /// Registry packages can activate an extra on a grounded URL package, but only that URL
    /// package's metadata or explicit configuration can authorize a new direct resource.
    pub(super) fn grounding(&self, state: &State<UvDependencyProvider>) -> Grounding {
        let mut selected: FxHashMap<_, _> = state.partial_solution.extract_solution().collect();
        // The root is mandatory, including while PubGrub is processing its first incompatibilities.
        selected
            .entry(state.root_package)
            .or_insert_with(|| SolverVersion::registry(MIN_VERSION.clone()));

        let mut grounding = Grounding::default();
        grounding.reachable.insert(state.root_package);
        loop {
            let old = (grounding.reachable.len(), grounding.presentations.len());
            let mut pending = VecDeque::from([state.root_package]);
            let mut seen = FxHashSet::default();
            while let Some(package) = pending.pop_front() {
                if !seen.insert(package) {
                    continue;
                }
                let Some(candidate) = selected.get(&package) else {
                    continue;
                };
                if let SolverSource::Url(source) = candidate.source
                    && !state.package_store[package]
                        .name_no_root()
                        .is_some_and(|name| grounding.contains(name, source))
                {
                    continue;
                }
                let Some(dependencies) = self.dependencies.get(&(package, candidate.clone()))
                else {
                    continue;
                };
                for dependency in dependencies {
                    grounding.reachable.insert(dependency.package);
                    pending.push_back(dependency.package);
                    if let Some(declaration) = &dependency.declaration
                        && declaration.trusted
                        && let Some(name) = state.package_store[dependency.package].name_no_root()
                    {
                        grounding
                            .urls
                            .entry(name.clone())
                            .or_default()
                            .insert(declaration.source);
                        grounding
                            .presentations
                            .entry(declaration.source)
                            .and_modify(|url| merge_presentation(url, &declaration.url))
                            .or_insert_with(|| declaration.url.clone());
                    }
                }
            }
            if old == (grounding.reachable.len(), grounding.presentations.len()) {
                break;
            }
        }

        for package in &grounding.reachable {
            let Some(candidate) = selected.get(package) else {
                continue;
            };
            if let SolverSource::Url(source) = candidate.source
                && !state.package_store[*package]
                    .name_no_root()
                    .is_some_and(|name| grounding.contains(name, source))
            {
                continue;
            }
            if let Some(dependencies) = self.dependencies.get(&(*package, candidate.clone())) {
                for dependency in dependencies {
                    if let Some(policy) = &dependency.policy {
                        grounding.policies.push(policy.clone());
                    }
                    if let Some(declaration) = &dependency.declaration
                        && let Some(requirement) = &declaration.hash_requirement
                    {
                        if declaration.trusted_hashes {
                            grounding.hashes.trusted.push(requirement.clone());
                        } else {
                            grounding.hashes.metadata.push(requirement.clone());
                        }
                    }
                    if let Some(declaration) = &dependency.declaration
                        && !declaration.trusted
                        && state.package_store[dependency.package]
                            .name_no_root()
                            .is_some_and(|name| !grounding.contains(name, declaration.source))
                    {
                        grounding.untrusted.push((
                            *package,
                            candidate.clone(),
                            dependency.package,
                            declaration.source,
                        ));
                    }
                }
            }
        }
        grounding
            .untrusted
            .sort_by_key(|(parent, _, package, source)| {
                (
                    state.package_store[*parent].clone(),
                    state.package_store[*package].clone(),
                    *source,
                )
            });
        grounding
    }
}

/// Source authorities derived from currently selected, reachable candidates.
#[derive(Default)]
pub(super) struct Grounding {
    pub(super) reachable: FxHashSet<Id<PubGrubPackage>>,
    urls: FxHashMap<PackageName, BTreeSet<SourceId>>,
    presentations: FxHashMap<SourceId, VerbatimParsedUrl>,
    pub(super) hashes: ActiveHashes,
    pub(super) policies: Vec<(Arc<Requirement>, bool)>,
    pub(super) untrusted: Vec<(
        Id<PubGrubPackage>,
        SolverVersion,
        Id<PubGrubPackage>,
        SourceId,
    )>,
}

/// Hash declarations from selected paths, separating explicit inputs from distribution metadata.
#[derive(Default)]
pub(super) struct ActiveHashes {
    trusted: Vec<Arc<Requirement>>,
    metadata: Vec<Arc<Requirement>>,
}

impl ActiveHashes {
    pub(super) fn extend(&mut self, hashes: Self) {
        self.trusted.extend(hashes.trusted);
        self.metadata.extend(hashes.metadata);
    }

    pub(super) fn strategy(&self, base: &HashStrategy) -> Result<HashStrategy, HashStrategyError> {
        base.clone()
            .augment_with_requirements(self.trusted.iter().map(AsRef::as_ref))?
            .augment_with_metadata_requirements(self.metadata.iter().map(AsRef::as_ref))
    }
}

impl Grounding {
    pub(super) fn contains(&self, name: &PackageName, source: SourceId) -> bool {
        self.urls
            .get(name)
            .is_some_and(|sources| sources.contains(&source))
    }

    pub(super) fn sources_for(&self, name: &PackageName) -> impl Iterator<Item = SourceId> + '_ {
        self.urls.get(name).into_iter().flatten().copied()
    }

    pub(super) fn source(&self, name: &PackageName, candidates: &CandidateSet) -> Option<SourceId> {
        self.urls
            .get(name)?
            .iter()
            .find(|source| {
                *candidates.for_source(SolverSource::Url(**source))
                    != crate::pubgrub::Range::empty()
            })
            .copied()
    }

    pub(super) fn url(&self, source: SourceId, urls: &Urls) -> VerbatimParsedUrl {
        self.presentations
            .get(&source)
            .cloned()
            .unwrap_or_else(|| urls.get(source).as_ref().clone())
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&PackageName, &BTreeSet<SourceId>)> {
        self.urls.iter()
    }
}

/// Merge equivalent active declarations, preferring user path spellings and explicit editability.
fn merge_presentation(previous: &mut VerbatimParsedUrl, incoming: &VerbatimParsedUrl) {
    let incoming_preferred =
        !incoming.verbatim.force_relative() || previous.verbatim.force_relative();
    let previous_editable = previous.is_editable();
    if incoming_preferred {
        *previous = incoming.clone();
    }
    if (previous_editable || incoming.is_editable())
        && let ParsedUrl::Directory(ParsedDirectoryUrl { editable, .. }) = &mut previous.parsed_url
        && editable.is_none()
    {
        *editable = Some(true);
    }
}

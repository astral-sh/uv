use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use pubgrub::{Id, State, VersionSet};
use rustc_hash::{FxHashMap, FxHashSet};
use uv_distribution_types::{DerivationChain, Requirement};
use uv_git::GitResolver;
use uv_normalize::PackageName;
use uv_pep440::{MIN_VERSION, Version};
use uv_pep508::MarkerTree;
use uv_pypi_types::{ParsedDirectoryUrl, ParsedUrl, VerbatimParsedUrl};
use uv_types::{HashStrategy, HashStrategyError};

use crate::dependency_provider::UvDependencyProvider;
use crate::pubgrub::{
    CandidateSet, IndexId, PubGrubPackage, SolverSource, SolverVersion, SourceId,
};
use crate::python_requirement::PythonRequirement;
use crate::resolver::environment::ResolverEnvironment;
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
    has_urls: bool,
    has_indexes: bool,
    has_contextual_sources: bool,
}

#[derive(Clone)]
pub(super) struct SolvedDependency {
    pub(super) package: Id<PubGrubPackage>,
    pub(super) candidates: CandidateSet,
    pub(super) declaration: Option<UrlDeclaration>,
    pub(super) index: Option<IndexId>,
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

/// Two selected declarations that require different installation modes for the same directory.
pub(super) struct DirectoryConflict {
    pub(super) name: PackageName,
    pub(super) origins: [(Id<PubGrubPackage>, SolverVersion, ParsedUrl); 2],
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
    pub(super) fn has_urls(&self) -> bool {
        self.has_urls
    }

    pub(super) fn has_indexes(&self) -> bool {
        self.has_indexes
    }

    pub(super) fn has_contextual_sources(&self) -> bool {
        self.has_contextual_sources
    }

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
        contextual: bool,
    ) {
        self.has_urls |= dependencies
            .iter()
            .any(|dependency| dependency.declaration.is_some());
        self.has_indexes |= dependencies
            .iter()
            .any(|dependency| dependency.index.is_some());
        self.has_contextual_sources |= contextual
            && dependencies
                .iter()
                .any(|dependency| dependency.declaration.is_some() || dependency.index.is_some());
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

    /// Find the selected candidate edges that pinned a registry named in a native failure proof.
    pub(super) fn index_declarations(
        &self,
        state: &State<UvDependencyProvider>,
        parent: &PubGrubPackage,
        candidates: &CandidateSet,
        dependency: &PackageName,
        index: IndexId,
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
                            && entry.index == Some(index)
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

    /// Return currently rooted choices that lead to local declarations making this package direct.
    pub(super) fn lowest_support(
        &self,
        state: &State<UvDependencyProvider>,
        grounding: &Grounding,
        name: &PackageName,
    ) -> Vec<(PubGrubPackage, SolverVersion)> {
        let Some(parents) = grounding.lowest_parents.get(name) else {
            return Vec::new();
        };
        let selected: FxHashMap<_, _> = state.partial_solution.extract_solution().collect();
        let mut reverse = FxHashMap::<_, Vec<_>>::default();
        for ((parent, candidate), dependencies) in &self.dependencies {
            if !grounding.reachable.contains(parent) || selected.get(parent) != Some(candidate) {
                continue;
            }
            for dependency in dependencies {
                if grounding.reachable.contains(&dependency.package) {
                    reverse.entry(dependency.package).or_default().push(*parent);
                }
            }
        }
        let mut support = FxHashSet::default();
        let mut pending = parents.iter().copied().collect::<Vec<_>>();
        while let Some(package) = pending.pop() {
            if support.insert(package)
                && let Some(parents) = reverse.get(&package)
            {
                pending.extend(parents.iter().copied());
            }
        }
        state
            .partial_solution
            .extract_solution()
            .filter_map(|(id, candidate)| {
                let package = &state.package_store[id];
                (support.contains(&id)
                    && package
                        .name_no_root()
                        .is_some_and(|package_name| package_name != name))
                .then(|| (package.clone(), candidate))
            })
            .collect()
    }

    /// Find selected registry paths without repeatedly computing source authorities. Registry
    /// metadata cannot introduce first-party candidate policies, but dependency-path markers still
    /// determine which selected packages belong in the output and where root policies can apply.
    pub(super) fn registry_grounding(
        &self,
        state: &State<UvDependencyProvider>,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
    ) -> Grounding {
        let mut selected: FxHashMap<_, _> = state.partial_solution.extract_solution().collect();
        selected
            .entry(state.root_package)
            .or_insert_with(|| SolverVersion::registry(MIN_VERSION.clone()));
        let mut grounding = Grounding::default();
        grounding.reachable.insert(state.root_package);
        grounding.contexts.insert(
            state.root_package,
            env.fork_markers().map_or(MarkerTree::TRUE, |marker| {
                marker.and(python_requirement.to_marker_tree())
            }),
        );
        let mut pending = VecDeque::from([state.root_package]);
        while let Some(package) = pending.pop_front() {
            let Some(candidate) = selected.get(&package) else {
                continue;
            };
            let Some(dependencies) = self.dependencies.get(&(package, candidate.clone())) else {
                continue;
            };
            let context = grounding.contexts[&package];
            for dependency in dependencies {
                let context = context.and(in_environment(
                    state.package_store[dependency.package].marker(),
                    env,
                ));
                if context.is_false() {
                    continue;
                }
                let previous = grounding
                    .contexts
                    .entry(dependency.package)
                    .or_insert(MarkerTree::FALSE);
                let updated = previous.or(context);
                if updated != *previous {
                    *previous = updated;
                    grounding.reachable.insert(dependency.package);
                    pending.push_back(dependency.package);
                }
            }
        }
        grounding
    }

    /// Find the URLs authorized by paths from the real root under the current solver decisions.
    ///
    /// Registry packages can activate an extra on a grounded URL package, but only that URL
    /// package's metadata or explicit configuration can authorize a new direct resource.
    pub(super) fn grounding(
        &self,
        state: &State<UvDependencyProvider>,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
        urls: &Urls,
        git: &GitResolver,
    ) -> Grounding {
        let mut selected: FxHashMap<_, _> = state.partial_solution.extract_solution().collect();
        // The root is mandatory, including while PubGrub is processing its first incompatibilities.
        selected
            .entry(state.root_package)
            .or_insert_with(|| SolverVersion::registry(MIN_VERSION.clone()));

        let mut grounding = Grounding::default();
        grounding.reachable.insert(state.root_package);
        grounding.contexts.insert(
            state.root_package,
            env.fork_markers().map_or(MarkerTree::TRUE, |marker| {
                marker.and(python_requirement.to_marker_tree())
            }),
        );
        // Reachability and source authorization grow together. Outgoing edges from a concrete
        // candidate apply only where that exact source was independently authorized.
        loop {
            let mut changed = false;
            let mut pending = VecDeque::from([state.root_package]);
            let mut seen = FxHashSet::default();
            while let Some(package) = pending.pop_front() {
                if !seen.insert(package) {
                    continue;
                }
                let Some(candidate) = selected.get(&package) else {
                    continue;
                };
                let mut context = grounding.contexts[&package];
                if let Some(name) = state.package_store[package].name_no_root() {
                    context = context.and(grounding.candidate_marker(name, candidate.source));
                }
                if context.is_false() {
                    continue;
                }
                let Some(dependencies) = self.dependencies.get(&(package, candidate.clone()))
                else {
                    continue;
                };
                for dependency in dependencies {
                    let context = context.and(in_environment(
                        state.package_store[dependency.package].marker(),
                        env,
                    ));
                    if context.is_false() {
                        continue;
                    }
                    grounding.reachable.insert(dependency.package);
                    let previous = grounding
                        .contexts
                        .entry(dependency.package)
                        .or_insert(MarkerTree::FALSE);
                    let updated = previous.or(context);
                    if updated != *previous {
                        *previous = updated;
                        changed = true;
                    }
                    pending.push_back(dependency.package);
                    if let Some(name) = state.package_store[dependency.package].name_no_root() {
                        if let Some(declaration) = &dependency.declaration
                            && declaration.trusted
                        {
                            let previous = grounding
                                .urls
                                .entry(name.clone())
                                .or_default()
                                .entry(declaration.source)
                                .or_insert(MarkerTree::FALSE);
                            let updated = previous.or(context);
                            if updated != *previous {
                                *previous = updated;
                                changed = true;
                            }
                        }
                        if let Some(index) = dependency.index {
                            let previous = grounding
                                .indexes
                                .entry(name.clone())
                                .or_default()
                                .entry(index)
                                .or_insert(MarkerTree::FALSE);
                            let updated = previous.or(context);
                            if updated != *previous {
                                *previous = updated;
                                changed = true;
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }

        let mut directory_declarations = BTreeMap::<_, Vec<_>>::new();
        for package in &grounding.reachable {
            let Some(candidate) = selected.get(package) else {
                continue;
            };
            let mut context = grounding.contexts[package];
            if let Some(name) = state.package_store[*package].name_no_root() {
                context = context.and(grounding.candidate_marker(name, candidate.source));
            }
            if context.is_false() {
                continue;
            }
            if let Some(dependencies) = self.dependencies.get(&(*package, candidate.clone())) {
                for dependency in dependencies {
                    let edge_context = context.and(in_environment(
                        state.package_store[dependency.package].marker(),
                        env,
                    ));
                    if edge_context.is_false() {
                        continue;
                    }
                    if dependency.declaration.is_some() || dependency.index.is_some() {
                        grounding.source_contexts.insert(edge_context);
                    }
                    if let Some((requirement, lowest)) = &dependency.policy {
                        let marker = requirement.marker.and(context);
                        if !marker.is_false() {
                            let requirement = scope_requirement(requirement, marker);
                            if *lowest {
                                grounding
                                    .lowest_parents
                                    .entry(requirement.name.clone())
                                    .or_default()
                                    .insert(*package);
                            }
                            grounding.policies.push((requirement, *lowest));
                        }
                    }
                    if let Some(declaration) = &dependency.declaration {
                        if declaration.trusted {
                            if let ParsedUrl::Directory(directory) = &declaration.url.parsed_url
                                && let Some(editable) = directory.editable
                                && directory.r#virtual != Some(true)
                                && let Some(name) =
                                    state.package_store[dependency.package].name_no_root()
                            {
                                directory_declarations
                                    .entry((name.clone(), declaration.source))
                                    .or_default()
                                    .push((
                                        *package,
                                        candidate.clone(),
                                        declaration.url.parsed_url.clone(),
                                        editable,
                                        edge_context,
                                    ));
                            }
                            grounding
                                .presentations
                                .entry(declaration.source)
                                .and_modify(|url| merge_presentation(url, &declaration.url))
                                .or_insert_with(|| declaration.url.clone());
                        }
                        if let Some(requirement) = &declaration.hash_requirement {
                            let requirement = scope_requirement(
                                requirement,
                                requirement.marker.and(edge_context),
                            );
                            grounding.hash_declarations.push((
                                *package,
                                candidate.clone(),
                                requirement.clone(),
                                declaration.trusted_hashes,
                            ));
                            if declaration.trusted_hashes {
                                grounding.hashes.trusted.push(requirement);
                            } else {
                                grounding.hashes.metadata.push(requirement);
                            }
                        }
                        if !declaration.trusted
                            && let Some(name) =
                                state.package_store[dependency.package].name_no_root()
                        {
                            let authorized = urls
                                .lookup(name, &declaration.url, git)
                                .into_iter()
                                .fold(MarkerTree::FALSE, |marker, source| {
                                    marker
                                        .or(grounding
                                            .candidate_marker(name, SolverSource::Url(source)))
                                });
                            if !edge_context.is_disjoint(authorized.negate()) {
                                grounding.untrusted.push((
                                    *package,
                                    candidate.clone(),
                                    dependency.package,
                                    declaration.source,
                                ));
                                grounding
                                    .untrusted_urls
                                    .entry(name.clone())
                                    .or_default()
                                    .push(declaration.url.clone());
                            }
                        }
                    }
                }
            }
        }
        for ((name, source), mut declarations) in directory_declarations {
            declarations.sort_by(|a, b| {
                (&state.package_store[a.0], &a.1, &a.2).cmp(&(
                    &state.package_store[b.0],
                    &b.1,
                    &b.2,
                ))
            });
            let mut editable_origins = declarations
                .iter()
                .filter(|(_, _, _, editable, _)| *editable)
                .map(|(parent, candidate, _, _, _)| (*parent, candidate.clone()))
                .collect::<Vec<_>>();
            editable_origins.dedup();
            if !editable_origins.is_empty() {
                grounding
                    .directory_editable_origins
                    .insert(source, editable_origins);
            }
            let pair = declarations.iter().enumerate().find_map(|(index, a)| {
                declarations[index + 1..]
                    .iter()
                    .find(|b| a.3 != b.3 && !a.4.is_disjoint(b.4))
                    .map(|b| (a, b))
            });
            if let Some((a, b)) = pair {
                grounding.directory_conflicts.push(DirectoryConflict {
                    name,
                    origins: [
                        (a.0, a.1.clone(), a.2.clone()),
                        (b.0, b.1.clone(), b.2.clone()),
                    ],
                });
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

/// Keep the symbolic marker in a universal resolution, or evaluate it for a specific environment.
fn in_environment(marker: MarkerTree, env: &ResolverEnvironment) -> MarkerTree {
    env.marker_environment().map_or(marker, |environment| {
        if marker.evaluate(environment, &[]) {
            MarkerTree::TRUE
        } else {
            MarkerTree::FALSE
        }
    })
}

/// Source authorities derived from currently selected, reachable candidates.
#[derive(Default)]
pub(super) struct Grounding {
    pub(super) reachable: FxHashSet<Id<PubGrubPackage>>,
    pub(super) contexts: FxHashMap<Id<PubGrubPackage>, MarkerTree>,
    lowest_parents: FxHashMap<PackageName, FxHashSet<Id<PubGrubPackage>>>,
    urls: FxHashMap<PackageName, BTreeMap<SourceId, MarkerTree>>,
    presentations: FxHashMap<SourceId, VerbatimParsedUrl>,
    pub(super) indexes: FxHashMap<PackageName, BTreeMap<IndexId, MarkerTree>>,
    source_contexts: BTreeSet<MarkerTree>,
    pub(super) directory_conflicts: Vec<DirectoryConflict>,
    directory_editable_origins: FxHashMap<SourceId, Vec<(Id<PubGrubPackage>, SolverVersion)>>,
    pub(super) hashes: ActiveHashes,
    hash_declarations: Vec<(Id<PubGrubPackage>, SolverVersion, Arc<Requirement>, bool)>,
    pub(super) policies: Vec<(Arc<Requirement>, bool)>,
    pub(super) untrusted: Vec<(
        Id<PubGrubPackage>,
        SolverVersion,
        Id<PubGrubPackage>,
        SourceId,
    )>,
    pub(super) untrusted_urls: FxHashMap<PackageName, Vec<VerbatimParsedUrl>>,
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
    /// Environments where this exact source was independently selected by a reachable declaration.
    fn candidate_marker(&self, name: &PackageName, source: SolverSource) -> MarkerTree {
        match source {
            SolverSource::Registry => MarkerTree::TRUE,
            SolverSource::Index(index) => self
                .indexes
                .get(name)
                .and_then(|indexes| indexes.get(&index))
                .copied()
                .unwrap_or(MarkerTree::FALSE),
            SolverSource::Url(source) => self
                .urls
                .get(name)
                .and_then(|sources| sources.get(&source))
                .copied()
                .unwrap_or(MarkerTree::FALSE),
        }
    }

    /// Return a declaration that is active in only part of the current supported environment.
    pub(super) fn conditional_source(
        &self,
        env: &ResolverEnvironment,
        python_requirement: &PythonRequirement,
    ) -> Option<MarkerTree> {
        let fork = env.fork_markers()?.and(python_requirement.to_marker_tree());
        self.source_contexts
            .iter()
            .copied()
            .find(|marker| !fork.is_disjoint(*marker) && !fork.is_disjoint(marker.negate()))
            .map(|marker| python_requirement.simplify_markers(marker))
    }

    /// Reduce a failed hash policy to selected candidate authors sufficient to reproduce it.
    /// Declaration order and the distinction between trusted inputs and metadata are retained.
    pub(super) fn hash_error_origins(
        &self,
        state: &State<UvDependencyProvider>,
        base: &HashStrategy,
        error: &HashStrategyError,
    ) -> Vec<(Id<PubGrubPackage>, SolverVersion)> {
        let mut origins: BTreeMap<_, _> = self
            .hash_declarations
            .iter()
            .map(|(id, candidate, _, _)| {
                ((state.package_store[*id].clone(), candidate.clone()), *id)
            })
            .collect();
        let error = error.to_string();
        for (origin, id) in origins.clone() {
            origins.remove(&origin);
            let mut hashes = ActiveHashes::default();
            for (id, candidate, requirement, trusted) in &self.hash_declarations {
                if !origins.contains_key(&(state.package_store[*id].clone(), candidate.clone())) {
                    continue;
                }
                if *trusted {
                    hashes.trusted.push(requirement.clone());
                } else {
                    hashes.metadata.push(requirement.clone());
                }
            }
            if !hashes
                .strategy(base)
                .is_err_and(|remaining| remaining.to_string() == error)
            {
                origins.insert(origin, id);
            }
        }
        origins
            .into_iter()
            .map(|((_, candidate), id)| (id, candidate))
            .collect()
    }

    pub(super) fn contains(&self, name: &PackageName, source: SourceId) -> bool {
        self.urls
            .get(name)
            .is_some_and(|sources| sources.contains_key(&source))
    }

    pub(super) fn sources_for(&self, name: &PackageName) -> impl Iterator<Item = SourceId> + '_ {
        self.urls
            .get(name)
            .into_iter()
            .flat_map(|sources| sources.keys().copied())
    }

    pub(super) fn source(&self, name: &PackageName, candidates: &CandidateSet) -> Option<SourceId> {
        self.urls
            .get(name)?
            .keys()
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

    /// Use editable metadata early during a retry, without making the expectation a declaration.
    pub(super) fn metadata_url(
        &self,
        source: SourceId,
        urls: &Urls,
        preferred_editable: &BTreeSet<SourceId>,
    ) -> VerbatimParsedUrl {
        let mut url = self.url(source, urls);
        if preferred_editable.contains(&source)
            && let ParsedUrl::Directory(directory) = &mut url.parsed_url
        {
            directory.editable = Some(true);
        }
        url
    }

    /// Selected declarations that require this source to be editable.
    pub(super) fn directory_editable_origins(
        &self,
        source: SourceId,
    ) -> impl Iterator<Item = &(Id<PubGrubPackage>, SolverVersion)> {
        self.directory_editable_origins
            .get(&source)
            .into_iter()
            .flatten()
    }

    pub(super) fn iter(
        &self,
    ) -> impl Iterator<Item = (&PackageName, &BTreeMap<SourceId, MarkerTree>)> {
        self.urls.iter()
    }
}

/// Reuse an authored requirement when its selected path leaves its marker unchanged.
fn scope_requirement(requirement: &Arc<Requirement>, marker: MarkerTree) -> Arc<Requirement> {
    if marker == requirement.marker {
        requirement.clone()
    } else {
        let mut scoped = requirement.as_ref().clone();
        scoped.marker = marker;
        Arc::new(scoped)
    }
}

/// Merge active declarations of the same resource. Unspecified editability can adopt either
/// explicit mode, while a virtual declaration must not suppress an independently required install.
fn merge_presentation(previous: &mut VerbatimParsedUrl, incoming: &VerbatimParsedUrl) {
    if let (ParsedUrl::Directory(old), ParsedUrl::Directory(new)) =
        (&previous.parsed_url, &incoming.parsed_url)
    {
        let editable = if old.r#virtual == Some(true) && new.r#virtual == Some(true) {
            Some(false)
        } else {
            let old_editable = if old.r#virtual == Some(true) {
                None
            } else {
                old.editable
            };
            let new_editable = if new.r#virtual == Some(true) {
                None
            } else {
                new.editable
            };
            match (old_editable, new_editable) {
                (Some(a), Some(b)) => Some(a || b),
                (a, b) => a.or(b),
            }
        };
        let r#virtual = if old.r#virtual.is_none() && new.r#virtual.is_none() {
            None
        } else {
            Some(old.r#virtual == Some(true) && new.r#virtual == Some(true))
        };
        let incoming_preferred = (
            incoming.verbatim.force_relative(),
            &incoming.verbatim,
            incoming.verbatim.given().is_none(),
            incoming.verbatim.given(),
        ) < (
            previous.verbatim.force_relative(),
            &previous.verbatim,
            previous.verbatim.given().is_none(),
            previous.verbatim.given(),
        );
        if incoming_preferred {
            *previous = incoming.clone();
        }
        if let ParsedUrl::Directory(ParsedDirectoryUrl {
            editable: selected_editable,
            r#virtual: selected_virtual,
            ..
        }) = &mut previous.parsed_url
        {
            *selected_editable = editable;
            *selected_virtual = r#virtual;
        }
        return;
    }
    let incoming_preferred =
        !incoming.verbatim.force_relative() || previous.verbatim.force_relative();
    if incoming_preferred {
        *previous = incoming.clone();
    }
}

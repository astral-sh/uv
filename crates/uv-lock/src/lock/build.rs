//! Independent build resolutions share artifacts with the runtime graph. Only nodes whose
//! outgoing edges differ need a universe to distinguish them from another copy of the same ID.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

use futures::{StreamExt, TryStreamExt, stream};
use itertools::Itertools;
use petgraph::Graph;
use rustc_hash::{FxHashMap, FxHashSet};
use uv_client::RegistryClient;
use uv_configuration::BuildOptions;
use uv_distribution_types::{
    ArchiveHashPolicy, Edge, ExtraBuildRequirement, FirstParty, Name, Node, Requirement,
    Resolution, ResolvedDist, SourceDist, satisfies_require_hashes,
};
use uv_normalize::PackageName;
use uv_pep508::MarkerTree;
use uv_platform_tags::Tags;
use uv_pypi_types::ResolverMarkerEnvironment;
use uv_resolver_types::ResolverOutput;

use super::{
    Dependency, HashedDist, Lock, LockError, LockErrorKind, Package, PackageId, ResolverManifest,
    SimplifiedMarkerTree, Source, TagPolicy,
};

const RUNTIME: &str = "@runtime";

/// A package and the requirements it contributes to a build environment.
#[derive(Debug)]
pub struct BuildUniverseOwner {
    pub id: PackageId,
    pub requirements: Vec<Requirement>,
}

/// An independent resolution shared by packages with compatible build requirements.
#[derive(Debug)]
pub struct BuildUniverse {
    pub backend: String,
    pub owners: Vec<BuildUniverseOwner>,
    pub lock: Lock,
}

impl BuildUniverse {
    /// When a later build expands a package's supported Python range, its replacement
    /// resolution can make the old backend (and that backend's own build) unreachable.
    fn reachable(universes: Vec<Self>, runtime: &[Package]) -> Vec<Self> {
        let owners: FxHashMap<_, _> = universes
            .iter()
            .enumerate()
            .flat_map(|(index, universe)| {
                universe.owners.iter().map(move |owner| (&owner.id, index))
            })
            .collect();
        let mut pending: Vec<_> = runtime.iter().map(Package::id).collect();
        let mut reachable = FxHashSet::default();
        while let Some(id) = pending.pop() {
            if let Some(&index) = owners.get(id)
                && reachable.insert(index)
            {
                pending.extend(universes[index].lock.packages.iter().map(Package::id));
            }
        }
        universes
            .into_iter()
            .enumerate()
            .filter_map(|(index, universe)| reachable.contains(&index).then_some(universe))
            .collect()
    }

    fn assign_names(universes: Vec<Self>) -> Vec<(String, Self)> {
        // Name collisions are resolved from the backend distribution's own version before
        // falling back to a serial number (two builds can use the same backend version).
        let mut counts = BTreeMap::new();
        for universe in &universes {
            *counts.entry(universe.backend.clone()).or_insert(0usize) += 1;
        }
        let mut used = BTreeSet::from([RUNTIME.to_string()]);
        let mut named = Vec::new();
        for universe in universes {
            let mut base = universe.backend.clone();
            if counts[&base] > 1
                && let Ok(name) = PackageName::from_owned(base.clone())
                && let [package] = universe.lock.packages_for_name(&name)
                && let Some(version) = package.version()
            {
                base = format!("{base}-{version}");
            }
            let mut label = base.clone();
            let mut serial = 1;
            while !used.insert(label.clone()) {
                serial += 1;
                label = format!("{base}:{serial}");
            }
            named.push((label, universe));
        }
        named
    }
}

/// Inputs not fixed by the source artifact's identity. Keep these only on mutable sources or
/// when the user supplies additional build requirements, so freshness checks need no downloads.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct BuildInputs {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    requires: Vec<Requirement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    extra: Vec<ExtraBuildRequirement>,
}

impl BuildInputs {
    pub fn new(
        root: &Path,
        requires: &[Requirement],
        extra: &[ExtraBuildRequirement],
    ) -> std::io::Result<Self> {
        let mut requires = requires
            .iter()
            .cloned()
            .map(|req| req.relative_to(root))
            .collect::<Result<Vec<_>, _>>()?;
        requires.sort();
        requires.dedup();
        let extra = extra
            .iter()
            .map(|entry| {
                Ok(ExtraBuildRequirement {
                    requirement: entry.requirement.clone().relative_to(root)?,
                    match_runtime: entry.match_runtime,
                })
            })
            .collect::<std::io::Result<_>>()?;
        Ok(Self { requires, extra })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PackageBuild {
    pub(super) inputs: BuildInputs,
    /// Non-empty only when this package needs an explicit scope.
    pub(super) universes: Vec<String>,
    /// Selects outgoing edges for the entire build, including transitive dependencies.
    pub(super) universe: Option<String>,
    /// Direct requirements whose version and source don't uniquely identify the locked choice.
    pub(super) dependencies: Vec<Dependency>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct BuildGraph {
    packages: BTreeMap<PackageName, Vec<Package>>,
}

impl Package {
    pub fn id(&self) -> &PackageId {
        &self.id
    }

    /// Return the buildable source, if there is one. Virtual projects are never built.
    pub fn build_source(&self, root: &Path) -> Result<Option<SourceDist>, LockError> {
        if matches!(self.id.source, Source::Virtual(_)) {
            return Ok(None);
        }
        self.to_source_dist(root, FirstParty::No)
    }

    pub fn source_hash_policy(&self) -> ArchiveHashPolicy<'_> {
        self.sdist
            .as_ref()
            .and_then(super::SourceDist::hash)
            .map_or(ArchiveHashPolicy::None, |hash| {
                ArchiveHashPolicy::All(std::slice::from_ref(hash))
            })
    }

    pub fn has_local_build_source(&self) -> bool {
        matches!(self.id.source, Source::Directory(_) | Source::Editable(_))
    }

    pub fn matches_build_inputs(&self, inputs: &BuildInputs) -> bool {
        self.build.as_ref().map_or_else(
            || inputs == &BuildInputs::default(),
            |plan| &plan.inputs == inputs,
        )
    }

    fn universes(&self) -> &[String] {
        self.build.as_ref().map_or(&[], |build| &build.universes)
    }

    fn in_universe(&self, universe: &str) -> bool {
        self.universes().is_empty() || self.universes().iter().any(|name| name == universe)
    }

    fn same_edges(&self, other: &Self) -> bool {
        self.dependencies == other.dependencies
            && self.optional_dependencies == other.optional_dependencies
            && self.dependency_groups == other.dependency_groups
    }
}

impl BuildGraph {
    pub(super) fn packages(&self) -> impl Iterator<Item = &Package> {
        self.packages.values().flatten()
    }

    fn insert(&mut self, package: Package) {
        self.packages
            .entry(package.id.name.clone())
            .or_default()
            .push(package);
    }

    /// Recover the runtime graph by following only runtime edges from the manifest roots.
    /// Keeping build-only nodes out of `Lock::packages` means they cannot influence runtime
    /// freshness checks, export, or selection of a fork.
    pub(super) fn partition(
        packages: Vec<Package>,
        manifest: &ResolverManifest,
    ) -> Result<(Vec<Package>, Self), LockError> {
        let mut runtime = FxHashSet::default();
        let mut queue = VecDeque::new();
        let mut candidates = FxHashMap::default();
        let mut scopes: FxHashMap<&PackageId, BTreeSet<&str>> = FxHashMap::default();
        for package in &packages {
            let assigned = scopes.entry(&package.id).or_default();
            if package.universes().is_empty() {
                if !assigned.is_empty() || !assigned.insert("") {
                    return Err(LockErrorKind::DuplicatePackage {
                        id: package.id.clone(),
                    }
                    .into());
                }
            } else {
                for universe in package.universes() {
                    if universe.is_empty() || assigned.contains("") || !assigned.insert(universe) {
                        return Err(LockErrorKind::DuplicatePackage {
                            id: package.id.clone(),
                        }
                        .into());
                    }
                }
            }
            if !package.in_universe(RUNTIME) {
                continue;
            }
            candidates.insert(&package.id, package);
            if package.id.source.is_implicit_root()
                || manifest.members.contains(package.name())
                || manifest
                    .requirements
                    .iter()
                    .chain(manifest.dependency_groups.values().flatten())
                    .any(|req| req.name == *package.name())
            {
                queue.push_back(&package.id);
            }
        }
        for package in &packages {
            for dep in package
                .all_dependencies()
                .chain(package.build.iter().flat_map(|build| &build.dependencies))
            {
                if !scopes.contains_key(&dep.package_id) {
                    return Err(LockErrorKind::UnrecognizedDependency {
                        id: package.id.clone(),
                        dependency: dep.clone(),
                    }
                    .into());
                }
            }
            // A build-locked file also pins artifacts from indexes that supplied no digest.
            // Check the entire file before any metadata validation can execute a backend.
            let required = package.id.source.requires_hash().unwrap_or(true);
            for (kind, hash) in package
                .wheels
                .iter()
                .map(|wheel| ("wheel", wheel.hash.as_ref()))
                .chain(
                    package
                        .sdist
                        .iter()
                        .map(|sdist| ("source distribution", sdist.hash())),
                )
            {
                let valid = match hash {
                    Some(digest) => required && satisfies_require_hashes(digest),
                    None => !required,
                };
                if !valid {
                    return Err(super::LockHashError::source_requirement(
                        package.id.clone(),
                        kind,
                        required,
                    )
                    .into());
                }
            }
        }
        while let Some(id) = queue.pop_front() {
            if runtime.insert(id.clone()) {
                let package = candidates.get(id).ok_or_else(|| {
                    LockErrorKind::InvalidBuildGraph(format!("no runtime node for `{id}`"))
                })?;
                queue.extend(package.all_dependencies().map(|dep| &dep.package_id));
            }
        }
        let mut build = Self::default();
        let mut roots = Vec::new();
        for package in packages {
            if runtime.contains(&package.id) && package.in_universe(RUNTIME) {
                roots.push(package);
            } else {
                build.insert(package);
            }
        }
        Ok((roots, build))
    }
}

impl Lock {
    pub fn locks_build_dependencies(&self) -> bool {
        self.build.is_some()
    }

    pub fn all_packages(&self) -> impl Iterator<Item = &Package> {
        self.packages
            .iter()
            .chain(self.build.iter().flat_map(|build| build.packages()))
    }

    /// Pair each locked package with the Python versions on which it can be reached from a
    /// resolution root, even when its name wasn't forked into multiple versions.
    pub fn build_python_environments(
        resolution: &ResolverOutput,
        root: &Path,
    ) -> Result<BTreeMap<PackageId, MarkerTree>, LockError> {
        let mut markers = BTreeMap::new();
        for (_, dist) in resolution.base_dists() {
            let id = PackageId::from_annotated_dist(dist, root)?;
            let python = dist.marker.pep508().only_python_version();
            let marker = markers.entry(id).or_insert(MarkerTree::FALSE);
            *marker = marker.or(python);
        }
        Ok(markers)
    }

    /// Reject a runtime-only lock when the caller explicitly requires build dependency locking.
    pub fn require_locked_builds(&self) -> Result<(), LockError> {
        if !self.locks_build_dependencies() {
            return Err(LockErrorKind::MissingLockedBuilds.into());
        }
        Ok(())
    }

    pub fn contains_build_package(&self, name: &PackageName) -> bool {
        self.build
            .as_ref()
            .is_some_and(|build| build.packages.contains_key(name))
    }

    pub(super) fn packages_in_order(&self) -> impl Iterator<Item = &Package> {
        self.packages.iter().merge_by(
            self.build.iter().flat_map(|build| build.packages()),
            |left, right| left.id <= right.id,
        )
    }

    fn build_candidates(&self, name: &PackageName) -> impl Iterator<Item = &Package> {
        self.packages_for_name(name).iter().chain(
            self.build
                .as_ref()
                .and_then(|build| build.packages.get(name))
                .into_iter()
                .flatten(),
        )
    }

    /// Retain the audit decision for a previously locked filename when the index supplies no
    /// hash of its own. Otherwise, an unrelated lock update could authorize a replaced wheel.
    pub fn retain_build_hashes(&mut self, previous: &Self, upgraded: &FxHashSet<PackageName>) {
        for package in &mut self.packages {
            if upgraded.contains(package.name())
                || !matches!(package.id.source, Source::Registry(_))
            {
                continue;
            }
            let Some(locked) = previous
                .build_candidates(package.name())
                .find(|locked| locked.id == package.id)
            else {
                continue;
            };
            for wheel in &mut package.wheels {
                if !wheel.hash.as_ref().is_some_and(satisfies_require_hashes)
                    && let Some(original) = locked
                        .wheels
                        .iter()
                        .find(|old| old.filename == wheel.filename)
                {
                    wheel.hash.clone_from(&original.hash);
                }
            }
            if let (Some(sdist), Some(original)) = (&mut package.sdist, &locked.sdist)
                && !sdist.hash().is_some_and(satisfies_require_hashes)
                && sdist.filename() == original.filename()
            {
                sdist.metadata_mut().hash = original.hash().cloned();
            }
        }
    }

    /// Fill in missing registry hashes before a build graph is authorized for installation.
    /// In particular, a local `--find-links` directory does not advertise artifact hashes.
    pub async fn complete_registry_hashes(
        &mut self,
        client: &RegistryClient,
        root: &Path,
        concurrency: usize,
    ) -> Result<(), LockError> {
        let mut jobs = Vec::new();
        for package in &mut self.packages {
            let Source::Registry(registry) = &package.id.source else {
                continue;
            };
            if package
                .sdist
                .as_ref()
                .is_some_and(|sdist| !sdist.hash().is_some_and(satisfies_require_hashes))
                && let Some(SourceDist::Registry(dist)) =
                    package.to_source_dist(root, FirstParty::No)?
                && let Some(sdist) = &mut package.sdist
            {
                jobs.push((&mut sdist.metadata_mut().hash, dist.file.location));
            }
            for wheel in &mut package.wheels {
                if !wheel.hash.as_ref().is_some_and(satisfies_require_hashes) {
                    let file = wheel.to_registry_wheel(registry, root)?.file;
                    jobs.push((&mut wheel.hash, file.location));
                }
            }
        }

        stream::iter(jobs)
            .map(Ok)
            .try_for_each_concurrent(concurrency, |(destination, location)| async move {
                let url = location.to_url().map_err(LockErrorKind::InvalidUrl)?;
                *destination = Some(
                    client
                        .hash_file(&url)
                        .await
                        .map_err(LockErrorKind::BuildHash)?,
                );
                Ok::<_, LockError>(())
            })
            .await
    }

    /// Merge independently resolved graphs into the flat lockfile. Two nodes can share an ID
    /// *and* their artifacts but disagree about their dependencies; keep a scoped copy for each
    /// distinct set of outgoing edges. A build selects its scope once, at its root.
    pub fn with_build_universes(
        mut self,
        universes: Vec<BuildUniverse>,
        inputs: &BTreeMap<PackageId, BuildInputs>,
        root: &Path,
    ) -> Result<Self, LockError> {
        let named =
            BuildUniverse::assign_names(BuildUniverse::reachable(universes, &self.packages));

        let mut nodes: BTreeMap<PackageId, Vec<(Package, Vec<String>)>> = BTreeMap::new();
        for package in &self.packages {
            nodes.insert(
                package.id.clone(),
                vec![(package.clone(), vec![RUNTIME.into()])],
            );
        }
        for (label, universe) in &named {
            for package in &universe.lock.packages {
                let variants = nodes.entry(package.id.clone()).or_default();
                if let Some((node, scopes)) = variants
                    .iter_mut()
                    .find(|(node, _)| node.same_edges(package))
                {
                    // A platform fork can make different wheels reachable in the two graphs.
                    for wheel in &package.wheels {
                        if let Some(existing) = node.wheels.iter_mut().find(|w| w.url == wheel.url)
                        {
                            if existing.hash.is_none() {
                                existing.hash.clone_from(&wheel.hash);
                            }
                        } else {
                            node.wheels.push(wheel.clone());
                        }
                    }
                    node.wheels.sort_by(|a, b| a.filename.cmp(&b.filename));
                    if node
                        .sdist
                        .as_ref()
                        .is_none_or(|sdist| sdist.hash().is_none())
                    {
                        node.sdist.clone_from(&package.sdist);
                    }
                    scopes.push(label.clone());
                } else {
                    variants.push((package.clone(), vec![label.clone()]));
                }
            }
        }

        let mut names: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for variants in nodes.values() {
            let package = &variants[0].0;
            names.entry(package.name()).or_default().push(package);
        }
        let manifest_names: BTreeSet<_> = self
            .manifest
            .members
            .iter()
            .chain(
                self.manifest
                    .requirements
                    .iter()
                    .chain(self.manifest.dependency_groups.values().flatten())
                    .map(|req| &req.name),
            )
            .collect();
        let needs_scope = |id: &PackageId| {
            nodes[id].len() > 1
                || (!self.by_id.contains_key(id) && manifest_names.contains(&id.name))
        };
        let mut plans = BTreeMap::new();
        for (label, universe) in &named {
            let needs_universe = universe
                .lock
                .packages
                .iter()
                .any(|package| needs_scope(&package.id));
            for owner in &universe.owners {
                let mut plan = PackageBuild {
                    universe: needs_universe.then(|| label.clone()),
                    ..PackageBuild::default()
                };
                for req in &owner.requirements {
                    let Some(candidates) = names.get(&req.name) else {
                        continue;
                    };
                    let mut matching = Vec::new();
                    for candidate in candidates {
                        if candidate
                            .id
                            .source
                            .satisfies_requirement_source(&req.source, root)?
                            && Self::package_satisfies_requirement(candidate, req, root)?
                        {
                            matching.push(candidate);
                        }
                    }
                    if matching.len() < 2 {
                        continue;
                    }
                    for package in universe.lock.packages_for_name(&req.name) {
                        if !Self::package_satisfies_requirement(package, req, root)? {
                            continue;
                        }
                        let selected = if package.fork_markers.is_empty() {
                            MarkerTree::TRUE
                        } else {
                            package
                                .fork_markers
                                .iter()
                                .fold(MarkerTree::FALSE, |m, fork| m.or(fork.pep508()))
                        };
                        plan.dependencies.push(Dependency::new(
                            &self.requires_python,
                            package.id.clone(),
                            BTreeSet::new(),
                            SimplifiedMarkerTree::new(
                                &self.requires_python,
                                req.marker.and(selected),
                            ),
                        ));
                    }
                }
                plan.dependencies.sort();
                plan.dependencies.dedup();
                plans.insert(owner.id.clone(), plan);
            }
        }

        let mut build = BuildGraph::default();
        for (id, variants) in nodes {
            let ambiguous = variants.len() > 1;
            for (mut package, scopes) in variants {
                let runtime = scopes.iter().any(|scope| scope == RUNTIME);
                let mut plan = plans.get(&id).cloned().unwrap_or_default();
                // Manifest roots have no incoming edge to disambiguate a build-only version.
                if ambiguous || (!runtime && manifest_names.contains(&id.name)) {
                    plan.universes = scopes;
                }
                if let Some(inputs) = inputs.get(&id) {
                    plan.inputs.clone_from(inputs);
                }
                if plan != PackageBuild::default() {
                    package.build = Some(Box::new(plan));
                }
                if runtime {
                    let index = self.by_id[&id];
                    self.packages[index.0] = package;
                } else {
                    build.insert(package);
                }
            }
        }
        self.build = Some(Box::new(build));
        Ok(self)
    }
}

impl Lock {
    /// Materialize the environment for a build directly from locked nodes. No index lookup or
    /// backend-provided metadata participates in selecting transitive dependencies.
    pub fn resolve_build(
        &self,
        root: &Path,
        source: &SourceDist,
        requirements: &[Requirement],
        env: &ResolverMarkerEnvironment,
        tags: &Tags,
        options: &BuildOptions,
    ) -> Result<Resolution, LockError> {
        let identity = Source::from_source_dist(source, root)?;
        let mut owners = self.build_candidates(source.name()).filter(|package| {
            (package.id.source == identity || matches!((&package.id.source, &identity),
                (Source::Editable(a), Source::Directory(b)) | (Source::Directory(a), Source::Editable(b)) if a == b))
                && source.version().is_none_or(|version| package.version() == Some(version))
        });
        // All copies of a source have the same build plan, regardless of where it is installed.
        let owner = owners.next().ok_or_else(|| {
            LockErrorKind::InvalidBuildGraph(format!("no entry for source distribution `{source}`"))
        })?;
        let plan = owner.build.as_deref();
        let universe = plan
            .and_then(|plan| plan.universe.as_deref())
            .unwrap_or(RUNTIME);
        let mut graph = Graph::new();
        let graph_root = graph.add_node(Node::Root);
        let mut queue = VecDeque::new();
        for req in requirements
            .iter()
            .filter(|req| req.evaluate_markers(Some(env), &[]))
        {
            let pins = plan
                .into_iter()
                .flat_map(|plan| &plan.dependencies)
                .filter(|dep| {
                    dep.package_id.name == req.name
                        && dep.complexified_marker.evaluate_no_extras(env)
                })
                .map(|dep| &dep.package_id)
                .collect::<Vec<_>>();
            let mut selected = None;
            for package in self.build_candidates(&req.name) {
                if !package.in_universe(universe)
                    || (!pins.is_empty() && !pins.contains(&&package.id))
                    || !package
                        .id
                        .source
                        .satisfies_requirement_source(&req.source, root)?
                    || !Self::package_satisfies_requirement(package, req, root)?
                {
                    continue;
                }
                if selected.replace(package).is_some() {
                    return Err(LockErrorKind::InvalidBuildGraph(format!(
                        "ambiguous build requirement `{req}` for `{}`",
                        owner.id,
                    ))
                    .into());
                }
            }
            let package = selected.ok_or_else(|| {
                LockErrorKind::InvalidBuildGraph(format!(
                    "no locked package satisfies build requirement `{req}` for `{}`",
                    owner.id,
                ))
            })?;
            queue.push_back((graph_root, Edge::Prod, package, req.extras.to_vec()));
        }

        let mut installed = FxHashMap::default();
        let mut visited = FxHashSet::default();
        while let Some((parent, edge, package, extras)) = queue.pop_front() {
            let index = if let Some(&(id, index)) = installed.get(package.name()) {
                if id != &package.id {
                    return Err(LockErrorKind::InvalidBuildGraph(format!(
                        "both `{id}` and `{}` selected while building `{}`",
                        package.id, owner.id,
                    ))
                    .into());
                }
                index
            } else {
                let HashedDist { dist, hashes } = package.to_dist(
                    root,
                    TagPolicy::Required(tags),
                    options,
                    env,
                    FirstParty::No,
                )?;
                if hashes.is_empty() && package.id.source.requires_hash() != Some(false) {
                    return Err(LockErrorKind::InvalidBuildGraph(format!(
                        "missing artifact hash for build dependency `{}` of `{}`",
                        package.id, owner.id,
                    ))
                    .into());
                }
                let index = graph.add_node(Node::Dist {
                    dist: ResolvedDist::Installable {
                        dist: Arc::new(dist),
                        version: package.id.version.clone(),
                    },
                    hashes,
                    install: true,
                });
                installed.insert(package.name(), (&package.id, index));
                index
            };
            graph.add_edge(parent, index, edge);
            for extra in std::iter::once(None).chain(extras.into_iter().map(Some)) {
                if !visited.insert((&package.id, extra.clone())) {
                    continue;
                }
                let deps = match &extra {
                    Some(extra) => package
                        .optional_dependencies
                        .get(extra)
                        .map_or(&[][..], Vec::as_slice),
                    None => &package.dependencies,
                };
                for dep in deps {
                    if !dep.complexified_marker.evaluate_no_extras(env) {
                        continue;
                    }
                    let target = self
                        .build_candidates(dep.package_name())
                        .filter(|target| {
                            target.id == dep.package_id && target.in_universe(universe)
                        })
                        .exactly_one()
                        .map_err(|_| {
                            LockErrorKind::InvalidBuildGraph(format!(
                                "expected one node for `{}` in universe `{universe}`",
                                dep.package_id,
                            ))
                        })?;
                    queue.push_back((
                        index,
                        extra.clone().map_or(Edge::Prod, Edge::Optional),
                        target,
                        dep.extra.iter().cloned().collect(),
                    ));
                }
            }
        }
        Ok(Resolution::new(graph))
    }
}

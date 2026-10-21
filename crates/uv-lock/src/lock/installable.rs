use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::collections::hash_map::Entry;
use std::path::Path;
use std::sync::Arc;

use itertools::Itertools;
use petgraph::Graph;
use petgraph::graph::NodeIndex;
use rustc_hash::{FxHashMap, FxHashSet};

use uv_cache::Cache;
use uv_client::RegistryClient;
use uv_configuration::{
    BuildOptions, DependencyGroupsWithDefaults, ExtrasSpecification,
    ExtrasSpecificationWithDefaults, InstallOptions,
};
use uv_distribution::{DistributionDatabase, read_locked_wheel_variant};
use uv_distribution_types::{
    BuiltDist, Dist, Edge, FirstParty, Node, RegistryBuiltDist, Resolution, ResolvedDist,
    SelectedVariant, VariantsJsonContent,
};
use uv_normalize::{DefaultExtras, ExtraName, GroupName, PackageName};
use uv_pep508::{MarkerTree, MarkerVariant, MarkerVariantsEnvironment};
use uv_platform_tags::Tags;
use uv_pypi_types::{ConflictItem, ConflictKind, ConflictSet, ResolverMarkerEnvironment};
use uv_types::BuildContext;

use uv_resolver_types::UniversalMarker;
use uv_resolver_types::universal_marker::ActivatedConflictItems;

use crate::lock::{
    Dependency, DependencySelectionContext, HashedDist, LockErrorKind, Package, PackageIndex,
    SelectedDependency, TagPolicy,
};
use crate::{Lock, LockError};

fn newly_activated_extras<'lock>(
    dep: &'lock Dependency,
    activated_extras: &[(&'lock PackageName, &'lock ExtraName)],
) -> Vec<(&'lock PackageName, &'lock ExtraName)> {
    dep.extra
        .iter()
        .filter_map(|extra| {
            let key = (&dep.package_id.name, extra);
            (!activated_extras.contains(&key)).then_some(key)
        })
        .collect()
}

/// Record another condition under which a locked package and optional extra are reachable.
///
/// Returns `true` when the combined reachability changed.
fn add_reachability<'lock>(
    reachability: &mut FxHashMap<(PackageIndex, Option<&'lock ExtraName>), UniversalMarker>,
    key: (PackageIndex, Option<&'lock ExtraName>),
    marker: UniversalMarker,
) -> bool {
    match reachability.entry(key) {
        Entry::Occupied(mut entry) => {
            let mut combined = *entry.get();
            combined.or(marker);
            if combined == *entry.get() {
                false
            } else {
                entry.insert(combined);
                true
            }
        }
        Entry::Vacant(entry) => {
            entry.insert(marker);
            true
        }
    }
}

/// Returns the dependencies a queued package contributes, either its own or those of one extra.
fn package_dependencies<'a>(
    package: &'a Package,
    extra: Option<&ExtraName>,
) -> impl Iterator<Item = &'a Dependency> {
    let dependencies = match extra {
        Some(extra) => package
            .optional_dependencies
            .get(extra)
            .map(Vec::as_slice)
            .unwrap_or_default(),
        None => &package.dependencies,
    };
    dependencies.iter()
}

fn push_possible_dependency<'lock>(
    queue: &mut Vec<(PackageIndex, Option<&'lock ExtraName>)>,
    dependency: &'lock Dependency,
    marker_env: &ResolverMarkerEnvironment,
) {
    if !dependency
        .complexified_marker
        .pep508()
        .evaluate(marker_env, &[])
    {
        return;
    }
    queue.push((dependency.index, None));
    queue.extend(
        dependency
            .extra
            .iter()
            .map(|extra| (dependency.index, Some(extra))),
    );
}

/// Find extras that could be activated from the selected roots. The environment is fixed, while
/// conflict items and variant selections may change during the traversal.
fn potentially_activated_extras<'lock>(
    lock: &'lock Lock,
    mut queue: Vec<(PackageIndex, Option<&'lock ExtraName>)>,
    marker_env: &ResolverMarkerEnvironment,
) -> FxHashSet<(&'lock PackageName, &'lock ExtraName)> {
    let mut seen = FxHashSet::default();
    let mut extras = FxHashSet::default();
    while let Some((index, extra)) = queue.pop() {
        if !seen.insert((index, extra)) {
            continue;
        }
        let package = lock.package(index);
        if let Some(extra) = extra {
            extras.insert((package.name(), extra));
        }
        for dependency in package_dependencies(package, extra) {
            push_possible_dependency(&mut queue, dependency, marker_env);
        }
    }
    extras
}

/// Determines which dependencies are included from an install target root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallableRootKind {
    /// Include the root's production dependencies and selected dependency groups.
    Production,
    /// Include only the root's selected dependency groups.
    DependencyGroups,
}

pub trait Installable<'lock> {
    /// Return the root install path.
    fn install_path(&self) -> &'lock Path;

    /// Return the [`Lock`] to install.
    fn lock(&self) -> &'lock Lock;

    /// Return the [`PackageName`] of the root packages in the target.
    fn roots(&self) -> impl Iterator<Item = &PackageName>;

    /// Return the package whose dependency groups, but not production dependencies, are included.
    fn group_root(&self, _groups: &DependencyGroupsWithDefaults) -> Option<&PackageName> {
        None
    }

    /// Return whether a dependency group should be included for its owning package.
    ///
    /// A `None` package represents groups defined directly on a non-project workspace root.
    fn includes_group(
        &self,
        _package: Option<&PackageName>,
        group: &GroupName,
        groups: &DependencyGroupsWithDefaults,
    ) -> bool {
        groups.contains(group)
    }

    /// Return the [`PackageName`] of the target, if available.
    fn project_name(&self) -> Option<&PackageName>;

    /// Convert the [`Lock`] to a [`Resolution`] using the given marker environment, tags, and root.
    fn to_resolution(
        &self,
        marker_env: &ResolverMarkerEnvironment,
        tags: &Tags,
        extras: &ExtrasSpecificationWithDefaults,
        groups: &DependencyGroupsWithDefaults,
        build_options: &BuildOptions,
        install_options: &InstallOptions,
    ) -> Result<Resolution, LockError> {
        let (roots, group_root) = self.resolution_roots(groups)?;

        InstallableExt::traverse_packages(
            self,
            &roots,
            group_root,
            true,
            DependencySelectionContext::None,
            marker_env,
            extras,
            groups,
            tags,
            build_options,
            install_options,
            None,
        )
        .map(|traversal| Resolution::new(traversal.graph))
    }

    /// Return whether this lock contains a wheel variant.
    fn has_variants(&self) -> bool {
        self.lock().packages().iter().any(package_has_variants)
    }

    /// Select each reachable package's artifact before following its dependencies.
    #[expect(async_fn_in_trait)]
    async fn to_resolution_with_variants<Context: BuildContext>(
        &self,
        marker_env: &ResolverMarkerEnvironment,
        tags: &Tags,
        extras: &ExtrasSpecificationWithDefaults,
        groups: &DependencyGroupsWithDefaults,
        build_options: &BuildOptions,
        install_options: &InstallOptions,
        client: &RegistryClient,
        database: &DistributionDatabase<'_, Context>,
        cache: &Cache,
        require_provider_hashes: bool,
    ) -> Result<Resolution, LockError> {
        let (roots, group_root) = self.resolution_roots(groups)?;
        let mut selections = VariantSelections::default();
        loop {
            let traversal = self.traverse_packages(
                &roots,
                group_root,
                true,
                DependencySelectionContext::None,
                marker_env,
                extras,
                groups,
                tags,
                build_options,
                install_options,
                Some(&selections),
            )?;
            let mut pending = pending_variants(&traversal.inverse).peekable();
            if pending.peek().is_none() {
                let contexts = selections.selected.iter().filter_map(|(index, node)| {
                    let Node::Dist {
                        dist: ResolvedDist::Installable { dist, .. },
                        ..
                    } = node
                    else {
                        return None;
                    };
                    let Dist::Built(built) = dist.as_ref() else {
                        return None;
                    };
                    let filename = built.wheel_filename();
                    let variant = selections
                        .variants
                        .get(&variant_base(self.lock().package(*index)))?;
                    Some((filename.clone(), variant.clone()))
                });
                return Ok(Resolution::new(traversal.graph).with_variant_contexts(contexts));
            }
            if !pending.clone().any(|(_, _, stable)| stable)
                && let Some((index, _, _)) = pending.peek()
            {
                return Err(LockErrorKind::VariantConflictReachability {
                    package_id: self.lock().package(*index).id.clone(),
                }
                .into());
            }
            for (index, install, stable) in pending {
                if !stable {
                    continue;
                }
                let package = self.lock().package(index);
                let default_build_options = BuildOptions::default();
                let build_options = if install {
                    build_options
                } else {
                    &default_build_options
                };
                let to_node = |package: &Package| {
                    node_from_package(
                        self,
                        package,
                        TagPolicy::Variant(tags),
                        marker_env,
                        build_options,
                        install,
                    )
                };
                let node = to_node(package)?;
                let selected = select_variant(
                    package,
                    node,
                    self.install_path(),
                    tags,
                    client,
                    database,
                    cache,
                    require_provider_hashes,
                )
                .await?;
                let (node, variant) = if let Some(selected) = selected {
                    selected
                } else {
                    let mut source_package = package.clone();
                    source_package.wheels.clear();
                    (to_node(&source_package)?, None)
                };
                if let Some(variant) = variant {
                    selections.variants.insert(variant_base(package), variant);
                } else {
                    selections.variants.remove(&variant_base(package));
                }
                selections.selected.insert(index, node);
            }
        }
    }

    /// Create an installable [`Node`] from a [`Package`].
    fn installable_node(
        &self,
        package: &Package,
        tags: &Tags,
        marker_env: &ResolverMarkerEnvironment,
        build_options: &BuildOptions,
    ) -> Result<Node, LockError> {
        node_from_package(
            self,
            package,
            TagPolicy::Required(tags),
            marker_env,
            build_options,
            true,
        )
    }

    /// Create a non-installable [`Node`] from a [`Package`].
    fn non_installable_node(
        &self,
        package: &Package,
        tags: &Tags,
        marker_env: &ResolverMarkerEnvironment,
    ) -> Result<Node, LockError> {
        node_from_package(
            self,
            package,
            TagPolicy::Preferred(tags),
            marker_env,
            &BuildOptions::default(),
            false,
        )
    }

    /// Convert a lockfile entry to a graph [`Node`].
    fn package_to_node(
        &self,
        package: &Package,
        tags: &Tags,
        build_options: &BuildOptions,
        install_options: &InstallOptions,
        marker_env: &ResolverMarkerEnvironment,
    ) -> Result<Node, LockError> {
        if install_options.include_package(
            package.as_install_target(),
            self.project_name(),
            self.lock().members(),
        ) {
            self.installable_node(package, tags, marker_env, build_options)
        } else {
            self.non_installable_node(package, tags, marker_env)
        }
    }
}

/// Internal lock-to-resolution implementation shared by [`Installable`] and [`Lock`].
trait InstallableExt<'lock>: Installable<'lock> {
    /// Find the root packages whose dependencies to traverse.
    fn resolution_roots(
        &self,
        groups: &DependencyGroupsWithDefaults,
    ) -> Result<(Vec<&'lock Package>, Option<&'lock Package>), LockError> {
        let resolve_root = |root_name: &PackageName| {
            self.lock()
                .find_by_name(root_name)
                .map_err(|_| LockErrorKind::MultipleRootPackages {
                    name: root_name.clone(),
                })?
                .ok_or_else(|| {
                    LockError::from(LockErrorKind::MissingRootPackage {
                        name: root_name.clone(),
                    })
                })
        };
        let roots = self
            .roots()
            .map(&resolve_root)
            .collect::<Result<Vec<_>, LockError>>()?;
        let group_root = self.group_root(groups).map(resolve_root).transpose()?;
        Ok((roots, group_root))
    }

    /// Traverse the dependencies of concrete locked packages.
    ///
    /// The graph is complete when the inverse table has no reachable pending variants.
    ///
    /// `include_manifest` controls whether requirements attached directly to the lock target are
    /// included in addition to `roots`.
    fn traverse_packages(
        &self,
        roots: &[&Package],
        group_root: Option<&Package>,
        include_manifest: bool,
        selection_context: DependencySelectionContext<'lock>,
        marker_env: &ResolverMarkerEnvironment,
        extras: &ExtrasSpecificationWithDefaults,
        groups: &DependencyGroupsWithDefaults,
        tags: &Tags,
        build_options: &BuildOptions,
        install_options: &InstallOptions,
        selections: Option<&VariantSelections>,
    ) -> Result<Traversal, LockError> {
        let variant_environment = |package: &Package| CurrentVariant {
            current: selections
                .and_then(|selections| selections.variants.get(&variant_base(package))),
            all: selections.map(|selections| &selections.variants),
        };
        let insert_node = |graph: &mut Graph<Node, Edge>,
                           state: &mut Option<TraversalNode>,
                           package_index: PackageIndex,
                           install: bool,
                           update: bool|
         -> Result<Option<NodeIndex>, LockError> {
            if let Some(TraversalNode::Ready(index)) = *state
                && !update
            {
                return Ok(Some(index));
            }
            let package = self.lock().package(package_index);
            let install = install
                || matches!(
                    *state,
                    Some(TraversalNode::PendingVariant { install: true, .. })
                );
            let node = if let Some(selections) = selections
                && package_has_variants(package)
            {
                let install = install
                    && install_options.include_package(
                        package.as_install_target(),
                        self.project_name(),
                        self.lock().members(),
                    );
                if let Some(Node::Dist {
                    dist,
                    hashes,
                    install: selected_install,
                }) = selections.selected.get(&package_index)
                    && (!install || *selected_install)
                {
                    Node::Dist {
                        dist: dist.clone(),
                        hashes: hashes.clone(),
                        install,
                    }
                } else {
                    *state = Some(TraversalNode::PendingVariant {
                        install,
                        stable: None,
                    });
                    return Ok(None);
                }
            } else if install {
                if package_has_variants(package) {
                    return Err(LockErrorKind::VariantSelectionRequired {
                        package_id: package.id.clone(),
                    }
                    .into());
                }
                self.package_to_node(package, tags, build_options, install_options, marker_env)?
            } else {
                self.non_installable_node(package, tags, marker_env)?
            };
            let index = match *state {
                Some(TraversalNode::Ready(index)) => {
                    graph[index] = node;
                    index
                }
                Some(TraversalNode::PendingVariant { .. }) | None => graph.add_node(node),
            };
            *state = Some(TraversalNode::Ready(index));
            Ok(Some(index))
        };
        let size_guess = self.lock().packages.len();
        let mut petgraph = Graph::with_capacity(size_guess, size_guess);
        let mut inverse = vec![None; size_guess];

        let mut queue: VecDeque<(PackageIndex, Option<&ExtraName>)> = VecDeque::new();
        let mut seen = FxHashSet::default();
        let mut conflict_reachability = FxHashMap::default();
        let mut activated_projects: Vec<&PackageName> = vec![];
        let mut activated_extras: Vec<(&PackageName, &ExtraName)> = vec![];
        let mut activated_groups: Vec<(&PackageName, &GroupName)> = vec![];
        let has_conflicts = !self.lock().conflicts().is_empty();
        let track_variant_conflicts = has_conflicts && selections.is_some();
        // A variant expression belongs to the package declaring the dependency. Evaluate it before
        // propagating the remaining conflict condition to the dependency's package.
        let variant_conflict = |marker: UniversalMarker, package: &Package| {
            UniversalMarker::new(
                MarkerTree::TRUE,
                marker.conflict_for_environment_with_variants(
                    marker_env,
                    &variant_environment(package),
                ),
            )
        };
        let validate_conflicts = !include_manifest && has_conflicts;
        let mut dependencies_for_conflict_validation = vec![];

        let root = petgraph.add_node(Node::Root);

        match selection_context {
            DependencySelectionContext::None => {}
            DependencySelectionContext::Production(project) => {
                activated_projects.push(project);
            }
            DependencySelectionContext::Group(project, group) => {
                activated_groups.push((project, group));
            }
        }

        // Determine the set of activated extras and groups, from the root.
        //
        // Extras activated by dependency groups (via `pkg[extra]` entries in the group) are
        // accumulated below, when we process the groups themselves. This ensures that when we
        // later evaluate conflict markers on transitive dependencies, self-extras enabled by an
        // active group are treated as enabled.
        //
        // TODO(zanieb): For completeness, the group-dep loop below still has two structural
        // soundness gaps. Neither is reachable through lockfiles the resolver currently
        // produces — they'd require a group-dep entry with a strict positive conflict marker
        // referencing an extra *other* than the entry's own self-extra, and the resolver
        // either emits self-extra markers (handled by `newly_activated_extras` below) or
        // markers with vacuously-true disjuncts. But the code should still handle them:
        //
        // 1. Ordering: an earlier group-dep entry whose marker references an extra activated
        //    by a later entry is evaluated with an incomplete `activated_extras` set.
        // 2. Transitive self-extras: a group dep `pkg[a]` where `pkg.optional_dependencies.a`
        //    includes `pkg[b]` only activates `(pkg, b)` during the first-pass traversal
        //    below, so any group dep whose marker needs `(pkg, b)` is evaluated too early.
        //
        // Fixing these correctly likely means iterating group-dep activation to a fixed point
        // or interleaving it with the first-pass traversal.
        if has_conflicts {
            for dist in roots.iter().copied() {
                // Track the activated extras.
                if groups.prod() {
                    activated_projects.push(&dist.id.name);
                    for extra in extras.extra_names(dist.optional_dependencies.keys()) {
                        activated_extras.push((&dist.id.name, extra));
                    }
                }
            }

            for dist in roots.iter().copied().chain(group_root) {
                for group in dist
                    .dependency_groups
                    .keys()
                    .filter(|group| self.includes_group(Some(&dist.id.name), group, groups))
                {
                    activated_groups.push((&dist.id.name, group));
                }
            }
        }

        // Initialize the workspace roots.
        let mut initialized_roots = vec![];
        for (dist, root_kind) in roots
            .iter()
            .copied()
            .map(|dist| (dist, InstallableRootKind::Production))
            .chain(group_root.map(|dist| (dist, InstallableRootKind::DependencyGroups)))
        {
            // Add the workspace package to the graph.
            let package_index = self.lock().by_id[&dist.id];
            let install = root_kind == InstallableRootKind::Production && groups.prod();
            // Each root has its own graph node, even when roots name the same package.
            if matches!(inverse[package_index.0], Some(TraversalNode::Ready(_))) {
                inverse[package_index.0] = None;
            }
            let index = insert_node(
                &mut petgraph,
                &mut inverse[package_index.0],
                package_index,
                install,
                false,
            )?;
            if let Some(index) = index {
                petgraph.add_edge(root, index, Edge::Prod);
            }

            // Push the package onto the queue.
            initialized_roots.push((dist, package_index, index, root_kind));
        }

        // Add the workspace dependencies to the queue.
        for (dist, package_index, index, root_kind) in initialized_roots {
            let Some(index) = index else {
                queue.push_back((package_index, None));
                add_reachability(
                    &mut conflict_reachability,
                    (package_index, None),
                    UniversalMarker::TRUE,
                );
                continue;
            };
            if root_kind == InstallableRootKind::Production && groups.prod() {
                // Push its dependencies onto the queue.
                queue.push_back((package_index, None));
                add_reachability(
                    &mut conflict_reachability,
                    (package_index, None),
                    UniversalMarker::TRUE,
                );
                for extra in extras.extra_names(dist.optional_dependencies.keys()) {
                    queue.push_back((package_index, Some(extra)));
                    add_reachability(
                        &mut conflict_reachability,
                        (package_index, Some(extra)),
                        UniversalMarker::TRUE,
                    );
                }
            }

            // Add any dev dependencies.
            for (group, dep) in dist
                .dependency_groups
                .iter()
                .filter_map(|(group, deps)| {
                    if self.includes_group(Some(&dist.id.name), group, groups) {
                        Some(deps.iter().map(move |dep| (group, dep)))
                    } else {
                        None
                    }
                })
                .flatten()
            {
                if validate_conflicts && dep.complexified_marker.has_conflict_marker() {
                    dependencies_for_conflict_validation.push((dist, dep));
                }
                let additional_activated_extras = newly_activated_extras(dep, &activated_extras);
                if !dep.complexified_marker.evaluate_with_variants(
                    marker_env,
                    &variant_environment(dist),
                    activated_projects.iter().copied(),
                    activated_extras
                        .iter()
                        .chain(additional_activated_extras.iter())
                        .copied(),
                    activated_groups.iter().copied(),
                ) {
                    continue;
                }

                // A workspace member omitted by the root may be required by this group.
                let update = !groups.prod()
                    || inverse[dep.index.0].is_some_and(|state| state.is_omitted(&petgraph));
                if let Some(dep_index) = insert_node(
                    &mut petgraph,
                    &mut inverse[dep.index.0],
                    dep.index,
                    true,
                    update,
                )? {
                    // The conflict marker has already been evaluated for this environment.
                    petgraph.add_edge(index, dep_index, Edge::Dev(group.clone()));
                }

                // Persist any self-extras activated by this group dependency (e.g., a group
                // that references `pkg[extra]`). Without this, conflict markers on transitive
                // dependencies gated by the activated extra would not evaluate to `true`
                // during the graph traversals below.
                for key in additional_activated_extras {
                    activated_extras.push(key);
                }

                // Push its dependencies on the queue.
                let dependency_reachability = if track_variant_conflicts {
                    variant_conflict(dep.complexified_marker, dist)
                } else {
                    dep.complexified_marker
                };
                add_reachability(
                    &mut conflict_reachability,
                    (dep.index, None),
                    dependency_reachability,
                );
                if seen.insert((dep.index, None)) {
                    queue.push_back((dep.index, None));
                }
                for extra in &dep.extra {
                    add_reachability(
                        &mut conflict_reachability,
                        (dep.index, Some(extra)),
                        dependency_reachability,
                    );
                    if seen.insert((dep.index, Some(extra))) {
                        queue.push_back((dep.index, Some(extra)));
                    }
                }
            }
        }

        if include_manifest {
            // Add any requirements that are exclusive to the workspace root (e.g., dependencies in
            // PEP 723 scripts).
            for dependency in self.lock().requirements() {
                if !dependency.marker.evaluate(marker_env, &[]) {
                    continue;
                }

                let root_name = &dependency.name;
                let dist = self
                    .lock()
                    .find_by_markers(root_name, marker_env)
                    .map_err(|_| LockErrorKind::MultipleRootPackages {
                        name: root_name.clone(),
                    })?
                    .ok_or_else(|| LockErrorKind::MissingRootPackage {
                        name: root_name.clone(),
                    })?;

                // Add the package to the graph.
                let package_index = self.lock().by_id[&dist.id];
                if let Some(index) = insert_node(
                    &mut petgraph,
                    &mut inverse[package_index.0],
                    package_index,
                    groups.prod(),
                    false,
                )? {
                    petgraph.add_edge(root, index, Edge::Prod);
                }

                // Push its dependencies on the queue.
                add_reachability(
                    &mut conflict_reachability,
                    (package_index, None),
                    UniversalMarker::TRUE,
                );
                if seen.insert((package_index, None)) {
                    queue.push_back((package_index, None));
                }
                for extra in &dependency.extras {
                    add_reachability(
                        &mut conflict_reachability,
                        (package_index, Some(extra)),
                        UniversalMarker::TRUE,
                    );
                    if seen.insert((package_index, Some(extra))) {
                        queue.push_back((package_index, Some(extra)));
                    }
                }
            }

            // Add any dependency groups that are exclusive to the workspace root (e.g., dev
            // dependencies in non-project workspace roots).
            for (group, dependency) in self
                .lock()
                .dependency_groups()
                .iter()
                .filter_map(|(group, deps)| {
                    if self.includes_group(None, group, groups) {
                        Some(deps.iter().map(move |dep| (group, dep)))
                    } else {
                        None
                    }
                })
                .flatten()
            {
                if !dependency.marker.evaluate(marker_env, &[]) {
                    continue;
                }

                let root_name = &dependency.name;
                let dist = self
                    .lock()
                    .find_by_markers(root_name, marker_env)
                    .map_err(|_| LockErrorKind::MultipleRootPackages {
                        name: root_name.clone(),
                    })?
                    .ok_or_else(|| LockErrorKind::MissingRootPackage {
                        name: root_name.clone(),
                    })?;

                // Add the package to the graph.
                let package_index = self.lock().by_id[&dist.id];
                if let Some(index) = insert_node(
                    &mut petgraph,
                    &mut inverse[package_index.0],
                    package_index,
                    true,
                    !groups.prod(),
                )? {
                    petgraph.add_edge(root, index, Edge::Dev(group.clone()));
                }

                // Persist any self-extras activated by this group dependency. Mirrors the
                // handling in the package-level `dependency_groups` loop above; without this,
                // conflict markers on transitive dependencies gated by the activated extra
                // would not evaluate to `true` during the graph traversals below.
                for extra in &dependency.extras {
                    let key = (&dist.id.name, extra);
                    if !activated_extras.contains(&key) {
                        activated_extras.push(key);
                    }
                }

                // Push its dependencies on the queue.
                add_reachability(
                    &mut conflict_reachability,
                    (package_index, None),
                    UniversalMarker::TRUE,
                );
                if seen.insert((package_index, None)) {
                    queue.push_back((package_index, None));
                }
                for extra in &dependency.extras {
                    add_reachability(
                        &mut conflict_reachability,
                        (package_index, Some(extra)),
                        UniversalMarker::TRUE,
                    );
                    if seen.insert((package_index, Some(extra))) {
                        queue.push_back((package_index, Some(extra)));
                    }
                }
            }
        }

        // Below, we traverse the dependency graph in a breadth first manner
        // twice. It's only in the second traversal that we actually build
        // up our resolution graph. In the first traversal, we accumulate all
        // activated extras. This includes the extras explicitly enabled on
        // the CLI (which were gathered above) and the extras enabled via
        // dependency specifications like `foo[extra]`. We need to do this
        // to correctly support conflicting extras.
        //
        // In particular, the way conflicting extras works is by forking the
        // resolver based on the extras that are declared as conflicting. But
        // this forking needs to be made manifest somehow in the lock file to
        // avoid multiple versions of the same package being installed into the
        // environment. This is why "conflict markers" were invented. For
        // example, you might have both `torch` and `torch+cpu` in your
        // dependency graph, where the latter is only enabled when the `cpu`
        // extra is enabled, and the former is specifically *not* enabled
        // when the `cpu` extra is enabled.
        //
        // In order to evaluate these conflict markers correctly, we need to
        // know whether the `cpu` extra is enabled when we visit the `torch`
        // dependency. If we think it's disabled, then we'll erroneously
        // include it if the extra is actually enabled. But in order to tell
        // if it's enabled, we need to traverse the entire dependency graph
        // first to inspect which extras are enabled!
        //
        // Of course, we don't need to do this at all if there aren't any
        // conflicts. In which case, we skip all of this and just do the one
        // traversal below.
        if has_conflicts {
            let mut activated_extras_set: BTreeSet<(&PackageName, &ExtraName)> =
                activated_extras.iter().copied().collect();
            let mut queue = queue.clone();
            let mut reachability = if track_variant_conflicts {
                conflict_reachability.clone()
            } else {
                std::mem::take(&mut conflict_reachability)
            };
            while let Some((package_index, extra)) = queue.pop_front() {
                let package = self.lock().package(package_index);
                if selections.is_some_and(|selections| {
                    package_has_variants(package)
                        && (!selections.selected.contains_key(&package_index)
                            || matches!(
                                inverse[package_index.0],
                                Some(TraversalNode::PendingVariant { .. })
                            ))
                }) {
                    continue;
                }
                let Some(parent_reachability) = reachability.get(&(package_index, extra)).copied()
                else {
                    continue;
                };
                for dep in package_dependencies(package, extra) {
                    let mut dep_reachability = if track_variant_conflicts {
                        variant_conflict(dep.complexified_marker, package)
                    } else {
                        dep.complexified_marker
                    };
                    dep_reachability.and(parent_reachability);
                    let additional_activated_extras =
                        newly_activated_extras(dep, &activated_extras);
                    if !dep_reachability.evaluate_with_variants(
                        marker_env,
                        &variant_environment(package),
                        activated_projects.iter().copied(),
                        activated_extras
                            .iter()
                            .chain(additional_activated_extras.iter())
                            .copied(),
                        activated_groups.iter().copied(),
                    ) {
                        continue;
                    }
                    // The dependency can still be visited provisionally before all activated
                    // extras are known. The second traversal below will exclude it once those
                    // extras are available. Crucially, `dep_reachability` includes the conditions
                    // required to reach the parent package: dependency markers may have been
                    // simplified under those conditions and cannot stand alone during this
                    // preliminary traversal. Otherwise, an unreachable package could activate an
                    // extra and cause the conflict check below to report a false positive.

                    for key in additional_activated_extras {
                        activated_extras_set.insert(key);
                        activated_extras.push(key);
                    }
                    // Push its dependencies on the queue.
                    if add_reachability(&mut reachability, (dep.index, None), dep_reachability) {
                        queue.push_back((dep.index, None));
                    }
                    for extra in &dep.extra {
                        if add_reachability(
                            &mut reachability,
                            (dep.index, Some(extra)),
                            dep_reachability,
                        ) {
                            queue.push_back((dep.index, Some(extra)));
                        }
                    }
                }
            }
            // At time of writing, it's somewhat expected that the set of
            // conflicting extras is pretty small. With that said, the
            // time complexity of the following routine is pretty gross.
            // Namely, `set.contains` is linear in the size of the set,
            // iteration over all conflicts is also obviously linear in
            // the number of conflicting sets and then for each of those,
            // we visit every possible pair of activated extra from above,
            // which is quadratic in the total number of extras enabled. I
            // believe the simplest improvement here, if it's necessary, is
            // to adjust the `Conflicts` internals to own these sorts of
            // checks. ---AG
            for set in self.lock().conflicts().iter() {
                for ((pkg1, extra1), (pkg2, extra2)) in
                    activated_extras_set.iter().tuple_combinations()
                {
                    if set.contains(pkg1, *extra1) && set.contains(pkg2, *extra2) {
                        return Err(LockErrorKind::ConflictingExtra {
                            package1: (*pkg1).clone(),
                            extra1: (*extra1).clone(),
                            package2: (*pkg2).clone(),
                            extra2: (*extra2).clone(),
                        }
                        .into());
                    }
                }
            }
        }

        // Unlike the traversals above, this one never activates an extra, so the activated set is
        // fixed for its duration and can be encoded once instead of once per dependency.
        let activated = ActivatedConflictItems::new(
            activated_projects.iter().copied(),
            activated_extras.iter().copied(),
            activated_groups.iter().copied(),
        );

        let mut unresolved = FxHashSet::default();
        while let Some((package_index, extra)) = queue.pop_front() {
            let package = self.lock().package(package_index);
            let Some(TraversalNode::Ready(index)) = inverse[package_index.0] else {
                unresolved.insert((package_index, extra));
                continue;
            };
            let reachability = if track_variant_conflicts {
                conflict_reachability
                    .get(&(package_index, extra))
                    .copied()
                    .unwrap_or(UniversalMarker::TRUE)
            } else {
                UniversalMarker::TRUE
            };
            if track_variant_conflicts
                && !reachability.evaluate_activated_with_variants(
                    marker_env,
                    &variant_environment(package),
                    &activated,
                )
            {
                continue;
            }
            for dep in package_dependencies(package, extra) {
                if validate_conflicts && dep.complexified_marker.has_conflict_marker() {
                    dependencies_for_conflict_validation.push((package, dep));
                }
                if !dep.complexified_marker.evaluate_activated_with_variants(
                    marker_env,
                    &variant_environment(package),
                    &activated,
                ) {
                    continue;
                }

                let update = inverse[dep.index.0].is_some_and(|state| state.is_omitted(&petgraph));
                let dep_index = insert_node(
                    &mut petgraph,
                    &mut inverse[dep.index.0],
                    dep.index,
                    true,
                    update,
                )?;
                if let Some(dep_index) = dep_index {
                    petgraph.add_edge(
                        index,
                        dep_index,
                        if let Some(extra) = extra {
                            Edge::Optional(extra.clone())
                        } else {
                            Edge::Prod
                        },
                    );
                }

                // Push its dependencies on the queue.
                let dependency_reachability = track_variant_conflicts.then(|| {
                    let mut dependency_reachability = reachability;
                    dependency_reachability.and(variant_conflict(dep.complexified_marker, package));
                    dependency_reachability
                });
                if let Some(dependency_reachability) = dependency_reachability {
                    add_reachability(
                        &mut conflict_reachability,
                        (dep.index, None),
                        dependency_reachability,
                    );
                }
                if seen.insert((dep.index, None)) || (update && dep_index.is_none()) {
                    queue.push_back((dep.index, None));
                }
                for extra in &dep.extra {
                    if let Some(dependency_reachability) = dependency_reachability {
                        add_reachability(
                            &mut conflict_reachability,
                            (dep.index, Some(extra)),
                            dependency_reachability,
                        );
                    }
                    if seen.insert((dep.index, Some(extra))) {
                        queue.push_back((dep.index, Some(extra)));
                    }
                }
            }
        }

        let is_active = |item: &ConflictItem| match item.kind() {
            ConflictKind::Project => activated_projects.contains(&item.package()),
            ConflictKind::Extra(extra) => activated_extras.contains(&(item.package(), extra)),
            ConflictKind::Group(group) => activated_groups.contains(&(item.package(), group)),
        };
        if !unresolved.is_empty() {
            let potentially_activated_extras = if has_conflicts {
                let mut possible_roots = conflict_reachability.keys().copied().collect::<Vec<_>>();
                if groups.prod() {
                    for root in roots {
                        let index = self.lock().by_id[&root.id];
                        possible_roots.extend(
                            extras
                                .extra_names(root.optional_dependencies.keys())
                                .map(|extra| (index, Some(extra))),
                        );
                    }
                }
                for root in roots.iter().copied().chain(group_root) {
                    for (group, dependencies) in &root.dependency_groups {
                        if self.includes_group(Some(root.name()), group, groups) {
                            for dependency in dependencies {
                                push_possible_dependency(
                                    &mut possible_roots,
                                    dependency,
                                    marker_env,
                                );
                            }
                        }
                    }
                }
                potentially_activated_extras(self.lock(), possible_roots, marker_env)
            } else {
                FxHashSet::default()
            };
            // Selection can reveal additional extras. Run providers only for packages that stay
            // reachable regardless of extras that unresolved packages might activate.
            for (package_index, extra) in unresolved {
                let package = self.lock().package(package_index);
                let reachability = if track_variant_conflicts {
                    conflict_reachability[&(package_index, extra)]
                } else {
                    UniversalMarker::TRUE
                };
                if track_variant_conflicts
                    && !reachability.evaluate_activated_with_variants(
                        marker_env,
                        &variant_environment(package),
                        &activated,
                    )
                {
                    continue;
                }
                let stable = if has_conflicts {
                    let mut stable_reachability = reachability;
                    for set in self.lock().conflicts().iter() {
                        let set_active = set.iter().any(is_active);
                        for item in set.iter() {
                            if is_active(item) {
                                stable_reachability.assume_conflict_item(item);
                            } else {
                                let fixed_inactive = match item.kind() {
                                    ConflictKind::Project | ConflictKind::Group(_) => true,
                                    ConflictKind::Extra(extra) => !potentially_activated_extras
                                        .contains(&(item.package(), extra)),
                                };
                                if set_active || fixed_inactive {
                                    stable_reachability.assume_not_conflict_item(item);
                                }
                            }
                        }
                    }
                    stable_reachability.conflict().is_true()
                } else {
                    true
                };
                if let Some(TraversalNode::PendingVariant {
                    stable: node_stable,
                    ..
                }) = &mut inverse[package_index.0]
                {
                    *node_stable = Some(node_stable.unwrap_or(false) || stable);
                }
            }
        }

        // Evaluate conflict markers from concrete roots, not from workspace members that depend on
        // them. Reject markers that still depend on conflict items outside the resulting subgraph.
        if !dependencies_for_conflict_validation.is_empty() {
            let subgraph_packages = inverse
                .iter()
                .enumerate()
                .filter_map(|(package_index, index)| {
                    index.map(|_| &self.lock().packages[package_index].id.name)
                })
                .collect::<FxHashSet<_>>();
            let selection_context_package = selection_context.package();

            // The environment and conflict state are shared by every dependency, so repeated
            // markers have the same result.
            let mut validated_markers = FxHashSet::default();
            for (package, dependency) in dependencies_for_conflict_validation {
                if !validated_markers.insert(dependency.complexified_marker) {
                    continue;
                }
                let mut marker = dependency.complexified_marker;
                for item in self.lock().conflicts().iter().flat_map(ConflictSet::iter) {
                    if selection_context_package != Some(item.package())
                        && !subgraph_packages.contains(item.package())
                    {
                        continue;
                    }

                    if is_active(item) {
                        marker.assume_conflict_item(item);
                    } else {
                        marker.assume_not_conflict_item(item);
                    }
                }

                let conflict = marker.conflict_for_environment(marker_env);
                // All in-subgraph conflict items were resolved above, so a non-constant marker
                // still depends on a package outside the subgraph.
                if !conflict.is_constant() {
                    return Err(LockErrorKind::DependencyConflictOutsideSubgraph {
                        package: package.id.clone(),
                        dependency: dependency.package_id.clone(),
                    }
                    .into());
                }
            }
        }

        Ok(Traversal {
            graph: petgraph,
            inverse,
        })
    }
}

fn node_from_package<'lock>(
    target: &(impl Installable<'lock> + ?Sized),
    package: &Package,
    tag_policy: TagPolicy<'_>,
    marker_env: &ResolverMarkerEnvironment,
    build_options: &BuildOptions,
    install: bool,
) -> Result<Node, LockError> {
    let HashedDist { dist, hashes } = package.to_dist(
        target.install_path(),
        tag_policy,
        build_options,
        marker_env,
        if install && target.lock().is_workspace_member(package) {
            FirstParty::Yes
        } else {
            FirstParty::No
        },
    )?;
    Ok(Node::Dist {
        dist: ResolvedDist::Installable {
            dist: Arc::new(dist),
            version: package.version().cloned(),
        },
        hashes: if install { hashes } else { package.hashes() },
        install,
    })
}

impl<'lock, T> InstallableExt<'lock> for T where T: Installable<'lock> + ?Sized {}

/// An [`Installable`] adapter for materializing concrete packages directly from a [`Lock`].
struct LockedPackages<'lock> {
    lock: &'lock Lock,
    install_path: &'lock Path,
    project_name: Option<&'lock PackageName>,
}

impl<'lock> Installable<'lock> for LockedPackages<'lock> {
    fn install_path(&self) -> &'lock Path {
        self.install_path
    }

    fn lock(&self) -> &'lock Lock {
        self.lock
    }

    fn roots(&self) -> impl Iterator<Item = &PackageName> {
        std::iter::empty()
    }

    fn project_name(&self) -> Option<&PackageName> {
        self.project_name
    }
}

impl Lock {
    /// Materialize a direct dependency selection from this lock.
    ///
    /// Like [`Self::to_resolution`], this materializes the selected dependency's subgraph. It also
    /// preserves the extras activated by the direct edge and the project production or group
    /// context used to select a conflict fork.
    pub fn to_resolution_from_dependency<'lock>(
        &'lock self,
        install_path: &'lock Path,
        dependency: &SelectedDependency<'lock>,
        project_name: Option<&'lock PackageName>,
        marker_env: &ResolverMarkerEnvironment,
        tags: &Tags,
        build_options: &BuildOptions,
        install_options: &InstallOptions,
    ) -> Result<Resolution, LockError> {
        let selected_package = dependency.package();
        let Some(index) = self.by_id.get(&selected_package.id) else {
            return Err(LockErrorKind::RootPackageMissingFromLock {
                id: selected_package.id.clone(),
            }
            .into());
        };
        let Some(package) = self.packages.get(index.0) else {
            return Err(LockErrorKind::RootPackageMissingFromLock {
                id: selected_package.id.clone(),
            }
            .into());
        };
        let extras = ExtrasSpecification::from_extra(dependency.extras().cloned().collect())
            .with_defaults(DefaultExtras::default());
        let groups = DependencyGroupsWithDefaults::none();

        let target = LockedPackages {
            lock: self,
            install_path,
            project_name,
        };
        target
            .traverse_packages(
                &[package],
                None,
                false,
                dependency.context(),
                marker_env,
                &extras,
                &groups,
                tags,
                build_options,
                install_options,
                None,
            )
            .map(|traversal| Resolution::new(traversal.graph))
    }

    /// Materialize the exact dependency subgraph reachable from concrete locked `roots`.
    ///
    /// Each root must be a [`Package`] from this lock. Unlike [`Installable::to_resolution`], this
    /// method does not include requirements or dependency groups attached directly to the lock
    /// manifest. Extras and dependency groups on the concrete roots are still included according
    /// to `extras` and `groups`.
    ///
    /// Conflict-marker evaluation starts from `roots` and their requested `extras` and `groups`,
    /// not from workspace members that depend on those roots. The method returns an error if a
    /// dependency marker still depends on a conflict item outside the resulting subgraph. Use
    /// [`Installable::to_resolution`] when materializing an existing lock target.
    ///
    /// `project_name` identifies the project for project-specific [`InstallOptions`] filters, if
    /// applicable. Callers are responsible for selecting roots that apply to `marker_env`.
    pub fn to_resolution<'lock>(
        &'lock self,
        install_path: &'lock Path,
        roots: impl IntoIterator<Item = &'lock Package>,
        project_name: Option<&'lock PackageName>,
        marker_env: &ResolverMarkerEnvironment,
        tags: &Tags,
        extras: &ExtrasSpecificationWithDefaults,
        groups: &DependencyGroupsWithDefaults,
        build_options: &BuildOptions,
        install_options: &InstallOptions,
    ) -> Result<Resolution, LockError> {
        let mut seen = FxHashSet::default();
        let mut concrete_roots = Vec::new();
        for root in roots {
            let Some(index) = self.by_id.get(&root.id) else {
                return Err(LockErrorKind::RootPackageMissingFromLock {
                    id: root.id.clone(),
                }
                .into());
            };
            if seen.insert(&root.id) {
                let Some(root) = self.packages.get(index.0) else {
                    return Err(LockErrorKind::RootPackageMissingFromLock {
                        id: root.id.clone(),
                    }
                    .into());
                };
                concrete_roots.push(root);
            }
        }

        let target = LockedPackages {
            lock: self,
            install_path,
            project_name,
        };
        target
            .traverse_packages(
                &concrete_roots,
                None,
                false,
                DependencySelectionContext::None,
                marker_env,
                extras,
                groups,
                tags,
                build_options,
                install_options,
                None,
            )
            .map(|traversal| Resolution::new(traversal.graph))
    }
}

fn package_has_variants(package: &Package) -> bool {
    package.variants_json.is_some()
        || package
            .wheels
            .iter()
            .any(|wheel| wheel.filename.variant().is_some())
}

fn variant_base(package: &Package) -> String {
    format!(
        "{}=={}",
        package.name(),
        package
            .version()
            .map(ToString::to_string)
            .unwrap_or_default()
    )
}

#[derive(Default)]
struct VariantSelections {
    selected: FxHashMap<PackageIndex, Node>,
    variants: FxHashMap<String, SelectedVariant>,
}

struct Traversal {
    graph: Graph<Node, Edge>,
    /// The graph node or pending variant selection for each package in the lock.
    inverse: Vec<Option<TraversalNode>>,
}

/// Track whether a package is ready in this pass or waiting for artifact selection.
#[derive(Clone, Copy)]
enum TraversalNode {
    Ready(NodeIndex),
    PendingVariant {
        install: bool,
        /// `None` until a path remains reachable after conflict evaluation.
        /// `Some(true)` means that path also survives additional activated extras.
        stable: Option<bool>,
    },
}

impl TraversalNode {
    fn is_omitted(self, graph: &Graph<Node, Edge>) -> bool {
        match self {
            Self::Ready(index) => matches!(graph[index], Node::Dist { install: false, .. }),
            Self::PendingVariant { install, .. } => !install,
        }
    }
}

/// Inspect packages whose artifact selection is reachable from the selected roots.
fn pending_variants(
    inverse: &[Option<TraversalNode>],
) -> impl Iterator<Item = (PackageIndex, bool, bool)> + Clone {
    inverse
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, state)| match state {
            Some(TraversalNode::PendingVariant {
                install,
                stable: Some(stable),
            }) => Some((PackageIndex(index), install, stable)),
            Some(TraversalNode::Ready(_) | TraversalNode::PendingVariant { stable: None, .. })
            | None => None,
        })
}

struct CurrentVariant<'a> {
    current: Option<&'a SelectedVariant>,
    all: Option<&'a FxHashMap<String, SelectedVariant>>,
}

impl MarkerVariantsEnvironment for CurrentVariant<'_> {
    fn get(&self, base: Option<&str>) -> MarkerVariant<'_> {
        let selected = match base {
            None => self.current,
            Some(base) => self.all.and_then(|all| all.get(base)),
        };
        SelectedVariant::marker_environment(selected)
    }
}

async fn select_variant<Context: BuildContext>(
    package: &Package,
    mut node: Node,
    install_path: &Path,
    tags: &Tags,
    client: &RegistryClient,
    database: &DistributionDatabase<'_, Context>,
    cache: &Cache,
    require_provider_hashes: bool,
) -> Result<Option<(Node, Option<SelectedVariant>)>, LockError> {
    let Node::Dist {
        dist: ResolvedDist::Installable { dist, .. },
        hashes,
        ..
    } = &mut node
    else {
        return Ok(Some((node, None)));
    };
    let Dist::Built(built) = Arc::make_mut(dist) else {
        return Ok(Some((node, None)));
    };
    let variant = match built {
        BuiltDist::Registry(registry) => {
            let selected = select_registry_variant(
                package,
                registry,
                install_path,
                tags,
                client,
                database,
                require_provider_hashes,
            )
            .await?;
            let Some((index, variant)) = selected else {
                let ordinary = registry.best_wheel().filename.variant().is_none();
                return Ok(ordinary.then_some((node, None)));
            };
            registry.best_wheel_index = index;
            *hashes = registry.best_wheel().file.hashes.clone();
            Some(variant)
        }
        BuiltDist::DirectUrl(_) | BuiltDist::Path(_) | BuiltDist::GitPath(_) => {
            let Some(label) = built.wheel_filename().variant() else {
                return Ok(Some((node, None)));
            };
            let expected_size = package
                .wheels
                .iter()
                .find(|wheel| &wheel.filename == built.wheel_filename())
                .and_then(|wheel| wheel.size);
            let metadata =
                read_locked_wheel_variant(client, cache, built, hashes.as_slice(), expected_size)
                    .await
                    .map_err(|error| LockErrorKind::VariantSelection {
                        package_id: package.id.clone(),
                        error,
                    })?;
            let Some(metadata) = metadata else {
                return Ok(None);
            };
            if !metadata.contains_variant(label) {
                return Ok(None);
            }
            let supported = database
                .supported_variant_properties(&metadata, require_provider_hashes)
                .await
                .map_err(|error| LockErrorKind::VariantSelection {
                    package_id: package.id.clone(),
                    error,
                })?;
            let Some(variant) = metadata.select_variant(label, &supported) else {
                return Ok(None);
            };
            Some(variant)
        }
    };
    Ok(Some((node, variant)))
}

async fn select_registry_variant<Context: BuildContext>(
    package: &Package,
    registry: &RegistryBuiltDist,
    install_path: &Path,
    tags: &Tags,
    client: &RegistryClient,
    database: &DistributionDatabase<'_, Context>,
    require_provider_hashes: bool,
) -> Result<Option<(usize, SelectedVariant)>, LockError> {
    let Some(first_candidate) = registry.wheels.iter().position(|wheel| {
        wheel.filename.variant().is_some() && wheel.filename.tag_compatibility(tags).is_compatible()
    }) else {
        return Ok(None);
    };
    let Some(sidecar) = package.to_registry_variants_json(install_path)? else {
        return Ok(None);
    };
    let mut candidates = registry
        .wheels
        .iter()
        .enumerate()
        .skip(first_candidate)
        .filter(|(_, wheel)| wheel.index == sidecar.index)
        .filter_map(|(index, wheel)| {
            let filename = &wheel.filename;
            let label = filename.variant()?;
            let tags = filename.tag_compatibility(tags);
            tags.is_compatible()
                .then_some((index, label, tags, filename.build_tag()))
        })
        .peekable();
    if candidates.peek().is_none() {
        return Ok(None);
    }
    let fetched = client
        .fetch_variants_json(package.name(), &sidecar.file, &sidecar.index)
        .await
        .map_err(|error| LockErrorKind::VariantsJsonFetch {
            package_id: package.id.clone(),
            error,
        })?;
    let metadata = VariantsJsonContent::from_slice(&fetched.contents).map_err(|error| {
        LockErrorKind::VariantsJsonParse {
            package_id: package.id.clone(),
            error,
        }
    })?;
    let mut candidates = candidates
        .filter(|(_, label, ..)| metadata.contains_variant(label))
        .peekable();
    if candidates.peek().is_none() {
        return Ok(None);
    }
    let supported = database
        .supported_variant_properties(&metadata, require_provider_hashes)
        .await
        .map_err(|error| LockErrorKind::VariantSelection {
            package_id: package.id.clone(),
            error,
        })?;
    let selected = candidates
        .filter_map(|(index, label, tags, build_tag)| {
            let score = metadata.score_variant(label, &supported)?;
            Some(((score, tags, build_tag), index, label))
        })
        .max_by(|(left, ..), (right, ..)| left.cmp(right))
        .and_then(|(_, index, label)| {
            metadata
                .select_variant(label, &supported)
                .map(|variant| (index, variant))
        });
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::cmp::Ordering;
    use std::str::FromStr;
    use std::sync::LazyLock;

    use indoc::formatdoc;
    use petgraph::visit::EdgeRef;
    use uv_configuration::{DependencyGroups, ExtrasSpecification};
    use uv_distribution_types::Name;
    use uv_normalize::{DefaultExtras, DefaultGroups};
    use uv_pep508::{MarkerEnvironment, MarkerEnvironmentBuilder};
    use uv_platform_tags::{Arch, Os, Platform, TagsOptions};
    use uv_warnings::anstream;

    use super::*;

    static TAGS: LazyLock<Tags> = LazyLock::new(|| {
        Tags::from_env(
            Platform::new(
                Os::Macos {
                    major: 14,
                    minor: 0,
                },
                Arch::Aarch64,
            ),
            (3, 11),
            "cpython",
            (3, 11),
            TagsOptions::default(),
        )
        .expect("valid tags")
    });

    static DARWIN_MARKERS: LazyLock<ResolverMarkerEnvironment> =
        LazyLock::new(|| ResolverMarkerEnvironment::from(marker_environment("darwin", "Darwin")));

    static LINUX_MARKERS: LazyLock<ResolverMarkerEnvironment> =
        LazyLock::new(|| ResolverMarkerEnvironment::from(marker_environment("linux", "Linux")));

    fn marker_environment(
        sys_platform: &'static str,
        platform_system: &'static str,
    ) -> MarkerEnvironment {
        MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
            implementation_name: "cpython",
            implementation_version: "3.11.5",
            os_name: "posix",
            platform_machine: "arm64",
            platform_python_implementation: "CPython",
            platform_release: "23.0.0",
            platform_system,
            platform_version: "test",
            python_full_version: "3.11.5",
            python_version: "3.11",
            sys_platform,
        })
        .expect("valid marker environment")
    }

    fn lock() -> Lock {
        toml::from_str(
            r#"
version = 1
revision = 3
requires-python = ">=3.11"
resolution-markers = [
    "sys_platform == 'darwin'",
    "sys_platform != 'darwin'",
]

[manifest]
requirements = [{ name = "unrelated" }]

[[package]]
name = "dev-dependency"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
sdist = { url = "https://example.com/dev_dependency-1.0.0.tar.gz", hash = "sha256:1111111111111111111111111111111111111111111111111111111111111111" }

[[package]]
name = "forked"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
resolution-markers = ["sys_platform == 'darwin'"]
sdist = { url = "https://example.com/forked-1.0.0.tar.gz", hash = "sha256:2222222222222222222222222222222222222222222222222222222222222222" }

[[package]]
name = "forked"
version = "2.0.0"
source = { registry = "https://example.com/simple" }
resolution-markers = ["sys_platform != 'darwin'"]
sdist = { url = "https://example.com/forked-2.0.0.tar.gz", hash = "sha256:3333333333333333333333333333333333333333333333333333333333333333" }

[[package]]
name = "optional-dependency"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
sdist = { url = "https://example.com/optional_dependency-1.0.0.tar.gz", hash = "sha256:4444444444444444444444444444444444444444444444444444444444444444" }

[[package]]
name = "root-a"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
dependencies = [
    { name = "forked", version = "1.0.0", source = { registry = "https://example.com/simple" }, marker = "sys_platform == 'darwin'" },
    { name = "forked", version = "2.0.0", source = { registry = "https://example.com/simple" }, marker = "sys_platform != 'darwin'" },
    { name = "shared" },
]
sdist = { url = "https://example.com/root_a-1.0.0.tar.gz", hash = "sha256:5555555555555555555555555555555555555555555555555555555555555555" }

[package.optional-dependencies]
feature = [{ name = "optional-dependency" }]

[package.dependency-groups]
dev = [{ name = "dev-dependency" }]

[package.metadata]
provides-extras = ["feature"]

[[package]]
name = "root-b"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
dependencies = [{ name = "shared" }]
sdist = { url = "https://example.com/root_b-1.0.0.tar.gz", hash = "sha256:6666666666666666666666666666666666666666666666666666666666666666" }

[[package]]
name = "shared"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
sdist = { url = "https://example.com/shared-1.0.0.tar.gz", hash = "sha256:7777777777777777777777777777777777777777777777777777777777777777" }

[[package]]
name = "unrelated"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
sdist = { url = "https://example.com/unrelated-1.0.0.tar.gz", hash = "sha256:8888888888888888888888888888888888888888888888888888888888888888" }
"#,
        )
        .expect("valid lock")
    }

    fn conflict_lock() -> Lock {
        toml::from_str(
            r#"
version = 1
revision = 3
requires-python = ">=3.11"
conflicts = [
    [
        { package = "tool", extra = "cpu" },
        { package = "tool", extra = "gpu" },
    ],
    [
        { package = "project", extra = "foo" },
        { package = "project", extra = "bar" },
    ],
]

[[package]]
name = "contextual-dependency"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
sdist = { url = "https://example.com/contextual_dependency-1.0.0.tar.gz", hash = "sha256:1111111111111111111111111111111111111111111111111111111111111111" }

[[package]]
name = "contextual-tool"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
dependencies = [
    { name = "contextual-dependency", marker = "sys_platform == 'linux' or (sys_platform == 'darwin' and extra == 'extra-7-project-foo')" },
]
sdist = { url = "https://example.com/contextual_tool-1.0.0.tar.gz", hash = "sha256:2222222222222222222222222222222222222222222222222222222222222222" }

[[package]]
name = "cpu-backend"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
sdist = { url = "https://example.com/cpu_backend-1.0.0.tar.gz", hash = "sha256:3333333333333333333333333333333333333333333333333333333333333333" }

[[package]]
name = "gpu-backend"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
sdist = { url = "https://example.com/gpu_backend-1.0.0.tar.gz", hash = "sha256:4444444444444444444444444444444444444444444444444444444444444444" }

[[package]]
name = "project"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
sdist = { url = "https://example.com/project-1.0.0.tar.gz", hash = "sha256:5555555555555555555555555555555555555555555555555555555555555555" }

[package.optional-dependencies]
foo = []
bar = []

[package.metadata]
provides-extras = ["foo", "bar"]

[[package]]
name = "runtime"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
dependencies = [
    { name = "cpu-backend", marker = "extra == 'extra-4-tool-cpu'" },
    { name = "gpu-backend", marker = "extra == 'extra-4-tool-gpu'" },
]
sdist = { url = "https://example.com/runtime-1.0.0.tar.gz", hash = "sha256:6666666666666666666666666666666666666666666666666666666666666666" }

[[package]]
name = "tool"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
dependencies = [{ name = "runtime" }]
sdist = { url = "https://example.com/tool-1.0.0.tar.gz", hash = "sha256:7777777777777777777777777777777777777777777777777777777777777777" }

[package.optional-dependencies]
cpu = []
gpu = []

[package.metadata]
provides-extras = ["cpu", "gpu"]
"#,
        )
        .expect("valid lock")
    }

    fn dependency_selection_lock() -> Lock {
        toml::from_str(
            r#"
version = 1
revision = 3
requires-python = ">=3.11"
conflicts = [[
    { package = "project", group = "dev" },
    { package = "project", group = "other" },
]]

[[package]]
name = "contextual-dev-dependency"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
sdist = { url = "https://example.com/contextual_dev_dependency-1.0.0.tar.gz", hash = "sha256:1111111111111111111111111111111111111111111111111111111111111111" }

[[package]]
name = "contextual-other-dependency"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
sdist = { url = "https://example.com/contextual_other_dependency-1.0.0.tar.gz", hash = "sha256:2222222222222222222222222222222222222222222222222222222222222222" }

[[package]]
name = "optional-dependency"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
sdist = { url = "https://example.com/optional_dependency-1.0.0.tar.gz", hash = "sha256:3333333333333333333333333333333333333333333333333333333333333333" }

[[package]]
name = "project"
version = "1.0.0"
source = { virtual = "." }

[package.dependency-groups]
dev = [{ name = "tool", extra = ["cli"] }]
other = [{ name = "tool" }]

[[package]]
name = "tool"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
dependencies = [
    { name = "contextual-dev-dependency", marker = "extra == 'group-7-project-dev'" },
    { name = "contextual-other-dependency", marker = "extra == 'group-7-project-other'" },
]
sdist = { url = "https://example.com/tool-1.0.0.tar.gz", hash = "sha256:4444444444444444444444444444444444444444444444444444444444444444" }

[package.optional-dependencies]
cli = [{ name = "optional-dependency" }]

[package.metadata]
provides-extras = ["cli"]
"#,
        )
        .expect("valid lock")
    }

    fn package<'lock>(lock: &'lock Lock, name: &str, version: &str) -> &'lock Package {
        lock.packages()
            .iter()
            .find(|package| {
                package.name().as_ref() == name
                    && package
                        .version()
                        .is_some_and(|package_version| package_version.to_string() == version)
            })
            .expect("locked package")
    }

    fn materialize(
        lock: &Lock,
        roots: &[&Package],
        marker_env: &ResolverMarkerEnvironment,
    ) -> Resolution {
        let extras = ExtrasSpecification::from_all_extras().with_defaults(DefaultExtras::default());
        let groups = DependencyGroups::from_all_groups().with_defaults(DefaultGroups::default());
        lock.to_resolution(
            Path::new("."),
            roots.iter().copied(),
            None,
            marker_env,
            &TAGS,
            &extras,
            &groups,
            &BuildOptions::default(),
            &InstallOptions::default(),
        )
        .expect("valid resolution")
    }

    fn materialize_with_extras(
        lock: &Lock,
        roots: &[&Package],
        marker_env: &ResolverMarkerEnvironment,
        extras: &ExtrasSpecification,
    ) -> Result<Resolution, LockError> {
        let extras = extras.with_defaults(DefaultExtras::default());
        let groups = DependencyGroupsWithDefaults::none();
        lock.to_resolution(
            Path::new("."),
            roots.iter().copied(),
            None,
            marker_env,
            &TAGS,
            &extras,
            &groups,
            &BuildOptions::default(),
            &InstallOptions::default(),
        )
    }

    fn materialize_selected_dependency(lock: &Lock, group: &str) -> Resolution {
        let project_name = PackageName::from_str("project").expect("valid package name");
        let dependency_name = PackageName::from_str("tool").expect("valid package name");
        let group = GroupName::from_str(group).expect("valid group name");
        let selection = lock
            .dependency_selection(
                Some(&project_name),
                &dependency_name,
                DARWIN_MARKERS.markers(),
            )
            .expect("unique dependency selection");
        let dependency = selection.group(&group).expect("group dependency");

        lock.to_resolution_from_dependency(
            Path::new("."),
            dependency,
            Some(&project_name),
            &DARWIN_MARKERS,
            &TAGS,
            &BuildOptions::default(),
            &InstallOptions::default(),
        )
        .expect("valid resolution")
    }

    #[test]
    fn unrelated_packages_do_not_change_dependency_identity() {
        let original = lock();
        let original_toml = original.to_toml().expect("valid lock TOML");
        let input = formatdoc! {r#"
            {original_toml}

            [[package]]
            name = "aaa-unrelated"
            version = "1.0.0"
            source = {{ registry = "https://example.com/simple" }}
        "#};
        let extended = Lock::from_toml(&input).expect("valid extended lock");
        let original_root = package(&original, "root-a", "1.0.0");
        let extended_root = package(&extended, "root-a", "1.0.0");

        assert_eq!(original_root, extended_root);
        assert_eq!(
            original_root.dependencies.cmp(&extended_root.dependencies),
            Ordering::Equal,
        );
        assert_eq!(
            graph_snapshot(&materialize(&original, &[original_root], &DARWIN_MARKERS)),
            graph_snapshot(&materialize(&extended, &[extended_root], &DARWIN_MARKERS)),
        );
    }

    struct OverridingInstallable<'lock> {
        lock: &'lock Lock,
        root_name: &'lock PackageName,
        package_to_node_calls: Cell<usize>,
    }

    impl<'lock> Installable<'lock> for OverridingInstallable<'lock> {
        fn install_path(&self) -> &'lock Path {
            Path::new(".")
        }

        fn lock(&self) -> &'lock Lock {
            self.lock
        }

        fn roots(&self) -> impl Iterator<Item = &PackageName> {
            std::iter::once(self.root_name)
        }

        fn project_name(&self) -> Option<&PackageName> {
            None
        }

        fn package_to_node(
            &self,
            _package: &Package,
            _tags: &Tags,
            _build_options: &BuildOptions,
            _install_options: &InstallOptions,
            _marker_env: &ResolverMarkerEnvironment,
        ) -> Result<Node, LockError> {
            self.package_to_node_calls
                .set(self.package_to_node_calls.get() + 1);
            Ok(Node::Root)
        }
    }

    fn graph_snapshot(resolution: &Resolution) -> (Vec<String>, Vec<String>) {
        let graph = resolution.graph();
        let labels = graph
            .node_weights()
            .map(|node| match node {
                Node::Root => "root".to_string(),
                Node::Dist {
                    dist,
                    hashes,
                    install,
                } => format!(
                    "{}=={} (install: {install}, hashes: {})",
                    dist.name(),
                    dist.version()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "<dynamic>".to_string()),
                    hashes.iter().map(ToString::to_string).join(", ")
                ),
            })
            .collect::<Vec<_>>();
        let mut nodes = labels.clone();
        nodes.sort_unstable();
        let mut edges = graph
            .edge_references()
            .map(|edge| {
                format!(
                    "{} --{:?}--> {}",
                    labels[edge.source().index()],
                    edge.weight(),
                    labels[edge.target().index()]
                )
            })
            .collect::<Vec<_>>();
        edges.sort_unstable();
        (nodes, edges)
    }

    #[test]
    fn materializes_multiple_concrete_roots_with_shared_dependencies() {
        let lock = lock();
        let resolution = materialize(
            &lock,
            &[
                package(&lock, "root-a", "1.0.0"),
                package(&lock, "root-b", "1.0.0"),
            ],
            &DARWIN_MARKERS,
        );

        insta::with_settings!({
            filters => [(r"sha256:[0-9a-f]{64}", "sha256:[HASH]")],
        }, {
            insta::assert_debug_snapshot!(graph_snapshot(&resolution), @r#"
        (
            [
                "dev-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "forked==1.0.0 (install: true, hashes: sha256:[HASH])",
                "optional-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root",
                "root-a==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root-b==1.0.0 (install: true, hashes: sha256:[HASH])",
                "shared==1.0.0 (install: true, hashes: sha256:[HASH])",
            ],
            [
                "root --Prod--> root-a==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root --Prod--> root-b==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root-a==1.0.0 (install: true, hashes: sha256:[HASH]) --Dev(GroupName(\"dev\"))--> dev-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root-a==1.0.0 (install: true, hashes: sha256:[HASH]) --Optional(ExtraName(\"feature\"))--> optional-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root-a==1.0.0 (install: true, hashes: sha256:[HASH]) --Prod--> forked==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root-a==1.0.0 (install: true, hashes: sha256:[HASH]) --Prod--> shared==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root-b==1.0.0 (install: true, hashes: sha256:[HASH]) --Prod--> shared==1.0.0 (install: true, hashes: sha256:[HASH])",
            ],
        )
            "#);
        });
    }

    #[test]
    fn materializes_group_root_referenced_by_production_dependency() {
        let lock = lock();
        let project = package(&lock, "root-a", "1.0.0");
        let group_root = package(&lock, "shared", "1.0.0");
        let extras = ExtrasSpecification::default().with_defaults(DefaultExtras::default());
        let groups = DependencyGroupsWithDefaults::none();

        let target = LockedPackages {
            lock: &lock,
            install_path: Path::new("."),
            project_name: Some(project.name()),
        };
        let resolution = target
            .traverse_packages(
                &[project],
                Some(group_root),
                false,
                DependencySelectionContext::None,
                &DARWIN_MARKERS,
                &extras,
                &groups,
                &TAGS,
                &BuildOptions::default(),
                &InstallOptions::default(),
                None,
            )
            .map(|traversal| Resolution::new(traversal.graph))
            .expect("valid resolution");

        assert!(
            resolution
                .distributions()
                .any(|distribution| distribution.name() == group_root.name())
        );
    }

    #[test]
    fn materializes_the_selected_universal_lock_fork() {
        let lock = lock();
        let root = package(&lock, "root-a", "1.0.0");
        let darwin = materialize(&lock, &[root], &DARWIN_MARKERS);
        let linux = materialize(&lock, &[root], &LINUX_MARKERS);
        let concrete_fork =
            materialize(&lock, &[package(&lock, "forked", "1.0.0")], &DARWIN_MARKERS);

        insta::with_settings!({
            filters => [(r"sha256:[0-9a-f]{64}", "sha256:[HASH]")],
        }, {
            insta::assert_debug_snapshot!(graph_snapshot(&darwin), @r#"
        (
            [
                "dev-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "forked==1.0.0 (install: true, hashes: sha256:[HASH])",
                "optional-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root",
                "root-a==1.0.0 (install: true, hashes: sha256:[HASH])",
                "shared==1.0.0 (install: true, hashes: sha256:[HASH])",
            ],
            [
                "root --Prod--> root-a==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root-a==1.0.0 (install: true, hashes: sha256:[HASH]) --Dev(GroupName(\"dev\"))--> dev-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root-a==1.0.0 (install: true, hashes: sha256:[HASH]) --Optional(ExtraName(\"feature\"))--> optional-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root-a==1.0.0 (install: true, hashes: sha256:[HASH]) --Prod--> forked==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root-a==1.0.0 (install: true, hashes: sha256:[HASH]) --Prod--> shared==1.0.0 (install: true, hashes: sha256:[HASH])",
            ],
        )
        "#);
            insta::assert_debug_snapshot!(graph_snapshot(&linux), @r#"
        (
            [
                "dev-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "forked==2.0.0 (install: true, hashes: sha256:[HASH])",
                "optional-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root",
                "root-a==1.0.0 (install: true, hashes: sha256:[HASH])",
                "shared==1.0.0 (install: true, hashes: sha256:[HASH])",
            ],
            [
                "root --Prod--> root-a==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root-a==1.0.0 (install: true, hashes: sha256:[HASH]) --Dev(GroupName(\"dev\"))--> dev-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root-a==1.0.0 (install: true, hashes: sha256:[HASH]) --Optional(ExtraName(\"feature\"))--> optional-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root-a==1.0.0 (install: true, hashes: sha256:[HASH]) --Prod--> forked==2.0.0 (install: true, hashes: sha256:[HASH])",
                "root-a==1.0.0 (install: true, hashes: sha256:[HASH]) --Prod--> shared==1.0.0 (install: true, hashes: sha256:[HASH])",
            ],
        )
        "#);
            insta::assert_debug_snapshot!(graph_snapshot(&concrete_fork), @r#"
        (
            [
                "forked==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root",
            ],
            [
                "root --Prod--> forked==1.0.0 (install: true, hashes: sha256:[HASH])",
            ],
        )
            "#);
        });
    }

    #[test]
    fn materializes_conflicting_extras_within_the_synthetic_root() {
        let lock = conflict_lock();
        let extras =
            ExtrasSpecification::from_extra(vec!["cpu".parse().expect("valid extra name")]);
        let resolution = materialize_with_extras(
            &lock,
            &[package(&lock, "tool", "1.0.0")],
            &DARWIN_MARKERS,
            &extras,
        )
        .expect("conflict markers are resolved within the subgraph");

        insta::with_settings!({
            filters => [(r"sha256:[0-9a-f]{64}", "sha256:[HASH]")],
        }, {
            insta::assert_debug_snapshot!(graph_snapshot(&resolution), @r#"
        (
            [
                "cpu-backend==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root",
                "runtime==1.0.0 (install: true, hashes: sha256:[HASH])",
                "tool==1.0.0 (install: true, hashes: sha256:[HASH])",
            ],
            [
                "root --Prod--> tool==1.0.0 (install: true, hashes: sha256:[HASH])",
                "runtime==1.0.0 (install: true, hashes: sha256:[HASH]) --Prod--> cpu-backend==1.0.0 (install: true, hashes: sha256:[HASH])",
                "tool==1.0.0 (install: true, hashes: sha256:[HASH]) --Prod--> runtime==1.0.0 (install: true, hashes: sha256:[HASH])",
            ],
        )
        "#);
        });
    }

    /// An unselected extra and a dependency for another platform can contain activators for a
    /// conflicting extra. Their presence must not make a reachable variant branch appear unstable.
    #[test]
    fn schedules_only_reachable_variants_with_unselected_activators() {
        let lock = Lock::from_toml(
            r#"
version = 1
revision = 3
requires-python = ">=3.11"
conflicts = [[
    { package = "project", extra = "cpu" },
    { package = "project", extra = "gpu" },
]]

[[package]]
name = "excluded"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
[[package.wheels]]
url = "https://example.com/excluded-1.0.0-py3-none-any-null.whl"
hash = "sha256:1111111111111111111111111111111111111111111111111111111111111111"

[[package]]
name = "platform-excluded"
version = "1.0.0"
source = { virtual = "platform-excluded" }
dependencies = [{ name = "project", extra = ["gpu"] }]

[[package]]
name = "project"
version = "1.0.0"
source = { virtual = "." }
dependencies = [
    { name = "excluded", marker = "extra == 'extra-7-project-gpu'" },
    { name = "platform-excluded", marker = "sys_platform == 'linux'" },
    { name = "selected", marker = "extra != 'extra-7-project-gpu'" },
]

[package.optional-dependencies]
cpu = []
gpu = []
unselected = [{ name = "project", extra = ["gpu"] }]

[package.metadata]
provides-extras = ["cpu", "gpu", "unselected"]

[[package]]
name = "selected"
version = "1.0.0"
source = { registry = "https://example.com/simple" }
[[package.wheels]]
url = "https://example.com/selected-1.0.0-py3-none-any-null.whl"
hash = "sha256:2222222222222222222222222222222222222222222222222222222222222222"
"#,
        )
        .expect("valid lock");
        let root = package(&lock, "project", "1.0.0");
        let target = LockedPackages {
            lock: &lock,
            install_path: Path::new(env!("CARGO_MANIFEST_DIR")),
            project_name: Some(root.name()),
        };
        let build_options = BuildOptions::default();
        let install_options = InstallOptions::default();
        let selections = VariantSelections::default();
        let extras = ExtrasSpecification::default().with_defaults(DefaultExtras::default());
        let traversal = target
            .traverse_packages(
                &[root],
                None,
                true,
                DependencySelectionContext::None,
                &DARWIN_MARKERS,
                &extras,
                &DependencyGroupsWithDefaults::none(),
                &TAGS,
                &build_options,
                &install_options,
                Some(&selections),
            )
            .expect("valid traversal");

        let pending = pending_variants(&traversal.inverse).collect::<Vec<_>>();
        let selected = package(&lock, "selected", "1.0.0");
        assert_eq!(pending, [(lock.by_id[&selected.id], true, true)]);
    }

    #[test]
    fn materializes_selected_dependency_extras() {
        let resolution = materialize_selected_dependency(&dependency_selection_lock(), "dev");

        insta::with_settings!({
            filters => [(r"sha256:[0-9a-f]{64}", "sha256:[HASH]")],
        }, {
            insta::assert_debug_snapshot!(graph_snapshot(&resolution), @r#"
        (
            [
                "contextual-dev-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "optional-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root",
                "tool==1.0.0 (install: true, hashes: sha256:[HASH])",
            ],
            [
                "root --Prod--> tool==1.0.0 (install: true, hashes: sha256:[HASH])",
                "tool==1.0.0 (install: true, hashes: sha256:[HASH]) --Optional(ExtraName(\"cli\"))--> optional-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "tool==1.0.0 (install: true, hashes: sha256:[HASH]) --Prod--> contextual-dev-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
            ],
        )
        "#);
        });
    }

    #[test]
    fn materializes_selected_dependency_project_conflict_context() {
        let resolution = materialize_selected_dependency(&dependency_selection_lock(), "other");

        insta::with_settings!({
            filters => [(r"sha256:[0-9a-f]{64}", "sha256:[HASH]")],
        }, {
            insta::assert_debug_snapshot!(graph_snapshot(&resolution), @r#"
        (
            [
                "contextual-other-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root",
                "tool==1.0.0 (install: true, hashes: sha256:[HASH])",
            ],
            [
                "root --Prod--> tool==1.0.0 (install: true, hashes: sha256:[HASH])",
                "tool==1.0.0 (install: true, hashes: sha256:[HASH]) --Prod--> contextual-other-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
            ],
        )
        "#);
        });
    }

    #[test]
    fn rejects_conflicts_outside_the_synthetic_root() {
        let lock = conflict_lock();
        let root = package(&lock, "contextual-tool", "1.0.0");
        let extras = ExtrasSpecification::default();

        let error = materialize_with_extras(&lock, &[root], &DARWIN_MARKERS, &extras)
            .expect_err("Darwin dependency depends on the project extra");
        let error = error.to_string();
        let error = anstream::adapter::strip_str(&error);
        insta::assert_snapshot!(error, @"Cannot materialize dependency `contextual-dependency==1.0.0 @ registry+https://example.com/simple` of `contextual-tool==1.0.0 @ registry+https://example.com/simple` because its conflict marker depends on a package outside the selected subgraph");

        let linux = materialize_with_extras(&lock, &[root], &LINUX_MARKERS, &extras)
            .expect("the dependency is unconditional on Linux");
        insta::with_settings!({
            filters => [(r"sha256:[0-9a-f]{64}", "sha256:[HASH]")],
        }, {
            insta::assert_debug_snapshot!(graph_snapshot(&linux), @r#"
        (
            [
                "contextual-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "contextual-tool==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root",
            ],
            [
                "contextual-tool==1.0.0 (install: true, hashes: sha256:[HASH]) --Prod--> contextual-dependency==1.0.0 (install: true, hashes: sha256:[HASH])",
                "root --Prod--> contextual-tool==1.0.0 (install: true, hashes: sha256:[HASH])",
            ],
        )
        "#);
        });
    }

    #[test]
    fn installable_to_resolution_preserves_node_overrides() {
        let mut lock = lock();
        lock.manifest.requirements.clear();
        let target = OverridingInstallable {
            root_name: package(&lock, "root-a", "1.0.0").name(),
            lock: &lock,
            package_to_node_calls: Cell::new(0),
        };
        let extras = ExtrasSpecification::from_all_extras().with_defaults(DefaultExtras::default());
        let groups = DependencyGroups::from_all_groups().with_defaults(DefaultGroups::default());

        target
            .to_resolution(
                &DARWIN_MARKERS,
                &TAGS,
                &extras,
                &groups,
                &BuildOptions::default(),
                &InstallOptions::default(),
            )
            .expect("valid resolution");

        assert!(target.package_to_node_calls.get() > 0);
    }
}

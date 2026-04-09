use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::collections::hash_map::Entry;
use std::path::Path;
use std::sync::Arc;

use either::Either;
use itertools::Itertools;
use petgraph::Graph;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use uv_configuration::ExtrasSpecificationWithDefaults;
use uv_configuration::{BuildOptions, DependencyGroupsWithDefaults, InstallOptions};
use uv_distribution_types::{Edge, Node, Resolution, ResolvedDist};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_platform_tags::Tags;
use uv_pypi_types::{ConflictKind, ConflictSet, ResolverMarkerEnvironment};

use crate::lock::{Dependency, HashedDist, LockErrorKind, Package, PackageId, TagPolicy};
use crate::{Lock, LockError, UniversalMarker};

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
    reachability: &mut FxHashMap<(&'lock PackageId, Option<&'lock ExtraName>), UniversalMarker>,
    key: (&'lock PackageId, Option<&'lock ExtraName>),
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

pub trait Installable<'lock> {
    /// Return the root install path.
    fn install_path(&self) -> &'lock Path;

    /// Return the [`Lock`] to install.
    fn lock(&self) -> &'lock Lock;

    /// Return the [`PackageName`] of the root packages in the target.
    fn roots(&self) -> impl Iterator<Item = &PackageName>;

    /// Return the [`PackageName`] of the target, if available.
    fn project_name(&self) -> Option<&PackageName>;

    /// Convert the [`Lock`] to a [`Resolution`] using the given marker environment, tags, and
    /// root, without any extras, dependency groups, or install options.
    fn to_resolution_simple(
        &self,
        marker_env: &ResolverMarkerEnvironment,
        tags: &Tags,
        build_options: &BuildOptions,
    ) -> Result<Resolution, LockError> {
        self.to_resolution(
            marker_env,
            tags,
            &ExtrasSpecificationWithDefaults::none(),
            &DependencyGroupsWithDefaults::none(),
            build_options,
            &InstallOptions::default(),
        )
    }

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
        let roots = self
            .roots()
            .map(|root_name| {
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
            })
            .collect::<Result<Vec<_>, LockError>>()?;

        InstallableExt::to_resolution_from_packages(
            self,
            &roots,
            true,
            marker_env,
            tags,
            extras,
            groups,
            build_options,
            install_options,
        )
    }

    /// Create an installable [`Node`] from a [`Package`].
    fn installable_node(
        &self,
        package: &Package,
        tags: &Tags,
        marker_env: &ResolverMarkerEnvironment,
        build_options: &BuildOptions,
    ) -> Result<Node, LockError> {
        let tag_policy = TagPolicy::Required(tags);
        let HashedDist { dist, hashes } =
            package.to_dist(self.install_path(), tag_policy, build_options, marker_env)?;
        let version = package.version().cloned();
        let dist = ResolvedDist::Installable {
            dist: Arc::new(dist),
            version,
        };
        Ok(Node::Dist {
            dist,
            hashes,
            install: true,
        })
    }

    /// Create a non-installable [`Node`] from a [`Package`].
    fn non_installable_node(
        &self,
        package: &Package,
        tags: &Tags,
        marker_env: &ResolverMarkerEnvironment,
    ) -> Result<Node, LockError> {
        let HashedDist { dist, .. } = package.to_dist(
            self.install_path(),
            TagPolicy::Preferred(tags),
            &BuildOptions::default(),
            marker_env,
        )?;
        let version = package.version().cloned();
        let dist = ResolvedDist::Installable {
            dist: Arc::new(dist),
            version,
        };
        let hashes = package.hashes();
        Ok(Node::Dist {
            dist,
            hashes,
            install: false,
        })
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
    /// Convert concrete locked packages to a [`Resolution`].
    ///
    /// `include_manifest` controls whether requirements attached directly to the lock target are
    /// included in addition to `roots`.
    fn to_resolution_from_packages(
        &self,
        roots: &[&Package],
        include_manifest: bool,
        marker_env: &ResolverMarkerEnvironment,
        tags: &Tags,
        extras: &ExtrasSpecificationWithDefaults,
        groups: &DependencyGroupsWithDefaults,
        build_options: &BuildOptions,
        install_options: &InstallOptions,
    ) -> Result<Resolution, LockError> {
        let size_guess = self.lock().packages.len();
        let mut petgraph = Graph::with_capacity(size_guess, size_guess);
        let mut inverse = FxHashMap::with_capacity_and_hasher(size_guess, FxBuildHasher);

        let mut queue: VecDeque<(&Package, Option<&ExtraName>)> = VecDeque::new();
        let mut seen = FxHashSet::default();
        let mut conflict_reachability = FxHashMap::default();
        let mut activated_projects: Vec<&PackageName> = vec![];
        let mut activated_extras: Vec<(&PackageName, &ExtraName)> = vec![];
        let mut activated_groups: Vec<(&PackageName, &GroupName)> = vec![];
        let has_conflicts = !self.lock().conflicts().is_empty();
        let validate_conflicts = !include_manifest && has_conflicts;
        let mut dependencies_for_conflict_validation = vec![];

        let root = petgraph.add_node(Node::Root);

        // Determine the set of activated extras and groups, from the root.
        //
        // TODO(charlie): This isn't quite right. Below, when we add the dependency groups to the
        // graph, we rely on the activated extras and dependency groups, to evaluate the conflict
        // marker. But at that point, we don't know the full set of activated extras; this is only
        // computed below. We somehow need to add the dependency groups _after_ we've computed all
        // enabled extras, but the groups themselves could depend on the set of enabled extras.
        if has_conflicts {
            for dist in roots.iter().copied() {
                // Track the activated extras.
                if groups.prod() {
                    activated_projects.push(&dist.id.name);
                    for extra in extras.extra_names(dist.optional_dependencies.keys()) {
                        activated_extras.push((&dist.id.name, extra));
                    }
                }

                // Track the activated groups.
                for group in dist
                    .dependency_groups
                    .keys()
                    .filter(|group| groups.contains(group))
                {
                    activated_groups.push((&dist.id.name, group));
                }
            }
        }

        // Initialize the workspace roots.
        let mut initialized_roots = vec![];
        for dist in roots.iter().copied() {
            // Add the workspace package to the graph.
            let index = petgraph.add_node(if groups.prod() {
                self.package_to_node(dist, tags, build_options, install_options, marker_env)?
            } else {
                self.non_installable_node(dist, tags, marker_env)?
            });
            inverse.insert(&dist.id, index);

            // Add an edge from the root.
            petgraph.add_edge(root, index, Edge::Prod);

            // Push the package onto the queue.
            initialized_roots.push((dist, index));
        }

        // Add the workspace dependencies to the queue.
        for (dist, index) in initialized_roots {
            if groups.prod() {
                // Push its dependencies onto the queue.
                queue.push_back((dist, None));
                add_reachability(
                    &mut conflict_reachability,
                    (&dist.id, None),
                    UniversalMarker::TRUE,
                );
                for extra in extras.extra_names(dist.optional_dependencies.keys()) {
                    queue.push_back((dist, Some(extra)));
                    add_reachability(
                        &mut conflict_reachability,
                        (&dist.id, Some(extra)),
                        UniversalMarker::TRUE,
                    );
                }
            }

            // Add any dev dependencies.
            for (group, dep) in dist
                .dependency_groups
                .iter()
                .filter_map(|(group, deps)| {
                    if groups.contains(group) {
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
                if !dep.complexified_marker.evaluate(
                    marker_env,
                    activated_projects.iter().copied(),
                    activated_extras
                        .iter()
                        .chain(additional_activated_extras.iter())
                        .copied(),
                    activated_groups.iter().copied(),
                ) {
                    continue;
                }

                let dep_dist = self.lock().find_by_id(&dep.package_id);

                // Add the package to the graph.
                let dep_index = match inverse.entry(&dep.package_id) {
                    Entry::Vacant(entry) => {
                        let index = petgraph.add_node(self.package_to_node(
                            dep_dist,
                            tags,
                            build_options,
                            install_options,
                            marker_env,
                        )?);
                        entry.insert(index);
                        index
                    }
                    Entry::Occupied(entry) => {
                        // Critically, if the package is already in the graph, then it's a workspace
                        // member. If it was omitted due to, e.g., `--only-dev`, but is itself
                        // referenced as a development dependency, then we need to re-enable it.
                        let index = *entry.get();
                        let node = &mut petgraph[index];
                        if !groups.prod() {
                            *node = self.package_to_node(
                                dep_dist,
                                tags,
                                build_options,
                                install_options,
                                marker_env,
                            )?;
                        }
                        index
                    }
                };

                petgraph.add_edge(
                    index,
                    dep_index,
                    // This is OK because we are resolving to a resolution for
                    // a specific marker environment and set of extras/groups.
                    // So at this point, we know the extras/groups have been
                    // satisfied, so we can safely drop the conflict marker.
                    Edge::Dev(group.clone()),
                );

                // Push its dependencies on the queue.
                add_reachability(
                    &mut conflict_reachability,
                    (&dep.package_id, None),
                    dep.complexified_marker,
                );
                if seen.insert((&dep.package_id, None)) {
                    queue.push_back((dep_dist, None));
                }
                for extra in &dep.extra {
                    add_reachability(
                        &mut conflict_reachability,
                        (&dep.package_id, Some(extra)),
                        dep.complexified_marker,
                    );
                    if seen.insert((&dep.package_id, Some(extra))) {
                        queue.push_back((dep_dist, Some(extra)));
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
                let index = petgraph.add_node(if groups.prod() {
                    self.package_to_node(dist, tags, build_options, install_options, marker_env)?
                } else {
                    self.non_installable_node(dist, tags, marker_env)?
                });
                inverse.insert(&dist.id, index);

                // Add the edge.
                petgraph.add_edge(root, index, Edge::Prod);

                // Push its dependencies on the queue.
                add_reachability(
                    &mut conflict_reachability,
                    (&dist.id, None),
                    UniversalMarker::TRUE,
                );
                if seen.insert((&dist.id, None)) {
                    queue.push_back((dist, None));
                }
                for extra in &dependency.extras {
                    add_reachability(
                        &mut conflict_reachability,
                        (&dist.id, Some(extra)),
                        UniversalMarker::TRUE,
                    );
                    if seen.insert((&dist.id, Some(extra))) {
                        queue.push_back((dist, Some(extra)));
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
                    if groups.contains(group) {
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
                let index = match inverse.entry(&dist.id) {
                    Entry::Vacant(entry) => {
                        let index = petgraph.add_node(self.package_to_node(
                            dist,
                            tags,
                            build_options,
                            install_options,
                            marker_env,
                        )?);
                        entry.insert(index);
                        index
                    }
                    Entry::Occupied(entry) => {
                        // Critically, if the package is already in the graph, then it's a workspace
                        // member. If it was omitted due to, e.g., `--only-dev`, but is itself
                        // referenced as a development dependency, then we need to re-enable it.
                        let index = *entry.get();
                        let node = &mut petgraph[index];
                        if !groups.prod() {
                            *node = self.package_to_node(
                                dist,
                                tags,
                                build_options,
                                install_options,
                                marker_env,
                            )?;
                        }
                        index
                    }
                };

                // Add the edge.
                petgraph.add_edge(root, index, Edge::Dev(group.clone()));

                // Push its dependencies on the queue.
                add_reachability(
                    &mut conflict_reachability,
                    (&dist.id, None),
                    UniversalMarker::TRUE,
                );
                if seen.insert((&dist.id, None)) {
                    queue.push_back((dist, None));
                }
                for extra in &dependency.extras {
                    add_reachability(
                        &mut conflict_reachability,
                        (&dist.id, Some(extra)),
                        UniversalMarker::TRUE,
                    );
                    if seen.insert((&dist.id, Some(extra))) {
                        queue.push_back((dist, Some(extra)));
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
            let mut reachability = conflict_reachability;
            while let Some((package, extra)) = queue.pop_front() {
                let Some(parent_reachability) = reachability.get(&(&package.id, extra)).copied()
                else {
                    continue;
                };
                let deps = if let Some(extra) = extra {
                    Either::Left(
                        package
                            .optional_dependencies
                            .get(extra)
                            .into_iter()
                            .flatten(),
                    )
                } else {
                    Either::Right(package.dependencies.iter())
                };
                for dep in deps {
                    let mut dep_reachability = dep.complexified_marker;
                    dep_reachability.and(parent_reachability);
                    let additional_activated_extras =
                        newly_activated_extras(dep, &activated_extras);
                    if !dep_reachability.evaluate(
                        marker_env,
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
                    let dep_dist = self.lock().find_by_id(&dep.package_id);
                    // Push its dependencies on the queue.
                    if add_reachability(
                        &mut reachability,
                        (&dep.package_id, None),
                        dep_reachability,
                    ) {
                        queue.push_back((dep_dist, None));
                    }
                    for extra in &dep.extra {
                        if add_reachability(
                            &mut reachability,
                            (&dep.package_id, Some(extra)),
                            dep_reachability,
                        ) {
                            queue.push_back((dep_dist, Some(extra)));
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

        while let Some((package, extra)) = queue.pop_front() {
            let deps = if let Some(extra) = extra {
                Either::Left(
                    package
                        .optional_dependencies
                        .get(extra)
                        .into_iter()
                        .flatten(),
                )
            } else {
                Either::Right(package.dependencies.iter())
            };
            for dep in deps {
                if validate_conflicts && dep.complexified_marker.has_conflict_marker() {
                    dependencies_for_conflict_validation.push((package, dep));
                }
                if !dep.complexified_marker.evaluate(
                    marker_env,
                    activated_projects.iter().copied(),
                    activated_extras.iter().copied(),
                    activated_groups.iter().copied(),
                ) {
                    continue;
                }

                let dep_dist = self.lock().find_by_id(&dep.package_id);

                // Add the dependency to the graph.
                let dep_index = match inverse.entry(&dep.package_id) {
                    Entry::Vacant(entry) => {
                        let index = petgraph.add_node(self.package_to_node(
                            dep_dist,
                            tags,
                            build_options,
                            install_options,
                            marker_env,
                        )?);
                        entry.insert(index);
                        index
                    }
                    Entry::Occupied(entry) => *entry.get(),
                };

                // Add the edge.
                let index = inverse[&package.id];
                petgraph.add_edge(
                    index,
                    dep_index,
                    if let Some(extra) = extra {
                        Edge::Optional(extra.clone())
                    } else {
                        Edge::Prod
                    },
                );

                // Push its dependencies on the queue.
                if seen.insert((&dep.package_id, None)) {
                    queue.push_back((dep_dist, None));
                }
                for extra in &dep.extra {
                    if seen.insert((&dep.package_id, Some(extra))) {
                        queue.push_back((dep_dist, Some(extra)));
                    }
                }
            }
        }

        // Evaluate conflict markers from concrete roots, not from workspace members that depend on
        // them. Reject markers that still depend on conflict items outside the resulting subgraph.
        if !dependencies_for_conflict_validation.is_empty() {
            let subgraph_packages = inverse
                .keys()
                .map(|package_id| &package_id.name)
                .collect::<FxHashSet<_>>();

            // The environment and conflict state are shared by every dependency, so repeated
            // markers have the same result.
            let mut validated_markers = FxHashSet::default();
            for (package, dependency) in dependencies_for_conflict_validation {
                if !validated_markers.insert(dependency.complexified_marker) {
                    continue;
                }
                let mut marker = dependency.complexified_marker;
                for item in self.lock().conflicts().iter().flat_map(ConflictSet::iter) {
                    if !subgraph_packages.contains(item.package()) {
                        continue;
                    }

                    let active = match item.kind() {
                        ConflictKind::Project => activated_projects.contains(&item.package()),
                        ConflictKind::Extra(extra) => {
                            activated_extras.contains(&(item.package(), extra))
                        }
                        ConflictKind::Group(group) => {
                            activated_groups.contains(&(item.package(), group))
                        }
                    };
                    if active {
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

        Ok(Resolution::new(petgraph))
    }
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
                let Some(root) = self.packages.get(*index) else {
                    return Err(LockErrorKind::RootPackageMissingFromLock {
                        id: root.id.clone(),
                    }
                    .into());
                };
                concrete_roots.push(root);
            }
        }

        LockedPackages {
            lock: self,
            install_path,
            project_name,
        }
        .to_resolution_from_packages(
            &concrete_roots,
            false,
            marker_env,
            tags,
            extras,
            groups,
            build_options,
            install_options,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::sync::LazyLock;

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
            &Platform::new(
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

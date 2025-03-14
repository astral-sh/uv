use std::collections::hash_map::Entry;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;

use either::Either;
use itertools::Itertools;
use petgraph::Graph;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use uv_configuration::{
    BuildOptions, DependencyGroupsWithDefaults, ExtrasSpecification, InstallOptions,
};
use uv_distribution_types::{Edge, Node, Resolution, ResolvedDist};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::MarkerTree;
use uv_platform_tags::Tags;
use uv_pypi_types::ResolverMarkerEnvironment;

use crate::lock::{LockErrorKind, Package, TagPolicy};
use crate::{Lock, LockError};

pub trait Installable<'lock> {
    /// Return the root install path.
    fn install_path(&self) -> &'lock Path;

    /// Return the [`Lock`] to install.
    fn lock(&self) -> &'lock Lock;

    /// Return the [`PackageName`] of the root packages in the target.
    fn roots(&self) -> impl Iterator<Item = &PackageName>;

    /// Return the [`PackageName`] of the target, if available.
    fn project_name(&self) -> Option<&PackageName>;

    /// Convert the [`Lock`] to a [`Resolution`] using the given marker environment, tags, and root.
    fn to_resolution(
        &self,
        marker_env: &ResolverMarkerEnvironment,
        tags: &Tags,
        extras: &ExtrasSpecification,
        dev: &DependencyGroupsWithDefaults,
        build_options: &BuildOptions,
        install_options: &InstallOptions,
    ) -> Result<Resolution, LockError> {
        let size_guess = self.lock().packages.len();
        let mut petgraph = Graph::with_capacity(size_guess, size_guess);
        let mut inverse = FxHashMap::with_capacity_and_hasher(size_guess, FxBuildHasher);

        let mut queue: VecDeque<(&Package, Option<&ExtraName>)> = VecDeque::new();
        let mut seen = FxHashSet::default();
        let mut activated_extras: Vec<(&PackageName, &ExtraName)> = vec![];
        let mut activated_groups: Vec<(&PackageName, &GroupName)> = vec![];

        let root = petgraph.add_node(Node::Root);

        // Determine the set of activated extras and groups, from the root.
        //
        // TODO(charlie): This isn't quite right. Below, when we add the dependency groups to the
        // graph, we rely on the activated extras and dependency groups, to evaluate the conflict
        // marker. But at that point, we don't know the full set of activated extras; this is only
        // computed below. We somehow need to add the dependency groups _after_ we've computed all
        // enabled extras, but the groups themselves could depend on the set of enabled extras.
        if !self.lock().conflicts().is_empty() {
            for root_name in self.roots() {
                let dist = self
                    .lock()
                    .find_by_name(root_name)
                    .map_err(|_| LockErrorKind::MultipleRootPackages {
                        name: root_name.clone(),
                    })?
                    .ok_or_else(|| LockErrorKind::MissingRootPackage {
                        name: root_name.clone(),
                    })?;

                // Track the activated extras.
                if dev.prod() {
                    for extra in extras.extra_names(dist.optional_dependencies.keys()) {
                        activated_extras.push((&dist.id.name, extra));
                    }
                }

                // Track the activated groups.
                for group in dist
                    .dependency_groups
                    .keys()
                    .filter(|group| dev.contains(group))
                {
                    activated_groups.push((&dist.id.name, group));
                }
            }
        }

        // Add the workspace packages to the queue.
        for root_name in self.roots() {
            let dist = self
                .lock()
                .find_by_name(root_name)
                .map_err(|_| LockErrorKind::MultipleRootPackages {
                    name: root_name.clone(),
                })?
                .ok_or_else(|| LockErrorKind::MissingRootPackage {
                    name: root_name.clone(),
                })?;

            // Add the workspace package to the graph.
            let index = petgraph.add_node(if dev.prod() {
                self.package_to_node(dist, tags, build_options, install_options)?
            } else {
                self.non_installable_node(dist, tags)?
            });
            inverse.insert(&dist.id, index);

            // Add an edge from the root.
            petgraph.add_edge(root, index, Edge::Prod(MarkerTree::TRUE));

            if dev.prod() {
                // Push its dependencies onto the queue.
                queue.push_back((dist, None));
                for extra in extras.extra_names(dist.optional_dependencies.keys()) {
                    queue.push_back((dist, Some(extra)));
                }
            }

            // Add any dev dependencies.
            for (group, dep) in dist
                .dependency_groups
                .iter()
                .filter_map(|(group, deps)| {
                    if dev.contains(group) {
                        Some(deps.iter().map(move |dep| (group, dep)))
                    } else {
                        None
                    }
                })
                .flatten()
            {
                if !dep.complexified_marker.evaluate(
                    marker_env,
                    activated_extras.iter().copied(),
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
                        if !dev.prod() {
                            *node = self.package_to_node(
                                dep_dist,
                                tags,
                                build_options,
                                install_options,
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
                    Edge::Dev(group.clone(), dep.complexified_marker.pep508()),
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
            let index = petgraph.add_node(if dev.prod() {
                self.package_to_node(dist, tags, build_options, install_options)?
            } else {
                self.non_installable_node(dist, tags)?
            });
            inverse.insert(&dist.id, index);

            // Add the edge.
            petgraph.add_edge(root, index, Edge::Prod(dependency.marker));

            // Push its dependencies on the queue.
            if seen.insert((&dist.id, None)) {
                queue.push_back((dist, None));
            }
            for extra in &dependency.extras {
                if seen.insert((&dist.id, Some(extra))) {
                    queue.push_back((dist, Some(extra)));
                }
            }
        }

        // Add any dependency groups that are exclusive to the workspace root (e.g., dev
        // dependencies in (legacy) non-project workspace roots).
        for (group, dependency) in self
            .lock()
            .dependency_groups()
            .iter()
            .filter_map(|(group, deps)| {
                if dev.contains(group) {
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
                    if !dev.prod() {
                        *node = self.package_to_node(dist, tags, build_options, install_options)?;
                    }
                    index
                }
            };

            // Add the edge.
            petgraph.add_edge(root, index, Edge::Dev(group.clone(), dependency.marker));

            // Push its dependencies on the queue.
            if seen.insert((&dist.id, None)) {
                queue.push_back((dist, None));
            }
            for extra in &dependency.extras {
                if seen.insert((&dist.id, Some(extra))) {
                    queue.push_back((dist, Some(extra)));
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
        if !self.lock().conflicts().is_empty() {
            let mut activated_extras_set: BTreeSet<(&PackageName, &ExtraName)> =
                activated_extras.iter().copied().collect();
            let mut queue = queue.clone();
            let mut seen = seen.clone();
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
                    let mut additional_activated_extras = vec![];
                    for extra in &dep.extra {
                        let key = (&dep.package_id.name, extra);
                        if !activated_extras_set.contains(&key) {
                            additional_activated_extras.push(key);
                        }
                    }
                    if !dep.complexified_marker.evaluate(
                        marker_env,
                        activated_extras
                            .iter()
                            .chain(additional_activated_extras.iter())
                            .copied(),
                        activated_groups.iter().copied(),
                    ) {
                        continue;
                    }
                    // It is, I believe, possible to be here for a dependency that
                    // will ultimately not be included in the final resolution.
                    // Specifically, carrying on from the example in the comments
                    // above, we might visit `torch` first and thus not know if
                    // the `cpu` feature is enabled or not, and thus, the marker
                    // evaluation above will pass.
                    //
                    // So is this a problem? Well, this is the main reason why we
                    // do two graph traversals. On the second traversal below, we
                    // will have seen all of the enabled extras, and so `torch`
                    // will be excluded.
                    //
                    // But could this lead to a bigger list of activated extras
                    // than we actually have? I believe that is indeed possible,
                    // but I think it is only a problem if it leads to extras that
                    // *conflict* with one another being simultaneously enabled.
                    // However, after this first traversal, we check our set of
                    // accumulated extras to ensure that there are no conflicts. If
                    // there are, we raise an error. ---AG

                    for key in additional_activated_extras {
                        activated_extras_set.insert(key);
                        activated_extras.push(key);
                    }
                    let dep_dist = self.lock().find_by_id(&dep.package_id);
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
                if !dep.complexified_marker.evaluate(
                    marker_env,
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
                        )?);
                        entry.insert(index);
                        index
                    }
                    Entry::Occupied(entry) => {
                        let index = *entry.get();
                        index
                    }
                };

                // Add the edge.
                let index = inverse[&package.id];
                petgraph.add_edge(
                    index,
                    dep_index,
                    if let Some(extra) = extra {
                        Edge::Optional(extra.clone(), dep.complexified_marker.pep508())
                    } else {
                        Edge::Prod(dep.complexified_marker.pep508())
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

        Ok(Resolution::new(petgraph))
    }

    /// Create an installable [`Node`] from a [`Package`].
    fn installable_node(
        &self,
        package: &Package,
        tags: &Tags,
        build_options: &BuildOptions,
    ) -> Result<Node, LockError> {
        let dist = package.to_dist(
            self.install_path(),
            TagPolicy::Required(tags),
            build_options,
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
            install: true,
        })
    }

    /// Create a non-installable [`Node`] from a [`Package`].
    fn non_installable_node(&self, package: &Package, tags: &Tags) -> Result<Node, LockError> {
        let dist = package.to_dist(
            self.install_path(),
            TagPolicy::Preferred(tags),
            &BuildOptions::default(),
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
    ) -> Result<Node, LockError> {
        if install_options.include_package(
            package.name(),
            self.project_name(),
            self.lock().members(),
        ) {
            self.installable_node(package, tags, build_options)
        } else {
            self.non_installable_node(package, tags)
        }
    }
}

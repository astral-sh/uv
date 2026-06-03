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
use uv_distribution_types::{Edge, Node, Requirement, Resolution, ResolvedDist};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_platform_tags::Tags;
use uv_pypi_types::ResolverMarkerEnvironment;

use crate::lock::{Dependency, HashedDist, LockErrorKind, Package, TagPolicy};
use crate::{Lock, LockError};

fn newly_activated_extras<'lock>(
    package: &'lock PackageName,
    extras: impl Iterator<Item = &'lock ExtraName>,
    activated_extras: &[(&'lock PackageName, &'lock ExtraName)],
) -> Vec<(&'lock PackageName, &'lock ExtraName)> {
    extras
        .filter_map(|extra| {
            let key = (package, extra);
            (!activated_extras.contains(&key)).then_some(key)
        })
        .collect()
}

fn activated_requirement_extras<'lock>(
    package: &'lock PackageName,
    requirement: &'lock Requirement,
    marker_env: &ResolverMarkerEnvironment,
    activated_extras: &[(&'lock PackageName, &'lock ExtraName)],
    next_activated_extras: &[(&'lock PackageName, &'lock ExtraName)],
) -> Option<Vec<(&'lock PackageName, &'lock ExtraName)>> {
    let mut package_extras = activated_extras
        .iter()
        .filter_map(|(candidate, extra)| (*candidate == package).then_some((*extra).clone()))
        .collect::<Vec<_>>();
    if requirement.name == *package {
        package_extras.extend(requirement.extras.iter().cloned());
    }
    requirement
        .evaluate_markers(Some(marker_env), &package_extras)
        .then(|| {
            newly_activated_extras(
                &requirement.name,
                requirement.extras.iter(),
                next_activated_extras,
            )
        })
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
        let size_guess = self.lock().packages.len();
        let mut petgraph = Graph::with_capacity(size_guess, size_guess);
        let mut inverse = FxHashMap::with_capacity_and_hasher(size_guess, FxBuildHasher);

        let mut queue: VecDeque<(&Package, Option<&ExtraName>)> = VecDeque::new();
        let mut seen = FxHashSet::default();
        let mut activated_projects: Vec<&PackageName> = vec![];
        let mut activated_extras: Vec<(&PackageName, &ExtraName)> = vec![];
        let mut activated_groups: Vec<(&PackageName, &GroupName)> = vec![];
        let needs_activation_context = !self.lock().conflicts().is_empty()
            || self.lock().packages.iter().any(|package| {
                package
                    .dependencies
                    .iter()
                    .chain(package.optional_dependencies.values().flatten())
                    .chain(package.dependency_groups.values().flatten())
                    .any(|dependency| !dependency.complexified_marker.conflict().is_true())
            });

        let root = petgraph.add_node(Node::Root);

        // Determine the set of activated extras and groups, from the root.
        if needs_activation_context {
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
                if groups.prod() {
                    activated_projects.push(&dist.id.name);
                    for extra in extras.extra_names(dist.provides_extras().iter()) {
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
        let mut roots = vec![];
        let mut group_dependencies: Vec<(_, &GroupName, &Dependency)> = vec![];
        let mut group_requirements: Vec<(&PackageName, &Requirement)> = vec![];
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
            let index = petgraph.add_node(if groups.prod() {
                self.package_to_node(dist, tags, build_options, install_options, marker_env)?
            } else {
                self.non_installable_node(dist, tags, marker_env)?
            });
            inverse.insert(&dist.id, index);

            // Add an edge from the root.
            petgraph.add_edge(root, index, Edge::Prod);

            // Push the package onto the queue.
            roots.push((dist, index));
        }

        // Add the workspace dependencies to the queue.
        for (dist, index) in roots {
            if groups.prod() {
                // Push its dependencies onto the queue.
                queue.push_back((dist, None));
                for extra in extras.extra_names(dist.optional_dependencies.keys()) {
                    queue.push_back((dist, Some(extra)));
                }
            }

            // Add any dev dependencies.
            group_requirements.extend(
                dist.dependency_groups()
                    .iter()
                    .filter(|(group, _)| groups.contains(group))
                    .flat_map(|(_, requirements)| {
                        requirements
                            .iter()
                            .map(|requirement| (&dist.id.name, requirement))
                    }),
            );
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
                group_dependencies.push((index, group, dep));
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
            let index = petgraph.add_node(if groups.prod() {
                self.package_to_node(dist, tags, build_options, install_options, marker_env)?
            } else {
                self.non_installable_node(dist, tags, marker_env)?
            });
            inverse.insert(&dist.id, index);

            // Add the edge.
            petgraph.add_edge(root, index, Edge::Prod);

            activated_extras.extend(newly_activated_extras(
                &dist.id.name,
                dependency.extras.iter(),
                &activated_extras,
            ));

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

            activated_extras.extend(newly_activated_extras(
                &dist.id.name,
                dependency.extras.iter(),
                &activated_extras,
            ));

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
        // until the activated extras stabilize. It's only in the final
        // traversal that we actually build up our resolution graph. The
        // activation traversal includes the extras explicitly enabled on the
        // CLI (which were gathered above) and the extras enabled via dependency
        // specifications like `foo[extra]`. We need to do this to correctly
        // support conflicting extras and source-selection markers.
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
        if needs_activation_context {
            let root_activated_extras = activated_extras.clone();
            let mut seen_activation_contexts = FxHashSet::default();

            loop {
                let activation_context = activated_extras.iter().copied().collect::<BTreeSet<_>>();
                if !seen_activation_contexts.insert(activation_context.clone()) {
                    return Err(LockErrorKind::UnstableActivationContext.into());
                }

                let mut next_activated_extras = root_activated_extras.clone();
                let mut queue = queue.clone();
                let mut seen = seen.clone();
                for (package, requirement) in &group_requirements {
                    let Some(additional_activated_extras) = activated_requirement_extras(
                        package,
                        requirement,
                        marker_env,
                        &activated_extras,
                        &next_activated_extras,
                    ) else {
                        continue;
                    };
                    next_activated_extras.extend(additional_activated_extras);
                }
                for (_, _, dep) in &group_dependencies {
                    let additional_activated_extras = newly_activated_extras(
                        &dep.package_id.name,
                        dep.extra.iter(),
                        &next_activated_extras,
                    );
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
                    next_activated_extras.extend(additional_activated_extras);

                    let dep_dist = self.lock().find_by_id(&dep.package_id);
                    if seen.insert((&dep.package_id, None)) {
                        queue.push_back((dep_dist, None));
                    }
                    for extra in &dep.extra {
                        if seen.insert((&dep.package_id, Some(extra))) {
                            queue.push_back((dep_dist, Some(extra)));
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
                        let additional_activated_extras = newly_activated_extras(
                            &dep.package_id.name,
                            dep.extra.iter(),
                            &next_activated_extras,
                        );
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

                        for key in additional_activated_extras {
                            next_activated_extras.push(key);
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

                let next_activation_context = next_activated_extras
                    .iter()
                    .copied()
                    .collect::<BTreeSet<_>>();
                if next_activation_context == activation_context {
                    activated_extras = next_activated_extras;
                    break;
                }
                activated_extras = next_activated_extras;
            }
        }

        for (index, group, dep) in group_dependencies {
            let additional_activated_extras =
                newly_activated_extras(&dep.package_id.name, dep.extra.iter(), &activated_extras);
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
            activated_extras.extend(additional_activated_extras);

            let dep_dist = self.lock().find_by_id(&dep.package_id);
            let dep_index = match inverse.entry(&dep.package_id) {
                Entry::Vacant(entry) => {
                    let dep_index = petgraph.add_node(self.package_to_node(
                        dep_dist,
                        tags,
                        build_options,
                        install_options,
                        marker_env,
                    )?);
                    entry.insert(dep_index);
                    dep_index
                }
                Entry::Occupied(entry) => {
                    // Critically, if the package is already in the graph, then it's a workspace
                    // member. If it was omitted due to, e.g., `--only-dev`, but is itself
                    // referenced as a development dependency, then we need to re-enable it.
                    let dep_index = *entry.get();
                    let node = &mut petgraph[dep_index];
                    if !groups.prod() {
                        *node = self.package_to_node(
                            dep_dist,
                            tags,
                            build_options,
                            install_options,
                            marker_env,
                        )?;
                    }
                    dep_index
                }
            };

            petgraph.add_edge(index, dep_index, Edge::Dev(group.clone()));

            if seen.insert((&dep.package_id, None)) {
                queue.push_back((dep_dist, None));
            }
            for extra in &dep.extra {
                if seen.insert((&dep.package_id, Some(extra))) {
                    queue.push_back((dep_dist, Some(extra)));
                }
            }
        }

        if !self.lock().conflicts().is_empty() {
            let activated_extras_set: BTreeSet<(&PackageName, &ExtraName)> =
                activated_extras.iter().copied().collect();
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

        Ok(Resolution::new(petgraph))
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

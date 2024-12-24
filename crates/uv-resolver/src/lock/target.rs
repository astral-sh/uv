use std::collections::hash_map::Entry;
use std::collections::VecDeque;

use either::Either;
use petgraph::Graph;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use uv_configuration::{BuildOptions, DevGroupsManifest, ExtrasSpecification, InstallOptions};
use uv_distribution_types::{Edge, Node, Resolution, ResolvedDist};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::MarkerTree;
use uv_platform_tags::Tags;
use uv_pypi_types::ResolverMarkerEnvironment;
use uv_workspace::Workspace;

use crate::lock::{LockErrorKind, Package, TagPolicy};
use crate::{Lock, LockError};

/// A target that can be installed from a lockfile.
#[derive(Debug, Copy, Clone)]
pub enum InstallTarget<'env> {
    /// A project (which could be a workspace root or member).
    Project {
        workspace: &'env Workspace,
        name: &'env PackageName,
        lock: &'env Lock,
    },
    /// An entire workspace.
    Workspace {
        workspace: &'env Workspace,
        lock: &'env Lock,
    },
    /// An entire workspace with a (legacy) non-project root.
    NonProjectWorkspace {
        workspace: &'env Workspace,
        lock: &'env Lock,
    },
}

impl<'env> InstallTarget<'env> {
    /// Return the [`Workspace`] of the target.
    pub fn workspace(&self) -> &'env Workspace {
        match self {
            Self::Project { workspace, .. } => workspace,
            Self::Workspace { workspace, .. } => workspace,
            Self::NonProjectWorkspace { workspace, .. } => workspace,
        }
    }

    /// Return the [`Lock`] of the target.
    pub fn lock(&self) -> &'env Lock {
        match self {
            Self::Project { lock, .. } => lock,
            Self::Workspace { lock, .. } => lock,
            Self::NonProjectWorkspace { lock, .. } => lock,
        }
    }

    /// Return the [`PackageName`] of the target.
    pub fn packages(&self) -> impl Iterator<Item = &PackageName> {
        match self {
            Self::Project { name, .. } => Either::Right(Either::Left(std::iter::once(*name))),
            Self::NonProjectWorkspace { lock, .. } => Either::Left(lock.members().iter()),
            Self::Workspace { lock, .. } => {
                // Identify the workspace members.
                //
                // The members are encoded directly in the lockfile, unless the workspace contains a
                // single member at the root, in which case, we identify it by its source.
                if lock.members().is_empty() {
                    Either::Right(Either::Right(
                        lock.root().into_iter().map(|package| &package.id.name),
                    ))
                } else {
                    Either::Left(lock.members().iter())
                }
            }
        }
    }

    /// Return the [`PackageName`] of the target, if available.
    pub fn project_name(&self) -> Option<&PackageName> {
        match self {
            Self::Project { name, .. } => Some(name),
            Self::Workspace { .. } => None,
            Self::NonProjectWorkspace { .. } => None,
        }
    }

    /// Convert the [`Lock`] to a [`Resolution`] using the given marker environment, tags, and root.
    pub fn to_resolution(
        &self,
        marker_env: &ResolverMarkerEnvironment,
        tags: &Tags,
        extras: &ExtrasSpecification,
        dev: &DevGroupsManifest,
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

        // Add the workspace packages to the queue.
        for root_name in self.packages() {
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
                // Push its dependencies on the queue and track
                // activated extras.
                queue.push_back((dist, None));
                for extra in extras.extra_names(dist.optional_dependencies.keys()) {
                    queue.push_back((dist, Some(extra)));
                    activated_extras.push((&dist.id.name, extra));
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
                if !dep.complexified_marker.evaluate_no_extras(marker_env) {
                    continue;
                }
                activated_groups.push((&dist.id.name, group));

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
                    &activated_extras,
                    &activated_groups,
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
            self.workspace().install_path(),
            TagPolicy::Required(tags),
            build_options,
        )?;
        let version = package.version().clone();
        let dist = ResolvedDist::Installable { dist, version };
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
            self.workspace().install_path(),
            TagPolicy::Preferred(tags),
            &BuildOptions::default(),
        )?;
        let version = package.version().clone();
        let dist = ResolvedDist::Installable { dist, version };
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

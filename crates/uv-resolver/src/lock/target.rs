use either::Either;
use petgraph::Graph;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, VecDeque};

use uv_configuration::{BuildOptions, DevGroupsManifest, ExtrasSpecification, InstallOptions};
use uv_distribution_types::{Edge, Node, Resolution, ResolvedDist};
use uv_normalize::{ExtraName, GroupName, PackageName, DEV_DEPENDENCIES};
use uv_pep508::MarkerTree;
use uv_platform_tags::Tags;
use uv_pypi_types::{ResolverMarkerEnvironment, VerbatimParsedUrl};
use uv_workspace::dependency_groups::{DependencyGroupError, FlatDependencyGroups};
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

    /// Return the [`InstallTarget`] dependency groups.
    ///
    /// Returns dependencies that apply to the workspace root, but not any of its members. As such,
    /// only returns a non-empty iterator for virtual workspaces, which can include dev dependencies
    /// on the virtual root.
    pub fn groups(
        &self,
    ) -> Result<
        BTreeMap<GroupName, Vec<uv_pep508::Requirement<VerbatimParsedUrl>>>,
        DependencyGroupError,
    > {
        match self {
            Self::Project { .. } => Ok(BTreeMap::default()),
            Self::Workspace { .. } => Ok(BTreeMap::default()),
            Self::NonProjectWorkspace { workspace, .. } => {
                // For non-projects, we might have `dependency-groups` or `tool.uv.dev-dependencies`
                // that are attached to the workspace root (which isn't a member).

                // First, collect `tool.uv.dev_dependencies`
                let dev_dependencies = workspace
                    .pyproject_toml()
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.dev_dependencies.as_ref());

                // Then, collect `dependency-groups`
                let dependency_groups = workspace
                    .pyproject_toml()
                    .dependency_groups
                    .iter()
                    .flatten()
                    .collect::<BTreeMap<_, _>>();

                // Merge any overlapping groups.
                let mut map = BTreeMap::new();
                for (name, dependencies) in
                    FlatDependencyGroups::from_dependency_groups(&dependency_groups)
                        .map_err(|err| err.with_dev_dependencies(dev_dependencies))?
                        .into_iter()
                        .chain(
                            // Only add the `dev` group if `dev-dependencies` is defined.
                            dev_dependencies.into_iter().map(|requirements| {
                                (DEV_DEPENDENCIES.clone(), requirements.clone())
                            }),
                        )
                {
                    match map.entry(name) {
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(dependencies);
                        }
                        std::collections::btree_map::Entry::Occupied(mut entry) => {
                            entry.get_mut().extend(dependencies);
                        }
                    }
                }

                Ok(map)
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
                // Push its dependencies on the queue.
                queue.push_back((dist, None));
                match extras {
                    ExtrasSpecification::None => {}
                    ExtrasSpecification::All => {
                        for extra in dist.optional_dependencies.keys() {
                            queue.push_back((dist, Some(extra)));
                        }
                    }
                    ExtrasSpecification::Some(extras) => {
                        for extra in extras {
                            queue.push_back((dist, Some(extra)));
                        }
                    }
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
                if !dep.complexified_marker.evaluate(marker_env, &[]) {
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
                        let index = *entry.get();
                        index
                    }
                };

                petgraph.add_edge(
                    index,
                    dep_index,
                    Edge::Dev(group.clone(), dep.complexified_marker.clone()),
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

        // Add any dependency groups that are exclusive to the workspace root (e.g., dev
        // dependencies in (legacy) non-project workspace roots).
        let groups = self
            .groups()
            .map_err(|err| LockErrorKind::DependencyGroup { err })?;
        for (group, dependency) in groups
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
                    let index = *entry.get();
                    index
                }
            };

            // Add the edge.
            petgraph.add_edge(
                root,
                index,
                Edge::Dev(group.clone(), dependency.marker.clone()),
            );

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
                if !dep.complexified_marker.evaluate(marker_env, &[]) {
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
                        Edge::Optional(extra.clone(), dep.complexified_marker.clone())
                    } else {
                        Edge::Prod(dep.complexified_marker.clone())
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

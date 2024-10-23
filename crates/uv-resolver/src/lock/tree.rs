use std::borrow::Cow;
use std::collections::{BTreeSet, VecDeque};

use itertools::Itertools;
use petgraph::prelude::EdgeRef;
use petgraph::visit::Dfs;
use petgraph::Direction;
use rustc_hash::{FxHashMap, FxHashSet};

use uv_configuration::DevMode;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pypi_types::ResolverMarkerEnvironment;

use crate::lock::{Dependency, PackageId};
use crate::Lock;

#[derive(Debug)]
pub struct TreeDisplay<'env> {
    /// The constructed dependency graph.
    graph: petgraph::graph::Graph<&'env PackageId, Edge<'env>, petgraph::Directed>,
    /// An inverse map from [`PackageId`] to the corresponding node index in the graph.
    inverse: FxHashMap<&'env PackageId, petgraph::graph::NodeIndex>,
    /// Maximum display depth of the dependency tree.
    depth: usize,
    /// Whether to de-duplicate the displayed dependencies.
    no_dedupe: bool,
}

impl<'env> TreeDisplay<'env> {
    /// Create a new [`DisplayDependencyGraph`] for the set of installed packages.
    pub fn new(
        lock: &'env Lock,
        markers: Option<&'env ResolverMarkerEnvironment>,
        depth: usize,
        prune: &[PackageName],
        packages: &[PackageName],
        dev: DevMode,
        no_dedupe: bool,
        invert: bool,
    ) -> Self {
        // Create a graph.
        let mut graph = petgraph::graph::Graph::<&PackageId, Edge, petgraph::Directed>::new();

        // Create the complete graph.
        let mut inverse = FxHashMap::default();
        for package in &lock.packages {
            if prune.contains(&package.id.name) {
                continue;
            }

            for dependency in &package.dependencies {
                // Skip dependencies that don't apply to the current environment.
                if let Some(environment_markers) = markers {
                    if !dependency
                        .complexified_marker
                        .evaluate(environment_markers, &[])
                    {
                        continue;
                    }
                }

                // Insert the package into the graph.
                let package_node = if let Some(index) = inverse.get(&package.id) {
                    *index
                } else {
                    let index = graph.add_node(&package.id);
                    inverse.insert(&package.id, index);
                    index
                };

                // Insert the dependency into the graph.
                let dependency_node = if let Some(index) = inverse.get(&dependency.package_id) {
                    *index
                } else {
                    let index = graph.add_node(&dependency.package_id);
                    inverse.insert(&dependency.package_id, index);
                    index
                };

                // Add an edge between the package and the dependency.
                if invert {
                    graph.add_edge(
                        dependency_node,
                        package_node,
                        Edge::Prod(Cow::Owned(Dependency {
                            package_id: package.id.clone(),
                            extra: dependency.extra.clone(),
                            simplified_marker: dependency.simplified_marker.clone(),
                            complexified_marker: dependency.complexified_marker.clone(),
                        })),
                    );
                } else {
                    graph.add_edge(
                        package_node,
                        dependency_node,
                        Edge::Prod(Cow::Borrowed(dependency)),
                    );
                }
            }

            for (extra, dependencies) in &package.optional_dependencies {
                for dependency in dependencies {
                    // Skip dependencies that don't apply to the current environment.
                    if let Some(environment_markers) = markers {
                        if !dependency
                            .complexified_marker
                            .evaluate(environment_markers, &[])
                        {
                            continue;
                        }
                    }

                    // Insert the package into the graph.
                    let package_node = if let Some(index) = inverse.get(&package.id) {
                        *index
                    } else {
                        let index = graph.add_node(&package.id);
                        inverse.insert(&package.id, index);
                        index
                    };

                    // Insert the dependency into the graph.
                    let dependency_node = if let Some(index) = inverse.get(&dependency.package_id) {
                        *index
                    } else {
                        let index = graph.add_node(&dependency.package_id);
                        inverse.insert(&dependency.package_id, index);
                        index
                    };

                    // Add an edge between the package and the dependency.
                    if invert {
                        graph.add_edge(
                            dependency_node,
                            package_node,
                            Edge::Optional(
                                extra,
                                Cow::Owned(Dependency {
                                    package_id: package.id.clone(),
                                    extra: dependency.extra.clone(),
                                    simplified_marker: dependency.simplified_marker.clone(),
                                    complexified_marker: dependency.complexified_marker.clone(),
                                }),
                            ),
                        );
                    } else {
                        graph.add_edge(
                            package_node,
                            dependency_node,
                            Edge::Optional(extra, Cow::Borrowed(dependency)),
                        );
                    }
                }
            }

            for (group, dependencies) in &package.dev_dependencies {
                for dependency in dependencies {
                    // Skip dependencies that don't apply to the current environment.
                    if let Some(environment_markers) = markers {
                        if !dependency
                            .complexified_marker
                            .evaluate(environment_markers, &[])
                        {
                            continue;
                        }
                    }

                    // Insert the package into the graph.
                    let package_node = if let Some(index) = inverse.get(&package.id) {
                        *index
                    } else {
                        let index = graph.add_node(&package.id);
                        inverse.insert(&package.id, index);
                        index
                    };

                    // Insert the dependency into the graph.
                    let dependency_node = if let Some(index) = inverse.get(&dependency.package_id) {
                        *index
                    } else {
                        let index = graph.add_node(&dependency.package_id);
                        inverse.insert(&dependency.package_id, index);
                        index
                    };

                    // Add an edge between the package and the dependency.
                    if invert {
                        graph.add_edge(
                            dependency_node,
                            package_node,
                            Edge::Dev(
                                group,
                                Cow::Owned(Dependency {
                                    package_id: package.id.clone(),
                                    extra: dependency.extra.clone(),
                                    simplified_marker: dependency.simplified_marker.clone(),
                                    complexified_marker: dependency.complexified_marker.clone(),
                                }),
                            ),
                        );
                    } else {
                        graph.add_edge(
                            package_node,
                            dependency_node,
                            Edge::Dev(group, Cow::Borrowed(dependency)),
                        );
                    }
                }
            }
        }

        let mut modified = false;

        // Filter the graph to those nodes reachable from the root nodes.
        if !packages.is_empty() {
            let mut reachable = FxHashSet::default();

            // Perform a DFS from the root nodes to find the reachable nodes.
            let mut dfs = Dfs {
                stack: graph
                    .node_indices()
                    .filter(|index| packages.contains(&graph[*index].name))
                    .collect::<Vec<_>>(),
                ..Dfs::empty(&graph)
            };
            while let Some(node) = dfs.next(&graph) {
                reachable.insert(node);
            }

            // Remove the unreachable nodes from the graph.
            graph.retain_nodes(|_, index| reachable.contains(&index));
            modified = true;
        }

        // Filter the graph to those that are reachable from production nodes, if `--no-dev` or
        // `--only-dev` was specified.
        if dev != DevMode::Include {
            let mut reachable = FxHashSet::default();

            // Perform a DFS from the root nodes to find the reachable nodes, following only the
            // production edges.
            let mut stack = graph
                .node_indices()
                .filter(|index| {
                    graph
                        .edges_directed(*index, Direction::Incoming)
                        .next()
                        .is_none()
                })
                .collect::<VecDeque<_>>();
            while let Some(node) = stack.pop_front() {
                reachable.insert(node);
                for edge in graph.edges_directed(node, Direction::Outgoing) {
                    if matches!(edge.weight(), Edge::Prod(_) | Edge::Optional(_, _)) {
                        stack.push_back(edge.target());
                    }
                }
            }

            // Remove the unreachable nodes from the graph.
            graph.retain_nodes(|_, index| {
                if reachable.contains(&index) {
                    dev != DevMode::Only
                } else {
                    dev != DevMode::Exclude
                }
            });
            modified = true;
        }

        // If the graph was modified, re-create the inverse map.
        if modified {
            inverse.clear();
            for node in graph.node_indices() {
                inverse.insert(graph[node], node);
            }
        }

        Self {
            graph,
            inverse,
            depth,
            no_dedupe,
        }
    }

    /// Perform a depth-first traversal of the given package and its dependencies.
    fn visit(
        &'env self,
        node: Node<'env>,
        visited: &mut FxHashMap<&'env PackageId, Vec<&'env PackageId>>,
        path: &mut Vec<&'env PackageId>,
    ) -> Vec<String> {
        // Short-circuit if the current path is longer than the provided depth.
        if path.len() > self.depth {
            return Vec::new();
        }

        let line = {
            let mut line = format!("{}", node.package_id().name);

            if let Some(extras) = node.extras().filter(|extras| !extras.is_empty()) {
                line.push_str(&format!("[{}]", extras.iter().join(",")));
            }

            line.push_str(&format!(" v{}", node.package_id().version));

            match node {
                Node::Root(_) => line,
                Node::Dependency(_) => line,
                Node::OptionalDependency(extra, _) => format!("{line} (extra: {extra})"),
                Node::DevDependency(group, _) => format!("{line} (group: {group})"),
            }
        };

        // Skip the traversal if:
        // 1. The package is in the current traversal path (i.e., a dependency cycle).
        // 2. The package has been visited and de-duplication is enabled (default).
        if let Some(requirements) = visited.get(node.package_id()) {
            if !self.no_dedupe || path.contains(&node.package_id()) {
                return if requirements.is_empty() {
                    vec![line]
                } else {
                    vec![format!("{} (*)", line)]
                };
            }
        }

        let mut dependencies = self
            .graph
            .edges_directed(self.inverse[node.package_id()], Direction::Outgoing)
            .map(|edge| match edge.weight() {
                Edge::Prod(dependency) => Node::Dependency(dependency),
                Edge::Optional(extra, dependency) => Node::OptionalDependency(extra, dependency),
                Edge::Dev(group, dependency) => Node::DevDependency(group, dependency),
            })
            .collect::<Vec<_>>();
        dependencies.sort_unstable();

        let mut lines = vec![line];

        // Keep track of the dependency path to avoid cycles.
        visited.insert(
            node.package_id(),
            dependencies.iter().map(Node::package_id).collect(),
        );
        path.push(node.package_id());

        for (index, dep) in dependencies.iter().enumerate() {
            // For sub-visited packages, add the prefix to make the tree display user-friendly.
            // The key observation here is you can group the tree as follows when you're at the
            // root of the tree:
            // root_package
            // ├── level_1_0          // Group 1
            // │   ├── level_2_0      ...
            // │   │   ├── level_3_0  ...
            // │   │   └── level_3_1  ...
            // │   └── level_2_1      ...
            // ├── level_1_1          // Group 2
            // │   ├── level_2_2      ...
            // │   └── level_2_3      ...
            // └── level_1_2          // Group 3
            //     └── level_2_4      ...
            //
            // The lines in Group 1 and 2 have `├── ` at the top and `|   ` at the rest while
            // those in Group 3 have `└── ` at the top and `    ` at the rest.
            // This observation is true recursively even when looking at the subtree rooted
            // at `level_1_0`.
            let (prefix_top, prefix_rest) = if dependencies.len() - 1 == index {
                ("└── ", "    ")
            } else {
                ("├── ", "│   ")
            };
            for (visited_index, visited_line) in self.visit(*dep, visited, path).iter().enumerate()
            {
                let prefix = if visited_index == 0 {
                    prefix_top
                } else {
                    prefix_rest
                };
                lines.push(format!("{prefix}{visited_line}"));
            }
        }

        path.pop();

        lines
    }

    /// Depth-first traverse the nodes to render the tree.
    fn render(&self) -> Vec<String> {
        let mut visited = FxHashMap::default();
        let mut path = Vec::new();
        let mut lines = Vec::new();

        let roots = {
            let mut roots = self
                .graph
                .node_indices()
                .filter(|index| {
                    self.graph
                        .edges_directed(*index, petgraph::Direction::Incoming)
                        .next()
                        .is_none()
                })
                .collect::<Vec<_>>();
            roots.sort_by_key(|index| &self.graph[*index]);
            roots
        };

        for node in roots {
            path.clear();
            lines.extend(self.visit(Node::Root(self.graph[node]), &mut visited, &mut path));
        }

        lines
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
enum Edge<'env> {
    Prod(Cow<'env, Dependency>),
    Optional(&'env ExtraName, Cow<'env, Dependency>),
    Dev(&'env GroupName, Cow<'env, Dependency>),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
enum Node<'env> {
    Root(&'env PackageId),
    Dependency(&'env Dependency),
    OptionalDependency(&'env ExtraName, &'env Dependency),
    DevDependency(&'env GroupName, &'env Dependency),
}

impl<'env> Node<'env> {
    fn package_id(&self) -> &'env PackageId {
        match self {
            Self::Root(id) => id,
            Self::Dependency(dep) => &dep.package_id,
            Self::OptionalDependency(_, dep) => &dep.package_id,
            Self::DevDependency(_, dep) => &dep.package_id,
        }
    }

    fn extras(&self) -> Option<&BTreeSet<ExtraName>> {
        match self {
            Self::Root(_) => None,
            Self::Dependency(dep) => Some(&dep.extra),
            Self::OptionalDependency(_, dep) => Some(&dep.extra),
            Self::DevDependency(_, dep) => Some(&dep.extra),
        }
    }
}

impl std::fmt::Display for TreeDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use owo_colors::OwoColorize;

        let mut deduped = false;
        for line in self.render() {
            deduped |= line.contains('*');
            writeln!(f, "{line}")?;
        }

        if deduped {
            let message = if self.no_dedupe {
                "(*) Package tree is a cycle and cannot be shown".italic()
            } else {
                "(*) Package tree already displayed".italic()
            };
            writeln!(f, "{message}")?;
        }

        Ok(())
    }
}

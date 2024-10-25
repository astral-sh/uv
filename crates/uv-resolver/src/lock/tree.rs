use std::borrow::Cow;
use std::collections::{BTreeSet, VecDeque};
use std::path::Path;

use itertools::Itertools;
use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::prelude::EdgeRef;
use petgraph::Direction;
use rustc_hash::{FxHashMap, FxHashSet};

use uv_configuration::DevGroupsManifest;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pypi_types::ResolverMarkerEnvironment;

use crate::lock::{Dependency, PackageId, Source};
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
    /// The packages considered as roots of the dependency tree.
    roots: Vec<NodeIndex>,
}

impl<'env> TreeDisplay<'env> {
    /// Create a new [`DisplayDependencyGraph`] for the set of installed packages.
    pub fn new(
        lock: &'env Lock,
        markers: Option<&'env ResolverMarkerEnvironment>,
        depth: usize,
        prune: &[PackageName],
        packages: &[PackageName],
        dev: &DevGroupsManifest,
        no_dedupe: bool,
        invert: bool,
    ) -> Self {
        // Identify the workspace members.
        //
        // The members are encoded directly in the lockfile, unless the workspace contains a
        // single member at the root, in which case, we identify it by its source.
        let members: FxHashSet<&PackageId> = if lock.members().is_empty() {
            lock.packages
                .iter()
                .filter_map(|package| {
                    let (Source::Editable(path) | Source::Virtual(path)) = &package.id.source
                    else {
                        return None;
                    };
                    if path == Path::new("") {
                        Some(&package.id)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            lock.packages
                .iter()
                .filter_map(|package| {
                    if lock.members().contains(&package.id.name) {
                        Some(&package.id)
                    } else {
                        None
                    }
                })
                .collect()
        };

        // Create a graph.
        let mut graph = petgraph::graph::Graph::<&PackageId, Edge, petgraph::Directed>::new();

        // Create the complete graph.
        let mut inverse = FxHashMap::default();
        for package in &lock.packages {
            if prune.contains(&package.id.name) {
                continue;
            }

            for dependency in &package.dependencies {
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
                graph.add_edge(
                    package_node,
                    dependency_node,
                    Edge::Prod(Cow::Borrowed(dependency)),
                );
            }

            for (extra, dependencies) in &package.optional_dependencies {
                for dependency in dependencies {
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
                    graph.add_edge(
                        package_node,
                        dependency_node,
                        Edge::Optional(extra, Cow::Borrowed(dependency)),
                    );
                }
            }

            for (group, dependencies) in &package.dependency_groups {
                for dependency in dependencies {
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
                    graph.add_edge(
                        package_node,
                        dependency_node,
                        Edge::Dev(group, Cow::Borrowed(dependency)),
                    );
                }
            }
        }

        // Step 1: Filter out packages that aren't reachable on this platform.
        if let Some(environment_markers) = markers {
            // Perform a DFS from the root nodes to find the reachable nodes, following only the
            // production edges.
            let mut reachable = graph
                .node_indices()
                .filter(|index| members.contains(graph[*index]))
                .collect::<FxHashSet<_>>();
            let mut stack = reachable.iter().copied().collect::<VecDeque<_>>();
            while let Some(node) = stack.pop_front() {
                for edge in graph.edges_directed(node, Direction::Outgoing) {
                    if edge
                        .weight()
                        .dependency()
                        .complexified_marker
                        .evaluate(environment_markers, &[])
                    {
                        if reachable.insert(edge.target()) {
                            stack.push_back(edge.target());
                        }
                    }
                }
            }

            // Remove the unreachable nodes from the graph.
            graph.retain_nodes(|_, index| reachable.contains(&index));
        }

        // Step 2: Filter the graph to those that are reachable in production or development.
        {
            // Perform a DFS from the root nodes to find the reachable nodes, following only the
            // production edges.
            let mut reachable = graph
                .node_indices()
                .filter(|index| members.contains(graph[*index]))
                .collect::<FxHashSet<_>>();
            let mut stack = reachable.iter().copied().collect::<VecDeque<_>>();
            while let Some(node) = stack.pop_front() {
                for edge in graph.edges_directed(node, Direction::Outgoing) {
                    let include = match edge.weight() {
                        Edge::Prod(_) => dev.prod(),
                        Edge::Optional(_, _) => dev.prod(),
                        Edge::Dev(group, _) => dev.iter().contains(*group),
                    };
                    if include {
                        if reachable.insert(edge.target()) {
                            stack.push_back(edge.target());
                        }
                    }
                }
            }

            // Remove the unreachable nodes from the graph.
            graph.retain_nodes(|_, index| reachable.contains(&index));
        }

        // Step 3: Reverse the graph.
        if invert {
            graph.reverse();
        }

        // Step 4: Filter the graph to those nodes reachable from the target packages.
        if !packages.is_empty() {
            // Perform a DFS from the root nodes to find the reachable nodes.
            let mut reachable = graph
                .node_indices()
                .filter(|index| packages.contains(&graph[*index].name))
                .collect::<FxHashSet<_>>();
            let mut stack = reachable.iter().copied().collect::<VecDeque<_>>();
            while let Some(node) = stack.pop_front() {
                for edge in graph.edges_directed(node, Direction::Outgoing) {
                    if reachable.insert(edge.target()) {
                        stack.push_back(edge.target());
                    }
                }
            }

            // Remove the unreachable nodes from the graph.
            graph.retain_nodes(|_, index| reachable.contains(&index));
        }

        // Compute the list of roots.
        let roots = {
            let mut edges = vec![];

            // Remove any cycles.
            let feedback_set: Vec<EdgeIndex> = petgraph::algo::greedy_feedback_arc_set(&graph)
                .map(|e| e.id())
                .collect();
            for edge_id in feedback_set {
                if let Some((source, target)) = graph.edge_endpoints(edge_id) {
                    if let Some(weight) = graph.remove_edge(edge_id) {
                        edges.push((source, target, weight));
                    }
                }
            }

            // Find the root nodes.
            let mut roots = graph
                .node_indices()
                .filter(|index| {
                    graph
                        .edges_directed(*index, Direction::Incoming)
                        .next()
                        .is_none()
                })
                .collect::<Vec<_>>();

            // Sort the roots.
            roots.sort_by_key(|index| &graph[*index]);

            // Re-add the removed edges.
            for (source, target, weight) in edges {
                graph.add_edge(source, target, weight);
            }

            roots
        };

        // Re-create the inverse map.
        {
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
            roots,
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
                Node::Dependency(_, _) => line,
                Node::OptionalDependency(extra, _, _) => format!("{line} (extra: {extra})"),
                Node::DevDependency(group, _, _) => format!("{line} (group: {group})"),
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
                Edge::Prod(dependency) => Node::Dependency(self.graph[edge.target()], dependency),
                Edge::Optional(extra, dependency) => {
                    Node::OptionalDependency(extra, self.graph[edge.target()], dependency)
                }
                Edge::Dev(group, dependency) => {
                    Node::DevDependency(group, self.graph[edge.target()], dependency)
                }
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

        for node in &self.roots {
            path.clear();
            lines.extend(self.visit(Node::Root(self.graph[*node]), &mut visited, &mut path));
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

impl<'env> Edge<'env> {
    fn dependency(&self) -> &Dependency {
        match self {
            Self::Prod(dependency) => dependency,
            Self::Optional(_, dependency) => dependency,
            Self::Dev(_, dependency) => dependency,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
enum Node<'env> {
    Root(&'env PackageId),
    Dependency(&'env PackageId, &'env Dependency),
    OptionalDependency(&'env ExtraName, &'env PackageId, &'env Dependency),
    DevDependency(&'env GroupName, &'env PackageId, &'env Dependency),
}

impl<'env> Node<'env> {
    fn package_id(&self) -> &'env PackageId {
        match self {
            Self::Root(id) => id,
            Self::Dependency(id, _) => id,
            Self::OptionalDependency(_, id, _) => id,
            Self::DevDependency(_, id, _) => id,
        }
    }

    fn extras(&self) -> Option<&BTreeSet<ExtraName>> {
        match self {
            Self::Root(_) => None,
            Self::Dependency(_, dep) => Some(&dep.extra),
            Self::OptionalDependency(_, _, dep) => Some(&dep.extra),
            Self::DevDependency(_, _, dep) => Some(&dep.extra),
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

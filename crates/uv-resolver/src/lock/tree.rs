use std::collections::{BTreeSet, VecDeque};

use itertools::Itertools;
use owo_colors::OwoColorize;
use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::prelude::EdgeRef;
use petgraph::Direction;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use uv_configuration::DependencyGroupsWithDefaults;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::Version;
use uv_pep508::MarkerTree;
use uv_pypi_types::ResolverMarkerEnvironment;

use crate::lock::PackageId;
use crate::{Lock, PackageMap};

#[derive(Debug)]
pub struct TreeDisplay<'env> {
    /// The constructed dependency graph.
    graph: petgraph::graph::Graph<Node<'env>, Edge<'env>, petgraph::Directed>,
    /// The packages considered as roots of the dependency tree.
    roots: Vec<NodeIndex>,
    /// The latest known version of each package.
    latest: &'env PackageMap<Version>,
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
        latest: &'env PackageMap<Version>,
        depth: usize,
        prune: &[PackageName],
        packages: &[PackageName],
        dev: &DependencyGroupsWithDefaults,
        no_dedupe: bool,
        invert: bool,
    ) -> Self {
        // Create a graph.
        let mut graph = petgraph::graph::Graph::<Node, Edge, petgraph::Directed>::new();

        // Create the complete graph.
        let mut inverse = FxHashMap::default();
        for package in &lock.packages {
            if prune.contains(&package.id.name) {
                continue;
            }

            // Insert the package into the graph.
            let package_node = if let Some(index) = inverse.get(&package.id) {
                *index
            } else {
                let index = graph.add_node(Node::Package(&package.id));
                inverse.insert(&package.id, index);
                index
            };

            if dev.prod() {
                for dependency in &package.dependencies {
                    if markers.is_some_and(|markers| {
                        !dependency.complexified_marker.evaluate_no_extras(markers)
                    }) {
                        continue;
                    }

                    // Insert the dependency into the graph.
                    let dependency_node = if let Some(index) = inverse.get(&dependency.package_id) {
                        *index
                    } else {
                        let index = graph.add_node(Node::Package(&dependency.package_id));
                        inverse.insert(&dependency.package_id, index);
                        index
                    };

                    // Add an edge between the package and the dependency.
                    graph.add_edge(
                        package_node,
                        dependency_node,
                        Edge::Prod(Some(&dependency.extra)),
                    );
                }
            }

            if dev.prod() {
                for (extra, dependencies) in &package.optional_dependencies {
                    for dependency in dependencies {
                        if markers.is_some_and(|markers| {
                            !dependency.complexified_marker.evaluate_no_extras(markers)
                        }) {
                            continue;
                        }

                        // Insert the dependency into the graph.
                        let dependency_node =
                            if let Some(index) = inverse.get(&dependency.package_id) {
                                *index
                            } else {
                                let index = graph.add_node(Node::Package(&dependency.package_id));
                                inverse.insert(&dependency.package_id, index);
                                index
                            };

                        // Add an edge between the package and the dependency.
                        graph.add_edge(
                            package_node,
                            dependency_node,
                            Edge::Optional(extra, Some(&dependency.extra)),
                        );
                    }
                }
            }

            for (group, dependencies) in &package.dependency_groups {
                if dev.contains(group) {
                    for dependency in dependencies {
                        if markers.is_some_and(|markers| {
                            !dependency.complexified_marker.evaluate_no_extras(markers)
                        }) {
                            continue;
                        }

                        // Insert the dependency into the graph.
                        let dependency_node =
                            if let Some(index) = inverse.get(&dependency.package_id) {
                                *index
                            } else {
                                let index = graph.add_node(Node::Package(&dependency.package_id));
                                inverse.insert(&dependency.package_id, index);
                                index
                            };

                        // Add an edge between the package and the dependency.
                        graph.add_edge(
                            package_node,
                            dependency_node,
                            Edge::Dev(group, Some(&dependency.extra)),
                        );
                    }
                }
            }
        }

        // Identify any workspace members.
        //
        // These include:
        // - The members listed in the lockfile.
        // - The root package, if it's not in the list of members. (The root package is omitted from
        //   the list of workspace members for single-member workspaces with a `[project]` section,
        //   to avoid cluttering the lockfile.
        let members: FxHashSet<&PackageId> = if lock.members().is_empty() {
            lock.root().into_iter().map(|package| &package.id).collect()
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

        // Identify any packages that are connected directly to the synthetic root node, i.e.,
        // requirements that are attached to the workspace itself.
        //
        // These include
        // - `[dependency-groups]` dependencies for workspaces whose roots do not include a
        //    `[project]` table, since those roots are not workspace members, but they _can_ define
        //    dependencies.
        // - `dependencies` in PEP 723 scripts.
        let root = graph.add_node(Node::Root);
        {
            // Index the lockfile by name.
            let by_name: FxHashMap<_, Vec<_>> = {
                lock.packages().iter().fold(
                    FxHashMap::with_capacity_and_hasher(lock.len(), FxBuildHasher),
                    |mut map, package| {
                        map.entry(&package.id.name).or_default().push(package);
                        map
                    },
                )
            };

            // Identify any requirements attached to the workspace itself.
            for requirement in lock.requirements() {
                for package in by_name.get(&requirement.name).into_iter().flatten() {
                    // Determine whether this entry is "relevant" for the requirement, by intersecting
                    // the markers.
                    let marker = if package.fork_markers.is_empty() {
                        requirement.marker
                    } else {
                        let mut combined = MarkerTree::FALSE;
                        for fork_marker in &package.fork_markers {
                            combined.or(fork_marker.pep508());
                        }
                        combined.and(requirement.marker);
                        combined
                    };
                    if marker.is_false() {
                        continue;
                    }
                    if markers.is_some_and(|markers| !marker.evaluate(markers, &[])) {
                        continue;
                    }
                    graph.add_edge(root, inverse[&package.id], Edge::Prod(None));
                }
            }

            // Identify any dependency groups attached to the workspace itself.
            for (group, requirements) in lock.dependency_groups() {
                for requirement in requirements {
                    for package in by_name.get(&requirement.name).into_iter().flatten() {
                        // Determine whether this entry is "relevant" for the requirement, by intersecting
                        // the markers.
                        let marker = if package.fork_markers.is_empty() {
                            requirement.marker
                        } else {
                            let mut combined = MarkerTree::FALSE;
                            for fork_marker in &package.fork_markers {
                                combined.or(fork_marker.pep508());
                            }
                            combined.and(requirement.marker);
                            combined
                        };
                        if marker.is_false() {
                            continue;
                        }
                        if markers.is_some_and(|markers| !marker.evaluate(markers, &[])) {
                            continue;
                        }
                        graph.add_edge(root, inverse[&package.id], Edge::Dev(group, None));
                    }
                }
            }
        }

        // Filter the graph to remove any unreachable nodes.
        {
            let mut reachable = graph
                .node_indices()
                .filter(|index| match graph[*index] {
                    Node::Package(package_id) => members.contains(package_id),
                    Node::Root => true,
                })
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

        // Reverse the graph.
        if invert {
            graph.reverse();
        }

        // Filter the graph to those nodes reachable from the target packages.
        if !packages.is_empty() {
            let mut reachable = graph
                .node_indices()
                .filter(|index| {
                    let Node::Package(package_id) = graph[*index] else {
                        return false;
                    };
                    packages.contains(&package_id.name)
                })
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

            // Find the root nodes: nodes with no incoming edges, or only an edge from the proxy.
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

        Self {
            graph,
            roots,
            latest,
            depth,
            no_dedupe,
        }
    }

    /// Perform a depth-first traversal of the given package and its dependencies.
    fn visit(
        &'env self,
        cursor: Cursor,
        visited: &mut FxHashMap<&'env PackageId, Vec<&'env PackageId>>,
        path: &mut Vec<&'env PackageId>,
    ) -> Vec<String> {
        // Short-circuit if the current path is longer than the provided depth.
        if path.len() > self.depth {
            return Vec::new();
        }

        let Node::Package(package_id) = self.graph[cursor.node()] else {
            return Vec::new();
        };
        let edge = cursor.edge().map(|edge_id| &self.graph[edge_id]);

        let line = {
            let mut line = format!("{}", package_id.name);

            if let Some(extras) = edge.and_then(Edge::extras) {
                if !extras.is_empty() {
                    line.push('[');
                    line.push_str(extras.iter().join(", ").as_str());
                    line.push(']');
                }
            }

            if let Some(version) = package_id.version.as_ref() {
                line.push(' ');
                line.push('v');
                line.push_str(&format!("{version}"));
            }

            if let Some(edge) = edge {
                match edge {
                    Edge::Prod(_) => {}
                    Edge::Optional(extra, _) => {
                        line.push_str(&format!(" (extra: {extra})"));
                    }
                    Edge::Dev(group, _) => {
                        line.push_str(&format!(" (group: {group})"));
                    }
                }
            }

            line
        };

        // Skip the traversal if:
        // 1. The package is in the current traversal path (i.e., a dependency cycle).
        // 2. The package has been visited and de-duplication is enabled (default).
        if let Some(requirements) = visited.get(package_id) {
            if !self.no_dedupe || path.contains(&package_id) {
                return if requirements.is_empty() {
                    vec![line]
                } else {
                    vec![format!("{line} (*)")]
                };
            }
        }

        // Incorporate the latest version of the package, if known.
        let line = if let Some(version) = self.latest.get(package_id) {
            format!("{line} {}", format!("(latest: v{version})").bold().cyan())
        } else {
            line
        };

        let mut dependencies = self
            .graph
            .edges_directed(cursor.node(), Direction::Outgoing)
            .filter_map(|edge| match self.graph[edge.target()] {
                Node::Root => None,
                Node::Package(_) => Some(Cursor::new(edge.target(), edge.id())),
            })
            .collect::<Vec<_>>();
        dependencies.sort_by_key(|cursor| {
            let node = &self.graph[cursor.node()];
            let edge = cursor
                .edge()
                .map(|edge_id| &self.graph[edge_id])
                .map(Edge::kind);
            (edge, node)
        });

        let mut lines = vec![line];

        // Keep track of the dependency path to avoid cycles.
        visited.insert(
            package_id,
            dependencies
                .iter()
                .filter_map(|node| match self.graph[node.node()] {
                    Node::Package(package_id) => Some(package_id),
                    Node::Root => None,
                })
                .collect(),
        );
        path.push(package_id);

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
        let mut path = Vec::new();
        let mut lines = Vec::with_capacity(self.graph.node_count());
        let mut visited =
            FxHashMap::with_capacity_and_hasher(self.graph.node_count(), FxBuildHasher);

        for node in &self.roots {
            match self.graph[*node] {
                Node::Root => {
                    for edge in self.graph.edges_directed(*node, Direction::Outgoing) {
                        let node = edge.target();
                        path.clear();
                        lines.extend(self.visit(
                            Cursor::new(node, edge.id()),
                            &mut visited,
                            &mut path,
                        ));
                    }
                }
                Node::Package(_) => {
                    path.clear();
                    lines.extend(self.visit(Cursor::root(*node), &mut visited, &mut path));
                }
            }
        }

        lines
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
enum Node<'env> {
    /// The synthetic root node.
    Root,
    /// A package in the dependency graph.
    Package(&'env PackageId),
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
enum Edge<'env> {
    Prod(Option<&'env BTreeSet<ExtraName>>),
    Optional(&'env ExtraName, Option<&'env BTreeSet<ExtraName>>),
    Dev(&'env GroupName, Option<&'env BTreeSet<ExtraName>>),
}

impl<'env> Edge<'env> {
    fn extras(&self) -> Option<&'env BTreeSet<ExtraName>> {
        match self {
            Self::Prod(extras) => *extras,
            Self::Optional(_, extras) => *extras,
            Self::Dev(_, extras) => *extras,
        }
    }

    fn kind(&self) -> EdgeKind<'env> {
        match self {
            Self::Prod(_) => EdgeKind::Prod,
            Self::Optional(extra, _) => EdgeKind::Optional(extra),
            Self::Dev(group, _) => EdgeKind::Dev(group),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
enum EdgeKind<'env> {
    Prod,
    Optional(&'env ExtraName),
    Dev(&'env GroupName),
}

/// A node in the dependency graph along with the edge that led to it, or `None` for root nodes.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
struct Cursor(NodeIndex, Option<EdgeIndex>);

impl Cursor {
    /// Create a [`Cursor`] representing a node in the dependency tree.
    fn new(node: NodeIndex, edge: EdgeIndex) -> Self {
        Self(node, Some(edge))
    }

    /// Create a [`Cursor`] representing a root node in the dependency tree.
    fn root(node: NodeIndex) -> Self {
        Self(node, None)
    }

    /// Return the [`NodeIndex`] of the node.
    fn node(&self) -> NodeIndex {
        self.0
    }

    /// Return the [`EdgeIndex`] of the edge that led to the node, if any.
    fn edge(&self) -> Option<EdgeIndex> {
        self.1
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

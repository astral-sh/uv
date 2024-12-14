use std::borrow::Cow;
use std::collections::VecDeque;

use itertools::Itertools;
use owo_colors::OwoColorize;
use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::prelude::EdgeRef;
use petgraph::Direction;
use rustc_hash::{FxHashMap, FxHashSet};

use uv_configuration::DevGroupsManifest;
use uv_normalize::{ExtraName, GroupName};
use uv_pypi_types::ResolverMarkerEnvironment;

use crate::lock::{Dependency, PackageId};
use crate::{Lock, PackageMap};

#[derive(Debug)]
pub struct LicenseDisplay<'env> {
    /// The constructed dependency graph.
    graph: petgraph::graph::Graph<&'env PackageId, Edge<'env>, petgraph::Directed>,
    /// The packages considered as roots of the dependency tree.
    roots: Vec<NodeIndex>,
    /// The discovered license data for each dependency
    license: &'env PackageMap<String>,
    /// Maximum display depth of the dependency tree.
    depth: usize,
}

impl<'env> LicenseDisplay<'env> {
    /// Create a new [`DisplayDependencyGraph`] for the set of installed packages.
    pub fn new(
        lock: &'env Lock,
        markers: Option<&'env ResolverMarkerEnvironment>,
        license: &'env PackageMap<String>,
        direct_only: bool,
        // packages: &[PackageName],
        dev: &DevGroupsManifest,
    ) -> Self {
        let depth = if direct_only { 1 } else { 255 };
        // Identify the workspace members.
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

        // Create a graph.
        let mut graph = petgraph::graph::Graph::<&PackageId, Edge, petgraph::Directed>::new();

        // Create the complete graph.
        let mut inverse = FxHashMap::default();
        for package in &lock.packages {
            // Insert the package into the graph.
            let package_node = if let Some(index) = inverse.get(&package.id) {
                *index
            } else {
                let index = graph.add_node(&package.id);
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
        }

        // Filter the graph to remove any unreachable nodes.
        {
            let mut reachable = graph
                .node_indices()
                .filter(|index| members.contains(graph[*index]))
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


        // // Filter the graph to those nodes reachable from the target packages.
        // if !packages.is_empty() {
        //     let mut reachable = graph
        //         .node_indices()
        //         .filter(|index| packages.contains(&graph[*index].name))
        //         .collect::<FxHashSet<_>>();
        //     let mut stack = reachable.iter().copied().collect::<VecDeque<_>>();
        //     while let Some(node) = stack.pop_front() {
        //         for edge in graph.edges_directed(node, Direction::Outgoing) {
        //             if reachable.insert(edge.target()) {
        //                 stack.push_back(edge.target());
        //             }
        //         }
        //     }

        //     // Remove the unreachable nodes from the graph.
        //     graph.retain_nodes(|_, index| reachable.contains(&index));
        // }

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

        Self {
            graph,
            roots,
            license,
            depth,
        }
    }

    /// Perform a depth-first traversal of the given package and its dependencies.
    fn visit(
        &'env self,
        cursor: Cursor,
        visited: &mut FxHashMap<&'env PackageId, Vec<&'env PackageId>>,
        path: &mut Vec<&'env PackageId>,
    ) -> Vec<String> {
        let unknown_license = String::from("Unknown License");
        // Short-circuit if the current path is longer than the provided depth.
        if path.len() > self.depth {
            return Vec::new();
        }

        let package_id = self.graph[cursor.node()];
        let edge = cursor.edge().map(|edge_id| &self.graph[edge_id]);

        if visited.contains_key(&package_id) {
            return vec![];
        }

        let line = {
            let mut line = format!("{}", package_id.name);

            if let Some(version) = package_id.version.as_ref() {
                line.push(' ');
                line.push('v');
                line.push_str(&format!("{version}"));
            }

            if let Some(edge) = edge {
                let extras = &edge.dependency().extra;
                if !extras.is_empty() {
                    line.push('[');
                    line.push_str(extras.iter().join(", ").as_str());
                    line.push(']');
                }
            }

            line.push(' ');
            line.push_str(self.license.get(package_id).unwrap_or_else(|| &unknown_license));

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
            if requirements.is_empty() {
                return vec![line]
            }
        }

        let mut dependencies = self
            .graph
            .edges_directed(cursor.node(), Direction::Outgoing)
            .map(|edge| {
                let node = edge.target();
                Cursor::new(node, edge.id())
            })
            .collect::<Vec<_>>();
        dependencies.sort_by_key(|node| {
            let package_id = self.graph[node.node()];
            let edge = node
                .edge()
                .map(|edge_id| &self.graph[edge_id])
                .map(Edge::kind);
            (edge, package_id)
        });

        let mut lines = vec![line];

        // Keep track of the dependency path to avoid cycles.
        visited.insert(
            package_id,
            dependencies
                .iter()
                .map(|node| self.graph[node.node()])
                .collect(),
        );
        path.push(package_id);

        for (_index, dep) in dependencies.iter().enumerate() {
            for (_visited_index, visited_line) in self.visit(*dep, visited, path).iter().enumerate()
            {
                lines.push(format!("{visited_line}"));
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
            FxHashMap::with_capacity_and_hasher(self.graph.node_count(), rustc_hash::FxBuildHasher);

        for node in &self.roots {
            path.clear();
            lines.extend(self.visit(Cursor::root(*node), &mut visited, &mut path));
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

impl std::fmt::Display for LicenseDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {

        for line in self.render() {
            writeln!(f, "{line}")?;
        }

        Ok(())
    }
}

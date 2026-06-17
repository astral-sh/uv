use std::collections::{BTreeSet, VecDeque};
use std::fmt::Write;

use either::Either;
use itertools::Itertools;
use owo_colors::OwoColorize;
use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::prelude::EdgeRef;
use petgraph::{Direction, Graph};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use serde::Serialize;

use uv_configuration::DependencyGroupsWithDefaults;
use uv_console::human_readable_bytes;
use uv_fs::PortablePath;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::Version;
use uv_pep508::MarkerTree;
use uv_pypi_types::ResolverMarkerEnvironment;

use crate::lock::{DirectSource, Package, PackageId, RegistrySource, Source};
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
    /// Whether the graph edges have been reversed (i.e., `--invert` mode).
    invert: bool,
    /// Reference to the lock to look up additional metadata (e.g., wheel sizes).
    lock: &'env Lock,
    /// Whether to show sizes in the rendered output.
    show_sizes: bool,
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
        groups: &DependencyGroupsWithDefaults,
        no_dedupe: bool,
        invert: bool,
        show_sizes: bool,
    ) -> Self {
        // Identify any workspace members.
        //
        // These include:
        // - The members listed in the lockfile.
        // - The root package, if it's not in the list of members. (The root package is omitted from
        //   the list of workspace members for single-member workspaces with a `[project]` section,
        //   to avoid cluttering the lockfile.
        let members: BTreeSet<&PackageId> = if lock.members().is_empty() {
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
        let size_guess = lock.packages.len();
        let mut graph =
            Graph::<Node, Edge, petgraph::Directed>::with_capacity(size_guess, size_guess);
        let mut inverse = FxHashMap::with_capacity_and_hasher(size_guess, FxBuildHasher);
        let mut queue: VecDeque<(&PackageId, Option<&ExtraName>)> = VecDeque::new();
        let mut seen = FxHashSet::default();

        let root = graph.add_node(Node::Root);

        // Add the root packages to the graph.
        for id in members.iter().copied() {
            if prune.contains(&id.name) {
                continue;
            }

            let dist = lock.find_by_id(id);

            // Add the workspace package to the graph. Under `--only-group`, the workspace member
            // may not be installed, but it's still relevant for the dependency tree, since we want
            // to show the connection from the workspace package to the enabled dependency groups.
            let index = *inverse
                .entry(id)
                .or_insert_with(|| graph.add_node(Node::Package(id)));

            // Add an edge from the root.
            graph.add_edge(root, index, Edge::Prod(None));

            if groups.prod() {
                // Push its dependencies on the queue.
                if seen.insert((id, None)) {
                    queue.push_back((id, None));
                }

                // Push any extras on the queue.
                for extra in dist.optional_dependencies.keys() {
                    if seen.insert((id, Some(extra))) {
                        queue.push_back((id, Some(extra)));
                    }
                }
            }

            // Add any development dependencies.
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
                if prune.contains(&dep.package_id.name) {
                    continue;
                }

                if markers
                    .is_some_and(|markers| !dep.complexified_marker.evaluate_no_extras(markers))
                {
                    continue;
                }

                // Add the dependency to the graph and get its index.
                let dep_index = *inverse
                    .entry(&dep.package_id)
                    .or_insert_with(|| graph.add_node(Node::Package(&dep.package_id)));

                // Add an edge from the workspace package.
                graph.add_edge(index, dep_index, Edge::Dev(group, Some(&dep.extra)));

                // Push its dependencies on the queue.
                if seen.insert((&dep.package_id, None)) {
                    queue.push_back((&dep.package_id, None));
                }
                for extra in &dep.extra {
                    if seen.insert((&dep.package_id, Some(extra))) {
                        queue.push_back((&dep.package_id, Some(extra)));
                    }
                }
            }
        }

        // Identify any packages that are connected directly to the synthetic root node, i.e.,
        // requirements that are attached to the workspace itself.
        //
        // These include
        // - `[dependency-groups]` dependencies for workspaces whose roots do not include a
        //    `[project]` table, since those roots are not workspace members, but they _can_ define
        //    dependencies.
        // - `dependencies` in PEP 723 scripts.
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
                    // Add the package to the graph.
                    let index = inverse
                        .entry(&package.id)
                        .or_insert_with(|| graph.add_node(Node::Package(&package.id)));

                    // Add an edge from the root.
                    graph.add_edge(root, *index, Edge::Prod(None));

                    // Push its dependencies on the queue.
                    if seen.insert((&package.id, None)) {
                        queue.push_back((&package.id, None));
                    }
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
                        // Add the package to the graph.
                        let index = inverse
                            .entry(&package.id)
                            .or_insert_with(|| graph.add_node(Node::Package(&package.id)));

                        // Add an edge from the root.
                        graph.add_edge(root, *index, Edge::Dev(group, None));

                        // Push its dependencies on the queue.
                        if seen.insert((&package.id, None)) {
                            queue.push_back((&package.id, None));
                        }
                    }
                }
            }
        }

        // Create all the relevant nodes.
        while let Some((id, extra)) = queue.pop_front() {
            let index = inverse[&id];
            let package = lock.find_by_id(id);

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
                if prune.contains(&dep.package_id.name) {
                    continue;
                }

                if markers
                    .is_some_and(|markers| !dep.complexified_marker.evaluate_no_extras(markers))
                {
                    continue;
                }

                // Add the dependency to the graph.
                let dep_index = *inverse
                    .entry(&dep.package_id)
                    .or_insert_with(|| graph.add_node(Node::Package(&dep.package_id)));

                // Add an edge from the workspace package.
                graph.add_edge(
                    index,
                    dep_index,
                    if let Some(extra) = extra {
                        Edge::Optional(extra, Some(&dep.extra))
                    } else {
                        Edge::Prod(Some(&dep.extra))
                    },
                );

                // Push its dependencies on the queue.
                if seen.insert((&dep.package_id, None)) {
                    queue.push_back((&dep.package_id, None));
                }
                for extra in &dep.extra {
                    if seen.insert((&dep.package_id, Some(extra))) {
                        queue.push_back((&dep.package_id, Some(extra)));
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
            // If specific packages were requested, use them as roots.
            if !packages.is_empty() {
                let mut roots = graph
                    .node_indices()
                    .filter(|index| {
                        let Node::Package(package_id) = graph[*index] else {
                            return false;
                        };
                        packages.contains(&package_id.name)
                    })
                    .collect::<Vec<_>>();

                // Sort the roots.
                roots.sort_by_key(|index| &graph[*index]);

                roots
            } else {
                let mut roots = if invert {
                    // For inverted trees, find leaf packages (nodes with no incoming
                    // edges).
                    graph
                        .node_indices()
                        .filter(|index| {
                            graph
                                .edges_directed(*index, Direction::Incoming)
                                .next()
                                .is_none()
                        })
                        .collect::<Vec<_>>()
                } else {
                    // For non-inverted trees, use the root node directly.
                    graph
                        .node_indices()
                        .filter(|index| matches!(graph[*index], Node::Root))
                        .collect::<Vec<_>>()
                };

                roots.sort_by_key(|index| &graph[*index]);
                roots
            }
        };

        Self {
            graph,
            roots,
            latest,
            depth,
            no_dedupe,
            invert,
            lock,
            show_sizes,
        }
    }

    /// Perform a depth-first traversal of the given package and its dependencies.
    fn visit(
        &'env self,
        cursor: Cursor,
        visited: &mut FxHashMap<VisitedNode<'env>, Vec<&'env PackageId>>,
        path: &mut Vec<VisitedNode<'env>>,
    ) -> Vec<String> {
        // Short-circuit if the current path is longer than the provided depth.
        if path.len() > self.depth {
            return Vec::new();
        }

        let Node::Package(package_id) = self.graph[cursor.node()] else {
            return Vec::new();
        };
        let edge = cursor.edge().map(|edge_id| &self.graph[edge_id]);
        let package = self.lock.find_by_id(package_id);

        let expanded_extras = self.expanded_extras(package, edge);
        let visited_node = VisitedNode {
            package_id,
            expanded_extras: expanded_extras.clone(),
        };

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
                let _ = write!(line, "{version}");
            }

            if let Some(edge) = edge {
                match edge {
                    Edge::Prod(_) => {}
                    Edge::Optional(extra, _) => {
                        let _ = write!(line, " (extra: {extra})");
                    }
                    Edge::Dev(group, _) => {
                        let _ = write!(line, " (group: {group})");
                    }
                }
            }

            // Append compressed wheel size, if available in the lockfile.
            // Keep it simple: use the first wheel entry that includes a size.
            if self.show_sizes {
                if let Some(size_bytes) = package.wheels.iter().find_map(|wheel| wheel.size) {
                    let (bytes, unit) = human_readable_bytes(size_bytes);
                    line.push(' ');
                    line.push_str(format!("{}", format!("({bytes:.1}{unit})").dimmed()).as_str());
                }
            }

            line
        };

        // Skip the traversal if:
        // 1. The package is in the current traversal path (i.e., a dependency cycle).
        // 2. The package has been visited and de-duplication is enabled (default).
        if path.contains(&visited_node) {
            return vec![format!("{line} (*)")];
        }
        if !self.no_dedupe
            && let Some(requirements) = visited.get(&visited_node)
        {
            return if requirements.is_empty() {
                vec![line]
            } else {
                vec![format!("{line} (*)")]
            };
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
                Node::Package(_) => {
                    // Only include extra-conditional dependencies if the activating extra is
                    // enabled in the current context.
                    if !self.invert
                        && let Edge::Optional(required_extra, _) = &self.graph[edge.id()]
                    {
                        if !expanded_extras.contains(required_extra) {
                            return None;
                        }
                    }
                    Some(Cursor::new(edge.target(), edge.id()))
                }
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
        // Only mark as visited if we're going to expand children (not at depth limit).
        if path.len() < self.depth {
            visited.insert(
                visited_node.clone(),
                dependencies
                    .iter()
                    .filter_map(|node| match self.graph[node.node()] {
                        Node::Package(package_id) => Some(package_id),
                        Node::Root => None,
                    })
                    .collect(),
            );
        }
        path.push(visited_node);

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

    /// Return the extras that can change this package's rendered child list.
    fn expanded_extras(
        &self,
        package: &'env Package,
        edge: Option<&Edge<'env>>,
    ) -> BTreeSet<&'env ExtraName> {
        if self.invert {
            // In inverted mode, optional edges are reverse "required by extra" relationships.
            // They do not select this package's outgoing dependencies, so de-dupe stays
            // package-only.
            return BTreeSet::default();
        }

        let Some(requested_extras) = edge.and_then(Edge::extras) else {
            // Roots are rendered with all optional dependency groups expanded.
            return package.optional_dependencies.keys().collect();
        };

        requested_extras
            .iter()
            .filter(|extra| package.optional_dependencies.contains_key(*extra))
            .collect()
    }

    /// Serialize the dependency graph in `NetworkX` node-link format.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&JsonGraph::from(self))
    }

    /// Return the shortest distance from any displayed root to every package within the requested
    /// depth.
    fn json_distances(&self) -> FxHashMap<NodeIndex, usize> {
        let mut distances = FxHashMap::default();
        let mut queue = VecDeque::new();

        for root in &self.roots {
            match self.graph[*root] {
                Node::Root => {
                    for edge in self.graph.edges_directed(*root, Direction::Outgoing) {
                        if matches!(self.graph[edge.target()], Node::Package(_))
                            && distances.insert(edge.target(), 0).is_none()
                        {
                            queue.push_back(edge.target());
                        }
                    }
                }
                Node::Package(_) => {
                    if distances.insert(*root, 0).is_none() {
                        queue.push_back(*root);
                    }
                }
            }
        }

        while let Some(source) = queue.pop_front() {
            let distance = distances[&source];
            if distance >= self.depth {
                continue;
            }

            for edge in self.graph.edges_directed(source, Direction::Outgoing) {
                let target = edge.target();
                if !matches!(self.graph[target], Node::Package(_))
                    || distances.contains_key(&target)
                {
                    continue;
                }
                distances.insert(target, distance + 1);
                queue.push_back(target);
            }
        }

        distances
    }
}

#[derive(Debug, Serialize)]
struct JsonGraph<'env> {
    directed: bool,
    multigraph: bool,
    graph: JsonGraphAttributes<'env>,
    nodes: Vec<JsonNode<'env>>,
    edges: Vec<JsonEdge<'env>>,
}

impl<'env> From<&TreeDisplay<'env>> for JsonGraph<'env> {
    fn from(tree: &TreeDisplay<'env>) -> Self {
        let distances = tree.json_distances();

        let mut package_nodes = distances.keys().copied().collect::<Vec<_>>();
        package_nodes.sort_by_key(|index| &tree.graph[*index]);

        let node_ids = package_nodes
            .iter()
            .enumerate()
            .map(|(id, index)| (*index, id))
            .collect::<FxHashMap<_, _>>();

        let nodes = package_nodes
            .iter()
            .map(|index| {
                let Node::Package(package_id) = tree.graph[*index] else {
                    unreachable!("JSON nodes only include packages");
                };
                let package = tree.lock.find_by_id(package_id);
                JsonNode {
                    id: node_ids[index],
                    name: &package_id.name,
                    version: package_id.version.as_ref(),
                    source: JsonSource::from(&package_id.source),
                    latest_version: tree.latest.get(package_id),
                    size_bytes: tree
                        .show_sizes
                        .then(|| package.wheels.iter().find_map(|wheel| wheel.size))
                        .flatten(),
                }
            })
            .collect();

        let mut roots = tree
            .roots
            .iter()
            .flat_map(|root| match tree.graph[*root] {
                Node::Root => tree
                    .graph
                    .edges_directed(*root, Direction::Outgoing)
                    .filter_map(|edge| {
                        node_ids
                            .get(&edge.target())
                            .map(|id| JsonRoot::new(*id, Some(edge.weight())))
                    })
                    .collect::<Vec<_>>(),
                Node::Package(_) => node_ids
                    .get(root)
                    .map(|id| vec![JsonRoot::new(*id, None)])
                    .unwrap_or_default(),
            })
            .collect::<Vec<_>>();
        roots.sort();
        roots.dedup();

        let mut graph_edges = tree
            .graph
            .edge_references()
            .filter(|edge| {
                distances
                    .get(&edge.source())
                    .is_some_and(|distance| *distance < tree.depth)
                    && node_ids.contains_key(&edge.source())
                    && node_ids.contains_key(&edge.target())
            })
            .collect::<Vec<_>>();
        graph_edges.sort_by(|left, right| {
            (
                node_ids[&left.source()],
                node_ids[&left.target()],
                left.weight(),
            )
                .cmp(&(
                    node_ids[&right.source()],
                    node_ids[&right.target()],
                    right.weight(),
                ))
        });

        let mut previous_endpoints = None;
        let mut key = 0;
        let edges = graph_edges
            .into_iter()
            .map(|edge| {
                let endpoints = (node_ids[&edge.source()], node_ids[&edge.target()]);
                if previous_endpoints == Some(endpoints) {
                    key += 1;
                } else {
                    previous_endpoints = Some(endpoints);
                    key = 0;
                }
                JsonEdge::new(endpoints.0, endpoints.1, key, edge.weight())
            })
            .collect();

        Self {
            directed: true,
            multigraph: true,
            graph: JsonGraphAttributes {
                schema: JsonSchema {
                    version: JsonSchemaVersion::Preview,
                },
                roots,
                inverted: tree.invert,
            },
            nodes,
            edges,
        }
    }
}

#[derive(Debug, Serialize)]
struct JsonGraphAttributes<'env> {
    schema: JsonSchema,
    roots: Vec<JsonRoot<'env>>,
    inverted: bool,
}

#[derive(Debug, Serialize)]
struct JsonSchema {
    version: JsonSchemaVersion,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum JsonSchemaVersion {
    Preview,
}

#[derive(Debug, Serialize)]
struct JsonNode<'env> {
    id: usize,
    name: &'env PackageName,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<&'env Version>,
    source: JsonSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_version: Option<&'env Version>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum JsonSource {
    Registry {
        registry: String,
    },
    Git {
        git: String,
    },
    Direct {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        subdirectory: Option<String>,
    },
    Path {
        path: String,
    },
    Directory {
        directory: String,
    },
    Editable {
        editable: String,
    },
    Virtual {
        r#virtual: String,
    },
}

impl From<&Source> for JsonSource {
    fn from(source: &Source) -> Self {
        match source {
            Source::Registry(source) => Self::Registry {
                registry: match source {
                    RegistrySource::Url(url) => url.as_ref().to_string(),
                    RegistrySource::Path(path) => PortablePath::from(path).to_string(),
                },
            },
            Source::Git(url, _) => Self::Git {
                git: url.as_ref().to_string(),
            },
            Source::Direct(url, DirectSource { subdirectory }) => Self::Direct {
                url: url.as_ref().to_string(),
                subdirectory: subdirectory
                    .as_deref()
                    .map(|path| PortablePath::from(path).to_string()),
            },
            Source::Path(path) => Self::Path {
                path: PortablePath::from(path).to_string(),
            },
            Source::Directory(path) => Self::Directory {
                directory: PortablePath::from(path).to_string(),
            },
            Source::Editable(path) => Self::Editable {
                editable: PortablePath::from(path).to_string(),
            },
            Source::Virtual(path) => Self::Virtual {
                r#virtual: PortablePath::from(path).to_string(),
            },
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct JsonRoot<'env> {
    id: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<JsonEdgeKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extra: Option<&'env ExtraName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<&'env GroupName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    requested_extras: Option<Vec<&'env ExtraName>>,
}

impl<'env> JsonRoot<'env> {
    fn new(id: usize, edge: Option<&Edge<'env>>) -> Self {
        let kind = match edge {
            None | Some(Edge::Prod(_)) => None,
            Some(Edge::Optional(_, _)) => Some(JsonEdgeKind::Optional),
            Some(Edge::Dev(_, _)) => Some(JsonEdgeKind::Development),
        };
        let extra = match edge {
            Some(Edge::Optional(extra, _)) => Some(*extra),
            None | Some(Edge::Prod(_) | Edge::Dev(_, _)) => None,
        };
        let group = match edge {
            Some(Edge::Dev(group, _)) => Some(*group),
            None | Some(Edge::Prod(_) | Edge::Optional(_, _)) => None,
        };
        let requested_extras = edge
            .and_then(Edge::extras)
            .filter(|extras| !extras.is_empty())
            .map(|extras| extras.iter().collect());
        Self {
            id,
            kind,
            extra,
            group,
            requested_extras,
        }
    }
}

#[derive(Debug, Serialize)]
struct JsonEdge<'env> {
    source: usize,
    target: usize,
    key: usize,
    kind: JsonEdgeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    extra: Option<&'env ExtraName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<&'env GroupName>,
    requested_extras: Vec<&'env ExtraName>,
}

impl<'env> JsonEdge<'env> {
    fn new(source: usize, target: usize, key: usize, edge: &Edge<'env>) -> Self {
        let (kind, extra, group) = match edge {
            Edge::Prod(_) => (JsonEdgeKind::Production, None, None),
            Edge::Optional(extra, _) => (JsonEdgeKind::Optional, Some(*extra), None),
            Edge::Dev(group, _) => (JsonEdgeKind::Development, None, Some(*group)),
        };
        Self {
            source,
            target,
            key,
            kind,
            extra,
            group,
            requested_extras: edge.extras().into_iter().flatten().collect::<Vec<_>>(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum JsonEdgeKind {
    Production,
    Optional,
    Development,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct VisitedNode<'env> {
    package_id: &'env PackageId,
    expanded_extras: BTreeSet<&'env ExtraName>,
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

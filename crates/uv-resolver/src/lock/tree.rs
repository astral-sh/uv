use std::collections::{BTreeSet, VecDeque};

use either::Either;
use itertools::Itertools;
use owo_colors::OwoColorize;
use serde_json::json;
use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::prelude::EdgeRef;
use petgraph::{Direction, Graph};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use uv_configuration::DependencyGroupsWithDefaults;
use uv_console::human_readable_bytes;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::Version;
use uv_pep508::MarkerTree;
use uv_pypi_types::ResolverMarkerEnvironment;

use crate::lock::PackageId;
use crate::{Lock, PackageMap};

/// Information about a node in the dependency tree.
#[derive(Debug, Clone)]
struct NodeInfo<'env> {
    /// The package identifier.
    package_id: &'env PackageId,
    /// The package version, if available.
    version: Option<&'env Version>,
    /// The extras associated with this dependency edge.
    extras: Option<&'env BTreeSet<ExtraName>>,
    /// The type of dependency edge that led to this node.
    edge_type: Option<EdgeType<'env>>,
    /// The compressed wheel size in bytes, if available.
    size: Option<u64>,
    /// The latest available version of this package, if known.
    latest_version: Option<&'env Version>,
}

/// The type of dependency edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeType<'env> {
    /// A production dependency.
    Prod,
    /// An optional dependency (from an extra).
    Optional(&'env ExtraName),
    /// A development dependency (from a dependency group).
    Dev(&'env GroupName),
}

/// Information about a node's position in the tree.
///
/// This struct provides complete positional information for formatters.
/// While not all fields may be used by every formatter (e.g., TextFormatter
/// primarily uses `depth` and `is_last_child`), they are available for
/// formatters that need more detailed position information.
#[derive(Debug, Clone, Copy)]
struct NodePosition {
    /// The depth of this node in the tree (0 = root).
    depth: usize,
    /// Whether this is the first child of its parent.
    /// Useful for formatters that want to apply special styling to first children.
    #[allow(dead_code)]
    is_first_child: bool,
    /// Whether this is the last child of its parent.
    /// Used by TextFormatter to determine tree connector characters.
    is_last_child: bool,
    /// The index of this child among its siblings (0-based).
    /// Useful for formatters that want to number or identify children.
    #[allow(dead_code)]
    child_index: usize,
    /// The total number of siblings (including this node).
    /// Useful for formatters that want to show "X of Y" information.
    #[allow(dead_code)]
    total_siblings: usize,
}

/// A trait for formatting dependency trees in different output formats.
///
/// This trait uses the visitor pattern to emit structural events as the tree
/// is traversed. Implementors can choose how to represent these events in their
/// output format (e.g., text with tree characters, JSON, HTML, etc.).
trait TreeFormatter {
    /// The type of output produced by this formatter.
    type Output;

    /// Called once at the beginning of tree rendering.
    fn begin_tree(&mut self);

    /// Called once at the end of tree rendering, returning the final output.
    fn end_tree(&mut self) -> Self::Output;

    /// Called when entering a node in the tree.
    ///
    /// # Arguments
    /// * `info` - Information about the package and its relationship to the parent
    /// * `position` - Information about the node's position in the tree structure
    fn begin_node(&mut self, info: &NodeInfo, position: NodePosition);

    /// Called when leaving a node in the tree.
    fn end_node(&mut self);

    /// Called to indicate that a node has been visited before (deduplicated).
    ///
    /// This is called when a package appears multiple times in the tree but
    /// deduplication is enabled, so its subtree won't be expanded again.
    fn mark_visited(&mut self);

    /// Called to indicate that a node creates a dependency cycle.
    ///
    /// This is called when a package appears in its own dependency path,
    /// creating a cycle that cannot be fully displayed.
    fn mark_cycle(&mut self);

    /// Called before visiting the children of a node.
    ///
    /// # Arguments
    /// * `count` - The number of children that will be visited
    fn begin_children(&mut self, count: usize);

    /// Called after all children of a node have been visited.
    fn end_children(&mut self);
}

/// A text-based tree formatter that produces lines with box-drawing characters.
///
/// This formatter produces output like:
/// ```text
/// package-name v1.0.0
/// ├── dependency-1 v2.0.0
/// │   └── nested-dep v3.0.0
/// └── dependency-2 v1.5.0 (*)
/// ```
#[derive(Debug)]
struct TextFormatter {
    /// The accumulated output lines.
    lines: Vec<String>,
    /// Stack of indentation strings for the current path in the tree.
    /// Each element represents the prefix for one level of depth.
    indent_stack: Vec<&'static str>,
    /// Whether any deduplicated packages have been encountered.
    has_deduped: bool,
    /// Whether any cycles have been encountered.
    has_cycles: bool,
    /// Whether deduplication is disabled (for determining the meaning of (*)).
    no_dedupe: bool,
}

impl TextFormatter {
    /// Create a new text formatter.
    ///
    /// # Arguments
    /// * `no_dedupe` - If true, (*) markers indicate cycles; if false, they indicate deduplication.
    fn new(no_dedupe: bool) -> Self {
        Self {
            lines: Vec::new(),
            indent_stack: Vec::new(),
            has_deduped: false,
            has_cycles: false,
            no_dedupe,
        }
    }
}

impl TreeFormatter for TextFormatter {
    type Output = Vec<String>;

    fn begin_tree(&mut self) {
        // Nothing to do for text output
    }

    fn end_tree(&mut self) -> Self::Output {
        // Add explanation footer if we saw any markers
        if self.has_deduped || self.has_cycles {
            let message = if self.no_dedupe {
                "(*) Package tree is a cycle and cannot be shown".italic()
            } else {
                "(*) Package tree already displayed".italic()
            };
            self.lines.push(format!("{message}"));
        }

        std::mem::take(&mut self.lines)
    }

    fn begin_node(&mut self, info: &NodeInfo, position: NodePosition) {
        // Build the line prefix (indentation + tree connector)
        let prefix = if position.depth == 0 {
            // Root level - no prefix
            String::new()
        } else {
            // Build indentation from the stack
            let mut prefix = self.indent_stack.join("");

            // Add the tree connector for this node
            let connector = if position.is_last_child {
                "└── "
            } else {
                "├── "
            };
            prefix.push_str(connector);
            prefix
        };

        // Build the line content
        let mut line = format!("{}{}", prefix, info.package_id.name);

        // Add extras if present
        if let Some(extras) = info.extras {
            if !extras.is_empty() {
                line.push('[');
                line.push_str(&extras.iter().join(", "));
                line.push(']');
            }
        }

        // Add version if present
        if let Some(version) = info.version {
            line.push_str(&format!(" v{version}"));
        }

        // Add edge type annotation
        if let Some(edge_type) = info.edge_type {
            match edge_type {
                EdgeType::Prod => {}
                EdgeType::Optional(extra) => {
                    line.push_str(&format!(" (extra: {extra})"));
                }
                EdgeType::Dev(group) => {
                    line.push_str(&format!(" (group: {group})"));
                }
            }
        }

        // Add size if present
        if let Some(size) = info.size {
            let (bytes, unit) = human_readable_bytes(size);
            line.push(' ');
            line.push_str(&format!("{}", format!("({bytes:.1}{unit})").dimmed()));
        }

        // Add latest version if known and outdated
        if let Some(latest) = info.latest_version {
            line.push_str(&format!(" {}", format!("(latest: v{latest})").bold().cyan()));
        }

        self.lines.push(line);

        // Update indent stack for potential children
        if position.depth > 0 {
            let indent = if position.is_last_child {
                "    "
            } else {
                "│   "
            };
            self.indent_stack.push(indent);
        }
    }

    fn end_node(&mut self) {
        // Pop the indent added by begin_node
        if !self.indent_stack.is_empty() {
            self.indent_stack.pop();
        }
    }

    fn mark_visited(&mut self) {
        self.has_deduped = true;
        // Append marker to the last line
        if let Some(last) = self.lines.last_mut() {
            last.push_str(" (*)");
        }
    }

    fn mark_cycle(&mut self) {
        self.has_cycles = true;
        // Append marker to the last line
        if let Some(last) = self.lines.last_mut() {
            last.push_str(" (*)");
        }
    }

    fn begin_children(&mut self, _count: usize) {
        // Nothing to do for text output
    }

    fn end_children(&mut self) {
        // Nothing to do for text output
    }
}

/// A JSON tree formatter that produces structured JSON output.
///
/// This formatter produces output like:
/// ```json
/// {
///   "name": "package-name",
///   "version": "1.0.0",
///   "dependencies": [
///     {
///       "name": "dependency-1",
///       "version": "2.0.0",
///       "dependencies": []
///     }
///   ]
/// }
/// ```
#[derive(Debug)]
struct JsonFormatter {
    /// Stack of JSON objects being built.
    /// The top of the stack is the current node being processed.
    stack: Vec<serde_json::Value>,
    /// The root nodes (top-level packages).
    roots: Vec<serde_json::Value>,
}

impl JsonFormatter {
    /// Create a new JSON formatter.
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            roots: Vec::new(),
        }
    }
}

impl TreeFormatter for JsonFormatter {
    type Output = serde_json::Value;

    fn begin_tree(&mut self) {
        // Nothing to do for JSON output
    }

    fn end_tree(&mut self) -> Self::Output {
        // Return all roots as a JSON array
        json!(self.roots)
    }

    fn begin_node(&mut self, info: &NodeInfo, _position: NodePosition) {
        // Create a JSON object for this node
        let mut node = json!({
            "name": info.package_id.name.to_string(),
        });

        // Add optional fields
        if let Some(version) = info.version {
            node["version"] = json!(version.to_string());
        }

        if let Some(extras) = info.extras {
            if !extras.is_empty() {
                node["extras"] = json!(extras.iter().map(|e| e.to_string()).collect::<Vec<_>>());
            }
        }

        if let Some(edge_type) = info.edge_type {
            match edge_type {
                EdgeType::Optional(extra) => {
                    node["extra"] = json!(extra.to_string());
                }
                EdgeType::Dev(group) => {
                    node["group"] = json!(group.to_string());
                }
                EdgeType::Prod => {}
            }
        }

        if let Some(size) = info.size {
            node["size"] = json!(size);
        }

        if let Some(latest) = info.latest_version {
            node["latest"] = json!(latest.to_string());
        }

        // Initialize empty dependencies array
        node["dependencies"] = json!([]);

        // Push onto stack
        self.stack.push(node);
    }

    fn end_node(&mut self) {
        // Pop the current node from the stack
        let node = self.stack.pop().expect("Stack should not be empty");

        if self.stack.is_empty() {
            // This is a root node - add to roots
            self.roots.push(node);
        } else {
            // This is a child node - add to parent's dependencies
            let parent = self.stack.last_mut().expect("Parent should exist");
            parent["dependencies"]
                .as_array_mut()
                .expect("Dependencies should be an array")
                .push(node);
        }
    }

    fn mark_visited(&mut self) {
        // Mark the current node as deduplicated
        if let Some(node) = self.stack.last_mut() {
            node["deduplicated"] = json!(true);
        }
    }

    fn mark_cycle(&mut self) {
        // Mark the current node as a cycle
        if let Some(node) = self.stack.last_mut() {
            node["cycle"] = json!(true);
        }
    }

    fn begin_children(&mut self, _count: usize) {
        // Nothing to do for JSON output
    }

    fn end_children(&mut self) {
        // Nothing to do for JSON output
    }
}

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
            }
        };

        Self {
            graph,
            roots,
            latest,
            depth,
            no_dedupe,
            lock,
            show_sizes,
        }
    }

    /// Extract node information for the tree formatter.
    fn extract_node_info(&self, cursor: Cursor, package_id: &'env PackageId) -> NodeInfo<'env> {
        let edge = cursor.edge().map(|edge_id| &self.graph[edge_id]);

        // Extract edge type and extras
        let (edge_type, extras) = match edge {
            Some(Edge::Prod(ex)) => (Some(EdgeType::Prod), *ex),
            Some(Edge::Optional(extra, ex)) => (Some(EdgeType::Optional(extra)), *ex),
            Some(Edge::Dev(group, ex)) => (Some(EdgeType::Dev(group)), *ex),
            None => (None, None),
        };

        // Extract size if enabled
        let size = if self.show_sizes {
            let package = self.lock.find_by_id(package_id);
            package.wheels.iter().find_map(|wheel| wheel.size)
        } else {
            None
        };

        // Extract latest version if known and outdated
        let latest_version = self.latest.get(package_id);

        NodeInfo {
            package_id,
            version: package_id.version.as_ref(),
            extras,
            edge_type,
            size,
            latest_version,
        }
    }

    /// Perform a depth-first traversal using the provided formatter.
    fn visit_with_formatter<F: TreeFormatter>(
        &'env self,
        cursor: Cursor,
        formatter: &mut F,
        visited: &mut FxHashMap<&'env PackageId, Vec<&'env PackageId>>,
        path: &mut Vec<&'env PackageId>,
        position: NodePosition,
    ) {
        // Short-circuit if the current path is longer than the provided depth.
        if position.depth > self.depth {
            return;
        }

        // Extract package information
        let Node::Package(package_id) = self.graph[cursor.node()] else {
            return;
        };

        // Extract node information
        let info = self.extract_node_info(cursor, package_id);

        // Emit begin_node event
        formatter.begin_node(&info, position);

        // Check if we've visited this package before
        if let Some(requirements) = visited.get(package_id) {
            // Determine if this is a cycle or just dedupe
            let is_cycle = path.contains(&package_id);

            if !self.no_dedupe || is_cycle {
                // Mark as visited/cycle and return early
                if is_cycle {
                    formatter.mark_cycle();
                } else if !requirements.is_empty() {
                    formatter.mark_visited();
                }
                formatter.end_node();
                return;
            }
        }

        // Get and sort dependencies
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

        // Track this visit
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

        // Emit begin_children event
        formatter.begin_children(dependencies.len());

        // Recursively visit children
        for (index, dep) in dependencies.iter().enumerate() {
            let child_position = NodePosition {
                depth: position.depth + 1,
                is_first_child: index == 0,
                is_last_child: index == dependencies.len() - 1,
                child_index: index,
                total_siblings: dependencies.len(),
            };
            self.visit_with_formatter(*dep, formatter, visited, path, child_position);
        }

        // Emit end_children event
        formatter.end_children();

        path.pop();

        // Emit end_node event
        formatter.end_node();
    }

    /// Perform a depth-first traversal of the given package and its dependencies.
    ///
    /// This is a backward-compatible wrapper around `visit_with_formatter` that uses
    /// the TextFormatter to produce the same output as before the refactoring.
    ///
    /// Note: This method is kept for potential external users or future compatibility,
    /// even though it's not currently used within this module.
    #[allow(dead_code)]
    fn visit(
        &'env self,
        cursor: Cursor,
        visited: &mut FxHashMap<&'env PackageId, Vec<&'env PackageId>>,
        path: &mut Vec<&'env PackageId>,
    ) -> Vec<String> {
        // Create a temporary formatter to collect output for this subtree
        let mut formatter = TextFormatter::new(self.no_dedupe);
        formatter.begin_tree();

        // Determine the depth based on current path length
        let depth = path.len();

        // Create position for the root of this subtree
        let position = NodePosition {
            depth,
            is_first_child: false,
            is_last_child: false,
            child_index: 0,
            total_siblings: 1,
        };

        // Visit using the formatter
        self.visit_with_formatter(cursor, &mut formatter, visited, path, position);

        // Return the lines (without calling end_tree, as we don't want the footer here)
        formatter.lines
    }

    /// Depth-first traverse the nodes to render the tree.
    fn render(&self) -> Vec<String> {
        let mut path = Vec::new();
        let mut visited =
            FxHashMap::with_capacity_and_hasher(self.graph.node_count(), FxBuildHasher);
        let mut formatter = TextFormatter::new(self.no_dedupe);

        formatter.begin_tree();

        for node in &self.roots {
            match self.graph[*node] {
                Node::Root => {
                    let edges: Vec<_> = self.graph.edges_directed(*node, Direction::Outgoing).collect();
                    let total_siblings = edges.len();
                    for (index, edge) in edges.into_iter().enumerate() {
                        let node = edge.target();
                        path.clear();
                        let position = NodePosition {
                            depth: 0,
                            is_first_child: index == 0,
                            is_last_child: index == total_siblings - 1,
                            child_index: index,
                            total_siblings,
                        };
                        self.visit_with_formatter(
                            Cursor::new(node, edge.id()),
                            &mut formatter,
                            &mut visited,
                            &mut path,
                            position,
                        );
                    }
                }
                Node::Package(_) => {
                    path.clear();
                    let position = NodePosition {
                        depth: 0,
                        is_first_child: true,
                        is_last_child: true,
                        child_index: 0,
                        total_siblings: 1,
                    };
                    self.visit_with_formatter(
                        Cursor::root(*node),
                        &mut formatter,
                        &mut visited,
                        &mut path,
                        position,
                    );
                }
            }
        }

        formatter.end_tree()
    }

    /// Depth-first traverse the nodes to render the tree as JSON.
    pub fn render_json(&self) -> serde_json::Value {
        let mut path = Vec::new();
        let mut visited =
            FxHashMap::with_capacity_and_hasher(self.graph.node_count(), FxBuildHasher);
        let mut formatter = JsonFormatter::new();

        formatter.begin_tree();

        for node in &self.roots {
            match self.graph[*node] {
                Node::Root => {
                    let edges: Vec<_> = self.graph.edges_directed(*node, Direction::Outgoing).collect();
                    let total_siblings = edges.len();
                    for (index, edge) in edges.into_iter().enumerate() {
                        let node = edge.target();
                        path.clear();
                        let position = NodePosition {
                            depth: 0,
                            is_first_child: index == 0,
                            is_last_child: index == total_siblings - 1,
                            child_index: index,
                            total_siblings,
                        };
                        self.visit_with_formatter(
                            Cursor::new(node, edge.id()),
                            &mut formatter,
                            &mut visited,
                            &mut path,
                            position,
                        );
                    }
                }
                Node::Package(_) => {
                    path.clear();
                    let position = NodePosition {
                        depth: 0,
                        is_first_child: true,
                        is_last_child: true,
                        child_index: 0,
                        total_siblings: 1,
                    };
                    self.visit_with_formatter(
                        Cursor::root(*node),
                        &mut formatter,
                        &mut visited,
                        &mut path,
                        position,
                    );
                }
            }
        }

        formatter.end_tree()
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
        // Render using the text formatter (which includes the footer)
        for line in self.render() {
            writeln!(f, "{line}")?;
        }

        Ok(())
    }
}

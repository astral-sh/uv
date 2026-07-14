use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write;
use std::path::Path;

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
use uv_fs::PortablePathBuf;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::Version;
use uv_pep508::MarkerTree;
use uv_pypi_types::ResolverMarkerEnvironment;

use crate::lock::export::{
    MetadataNode, MetadataNodeId, MetadataNodeKind, MetadataScript, MetadataWorkspace,
    MetadataWorkspaceMember,
};
use crate::lock::{Package, PackageId};
use crate::{ConflictMarker, Lock, PackageMap, UniversalMarker};

#[derive(Debug, Clone, Copy)]
pub enum TreeJsonTarget<'a> {
    Workspace(&'a Path),
    Script(&'a Path),
}

impl<'a> TreeJsonTarget<'a> {
    fn root(self) -> &'a Path {
        match self {
            Self::Workspace(root) => root,
            Self::Script(script) => script.parent().unwrap_or_else(|| Path::new("")),
        }
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
    /// Whether the graph edges have been reversed (i.e., `--invert` mode).
    invert: bool,
    /// Whether production dependencies are included in the tree.
    prod: bool,
    /// The dependency groups included in the tree.
    groups: DependencyGroupsWithDefaults,
    /// Reference to the lock to look up additional metadata (e.g., wheel sizes).
    lock: &'env Lock,
    /// Whether to show sizes in the rendered output.
    show_sizes: bool,
    /// The marker constraints imposed by declared conflicting extras and groups.
    conflict_marker: UniversalMarker,
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

        // Conflict extras and groups are encoded as marker expressions. Include the declared
        // mutual-exclusion constraints when checking whether a universal path is satisfiable.
        let conflict_marker = UniversalMarker::new(
            MarkerTree::TRUE,
            ConflictMarker::from_conflicts(lock.conflicts()),
        );

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
            graph.add_edge(root, index, Edge::Prod(None, UniversalMarker::TRUE));

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
                graph.add_edge(
                    index,
                    dep_index,
                    Edge::Dev(
                        group,
                        Some(RequestedExtras::Dependency(&dep.extra)),
                        dep.complexified_marker,
                    ),
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
                    graph.add_edge(
                        root,
                        *index,
                        Edge::Prod(
                            Some(RequestedExtras::Requirement(requirement.extras.as_ref())),
                            UniversalMarker::from_combined(marker),
                        ),
                    );

                    // Push its dependencies on the queue.
                    if seen.insert((&package.id, None)) {
                        queue.push_back((&package.id, None));
                    }
                    for extra in &*requirement.extras {
                        if seen.insert((&package.id, Some(extra))) {
                            queue.push_back((&package.id, Some(extra)));
                        }
                    }
                }
            }

            // Identify any dependency groups attached to the workspace itself.
            for (group, requirements) in lock.dependency_groups() {
                if !groups.contains(group) {
                    continue;
                }
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
                        graph.add_edge(
                            root,
                            *index,
                            Edge::Dev(
                                group,
                                Some(RequestedExtras::Requirement(requirement.extras.as_ref())),
                                UniversalMarker::from_combined(marker),
                            ),
                        );

                        // Push its dependencies on the queue.
                        if seen.insert((&package.id, None)) {
                            queue.push_back((&package.id, None));
                        }
                        for extra in &*requirement.extras {
                            if seen.insert((&package.id, Some(extra))) {
                                queue.push_back((&package.id, Some(extra)));
                            }
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
                        Edge::Optional(
                            extra,
                            Some(RequestedExtras::Dependency(&dep.extra)),
                            dep.complexified_marker,
                        )
                    } else {
                        Edge::Prod(
                            Some(RequestedExtras::Dependency(&dep.extra)),
                            dep.complexified_marker,
                        )
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
            prod: groups.prod(),
            groups: groups.clone(),
            lock,
            show_sizes,
            conflict_marker,
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
            marker: self.invert.then_some(cursor.marker()),
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
                    Edge::Prod(..) => {}
                    Edge::Optional(extra, ..) => {
                        let _ = write!(line, " (extra: {extra})");
                    }
                    Edge::Dev(group, ..) => {
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

        let mut dependencies = if self.invert && edge.is_some_and(Edge::is_dev) {
            // A member's dependency group is activated for the root member. It is not part of the
            // member when that member is installed as another package's dependency.
            Vec::new()
        } else {
            self.graph
                .edges_directed(cursor.node(), Direction::Outgoing)
                .filter_map(|edge| match self.graph[edge.target()] {
                    Node::Root => None,
                    Node::Package(_) => {
                        let edge_kind = &self.graph[edge.id()];

                        if self.invert {
                            // If the path to the target requires an extra on this package, only
                            // follow consumers that activate that extra.
                            if !expanded_extras.is_empty()
                                && edge_kind.extras().is_none_or(|extras| {
                                    !expanded_extras.iter().all(|extra| extras.contains(extra))
                                })
                            {
                                return None;
                            }

                            // A package node can appear in several universal marker branches. Do
                            // not join incoming and outgoing edges that cannot coexist.
                            let mut marker = cursor.marker();
                            marker.and(edge_kind.marker());
                            if marker.is_false() {
                                return None;
                            }
                            Some(Cursor::new(edge.target(), edge.id(), marker))
                        } else {
                            // Only include extra-conditional dependencies if the activating extra
                            // is enabled in the current context.
                            if let Edge::Optional(required_extra, ..) = edge_kind
                                && !expanded_extras.contains(required_extra)
                            {
                                return None;
                            }
                            Some(Cursor::new(edge.target(), edge.id(), UniversalMarker::TRUE))
                        }
                    }
                })
                .collect::<Vec<_>>()
        };
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
                            Cursor::new(node, edge.id(), self.conflict_marker),
                            &mut visited,
                            &mut path,
                        ));
                    }
                }
                Node::Package(_) => {
                    path.clear();
                    lines.extend(self.visit(
                        Cursor::root(*node, self.conflict_marker),
                        &mut visited,
                        &mut path,
                    ));
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
            // In inverted mode, an optional edge records the extra that must have been activated
            // on this package for the path to exist.
            return edge.and_then(Edge::required_extra).into_iter().collect();
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

    /// Serialize the displayed dependency graph as JSON.
    pub fn to_json(&self, target: TreeJsonTarget<'_>) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&JsonGraph::new(self, target))
    }

    /// Return the packages and edges reachable from the displayed roots within the requested
    /// depth.
    ///
    /// Depth follows the text tree's package-graph semantics. The targets of edges from
    /// [`Node::Root`] start at depth zero; the synthetic root itself does not consume a level.
    /// JSON subsequently represents a script or workspace-owned dependency group as an explicit
    /// node, so its direct requirements remain at depth zero despite appearing one edge away from
    /// a root in the serialized graph. Structural extra-to-package relationships likewise do not
    /// participate in depth traversal.
    fn json_traversal(&self) -> JsonTraversal {
        let mut distances = FxHashMap::default();
        let mut queue = VecDeque::new();
        let mut nodes = FxHashSet::default();
        let mut edges = FxHashSet::default();

        for root in &self.roots {
            match self.graph[*root] {
                Node::Root => {
                    for edge in self.graph.edges_directed(*root, Direction::Outgoing) {
                        let Node::Package(package_id) = self.graph[edge.target()] else {
                            continue;
                        };
                        let state = JsonTraversalNode {
                            index: edge.target(),
                            expanded_extras: self.expanded_extras(
                                self.lock.find_by_id(package_id),
                                Some(edge.weight()),
                            ),
                            marker: UniversalMarker::TRUE,
                            reached_via_dependency_group: false,
                        };
                        nodes.insert(state.index);
                        if distances.insert(state.clone(), 0).is_none() {
                            queue.push_back(state);
                        }
                    }
                }
                Node::Package(package_id) => {
                    let state = JsonTraversalNode {
                        index: *root,
                        expanded_extras: self
                            .expanded_extras(self.lock.find_by_id(package_id), None),
                        marker: if self.invert {
                            self.conflict_marker
                        } else {
                            UniversalMarker::TRUE
                        },
                        reached_via_dependency_group: false,
                    };
                    nodes.insert(state.index);
                    if distances.insert(state.clone(), 0).is_none() {
                        queue.push_back(state);
                    }
                }
            }
        }

        while let Some(source) = queue.pop_front() {
            let distance = distances[&source];
            if distance >= self.depth || self.invert && source.reached_via_dependency_group {
                continue;
            }

            for edge in self.graph.edges_directed(source.index, Direction::Outgoing) {
                let edge_kind = edge.weight();
                let marker = if self.invert {
                    // If the path to the target requires an extra on this package, only follow
                    // consumers that activate that extra.
                    if !source.expanded_extras.is_empty()
                        && edge_kind.extras().is_none_or(|extras| {
                            !source
                                .expanded_extras
                                .iter()
                                .all(|extra| extras.contains(extra))
                        })
                    {
                        continue;
                    }

                    // Do not join incoming and outgoing edges that cannot coexist in the same
                    // universal marker environment.
                    let mut marker = source.marker;
                    marker.and(edge_kind.marker());
                    if marker.is_false() {
                        continue;
                    }
                    marker
                } else {
                    // Only include extra-conditional dependencies if the activating extra is
                    // enabled in the current context.
                    if let Edge::Optional(required_extra, ..) = edge_kind
                        && !source.expanded_extras.contains(required_extra)
                    {
                        continue;
                    }
                    UniversalMarker::TRUE
                };

                let target = edge.target();
                if matches!(self.graph[target], Node::Root) {
                    edges.insert(edge.id());
                    continue;
                }
                let Node::Package(package_id) = self.graph[target] else {
                    continue;
                };
                let state = JsonTraversalNode {
                    index: target,
                    expanded_extras: self
                        .expanded_extras(self.lock.find_by_id(package_id), Some(edge.weight())),
                    marker,
                    reached_via_dependency_group: self.invert && edge_kind.is_dev(),
                };
                nodes.insert(state.index);
                edges.insert(edge.id());
                if !distances.contains_key(&state) {
                    distances.insert(state.clone(), distance + 1);
                    queue.push_back(state);
                }
            }
        }

        JsonTraversal { nodes, edges }
    }
}

#[derive(Debug)]
struct JsonTraversal {
    nodes: FxHashSet<NodeIndex>,
    edges: FxHashSet<EdgeIndex>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct JsonTraversalNode<'env> {
    index: NodeIndex,
    expanded_extras: BTreeSet<&'env ExtraName>,
    marker: UniversalMarker,
    reached_via_dependency_group: bool,
}

/// A JSON representation of the output of `uv tree`.
///
/// The core format is the one from `uv workspace metadata` ([`crate::lock::export::Metadata`]),
/// because they're representing essentially the same data (the resolved dependency graph).
///
/// The two formats most notably diverge in what can or can't be roots, because `uv tree`
/// supports filtering and inverting the graph. So while `metadata` has fixed "members",
/// "workspace", and "script" entry points, `tree` needs to cope with filters
/// and inversion making random nodes roots (that said, having workspace/member/script entries
/// is still useful for quickly identifying those special kinds of node in the graph).
/// (As with metadata it's discouraged for you to just iterate the `resolution`: you should
/// start at a given entry point and traverse the graph from there.)
///
/// These more advanced operations raise interesting questions for the graph representation.
/// The following discussion assumes you've read the documentation on
/// [`crate::lock::export::MetadataNode`] and understand the notion of Node and Edge we use.
///
///
/// # Roots
///
/// As noted in those docs, pedantically `roots` should include `mypackage`, `mypackage[extra]`,
/// `mypackage:group` as separate roots. At first this seemed like a stance worth rejecting
/// for ergonomics and clarity, but as I tried to rationalize the semantics of `uv tree` it
/// felt increasingly necessary.
///
/// In particular, because `uv tree` has some limited support for changing what parts of the
/// graph are "active" with flags like `--all-groups` or `--no-default-groups`, we "need" a
/// way to refer to a package's groups without referring to the package itself. You can't
/// actually toggle the extras on the workspace but I consider that a bug, and so our format
/// should ideally support referring to *specifically* "a package with some extra(s) activated".
///
/// Thus with everything activated you *may* find all of `mypackage`, `mypackage[extra1]`,
/// `mypackage[extra2]`, `mypackage:group1`, `mypackage:group2` in the `roots` list.
///
/// Conversely, we will *exclude* the workspace node from the `roots`, as it is a purely virtual
/// concept that only exists to hang workspace-exclusive groups from (e.g. when you define
/// `dependency-groups` in a `pyproject.toml` that does not contain a `[project]` table).
///
///
/// # Depth
///
/// Node depth is an annoying concept. The baseline of the theory here is once again
/// an appeal to `mypackage`, `mypackage[extra]`, and `mypackage:group` being all on an equal
/// footing. However, `mypackage[extra]` and `mypackage:group` in the graph are "virtual"
/// in the sense that they don't actually refer to a thing to install, but are instead a
/// list of dependencies that should all be installed together.
///
/// Let's start with the nice examples with obvious answers to establish a baseline:
/// a uv workspace with no extras or groups, just pure production dependencies.
///
/// * depth 0: lists off all workspace members
/// * depth 1: lists off all workspace members and their direct dependencies
/// * depth 2: lists off all workspace members and two levels of dependencies
///
/// So ok depth here refers to how many levels of edges we're willing to follow, great!
/// Now let's add dependency groups and extras.
///
/// Do you list the *existence* of `mypackage:group` or `mypackage[extra]` at depth 0?
/// Or do you only acknowledge their existence at depth 1 when they would be non-empty?
///
/// In the current textual display of `uv tree` we choose the second answer essentially
/// garbage-collecting extras and groups that would be empty because all their edges
/// have been deleted. For now the JSON output respects this behaviour, but we may
/// change that decision if we decide we don't like it:
/// <https://github.com/astral-sh/uv/issues/19973>
///
/// Now to be clear we do this "edge" analysis before lowering to the output graph,
/// and this matters for several cases.
///
/// First, scripts and workspaces aren't considered nodes before the lowering, and
/// so script dependencies and workspace-group dependencies appear at depth 0 (another case where
/// the JSON output respects the behaviour of the textual output):
/// <https://github.com/astral-sh/uv/issues/19976>
///
/// Second, the fact that `mypackage` is a dependency of `mypackage[extra]`.
/// Specifically, in `metadata` if `foo` depends on `bar[extra1, extra2]`
/// then we will only include edges to `bar[extra1]` and `bar[extra2]` and not to `bar` itself,
/// because we know those two extra nodes will include the edge to `bar` anyway (nothing requires
/// this, it just seemed tidier to simplify the graph in that way).
///
/// As long as we want to do that simplification, it is *not* correct for us to
/// cut the edge from the extra to the package, and so we don't guarantee a simple
/// statement like "the resulting graph will have at most depth N" when counting
/// `dependencies` (and that's all muddy anyway since there can be cycles
/// in the final graph).
///
///
/// # Inversion
///
/// `--invert` should flip the edges of the graph, turning the leaves into roots
/// (with operations like `--depth` being applied afterwards).
///
/// Unfortunately this is ill-defined at the moment in the face of leaf-cycles:
/// <https://github.com/astral-sh/uv/issues/19972>
///
/// Ignoring the issue of cycles, the only thing to note here is that only the
/// `dependencies` lists of nodes should be inverted. The `optional_dependencies`
/// and `dependency_groups` listings remain unchanged, because those aren't edges
/// of the graph, they're metadata on those packages (or the workspace).
#[derive(Debug, Serialize)]
struct JsonGraph {
    schema: JsonSchema,
    workspace_root: PortablePathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    script: Option<MetadataScript>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace: Option<MetadataWorkspace>,
    roots: Vec<JsonRoot>,
    inverted: bool,
    /// Workspace members included in the projected resolution.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    members: Vec<MetadataWorkspaceMember>,
    resolution: BTreeMap<String, MetadataNode>,
}

impl JsonGraph {
    fn new(tree: &TreeDisplay<'_>, target: TreeJsonTarget<'_>) -> Self {
        let traversal = tree.json_traversal();
        let workspace_root = PortablePathBuf::from(target.root());
        let mut builder = JsonGraphBuilder::new(tree, workspace_root.clone());

        for index in traversal.nodes.iter().copied() {
            let Node::Package(package_id) = tree.graph[index] else {
                continue;
            };
            builder.ensure_package(package_id, MetadataNodeKind::Package);
        }

        for edge in tree
            .graph
            .edge_references()
            .filter(|edge| traversal.edges.contains(&edge.id()))
        {
            builder.add_package_edge(edge.source(), edge.target(), edge.weight());
        }

        builder.add_target_edges(target, &traversal);
        let (script, workspace) = match target {
            TreeJsonTarget::Script(path) => {
                let path = PortablePathBuf::from(path);
                let id = builder.ensure_script(path.as_ref());
                (Some(MetadataScript::new(path, id)), None)
            }
            TreeJsonTarget::Workspace(path) => {
                let path = PortablePathBuf::from(path);
                let id = builder.ensure_workspace();
                (None, Some(MetadataWorkspace::new(path, id)))
            }
        };
        let roots = builder.roots(target);
        let members = builder.members(target);
        let resolution = builder.finish();

        Self {
            schema: JsonSchema {
                version: JsonSchemaVersion::Preview,
            },
            workspace_root,
            script,
            workspace,
            roots,
            inverted: tree.invert,
            members,
            resolution,
        }
    }
}

struct JsonGraphBuilder<'tree, 'env> {
    tree: &'tree TreeDisplay<'env>,
    workspace_root: PortablePathBuf,
    resolution: BTreeMap<String, MetadataNode>,
}

impl<'tree, 'env> JsonGraphBuilder<'tree, 'env> {
    fn new(tree: &'tree TreeDisplay<'env>, workspace_root: PortablePathBuf) -> Self {
        Self {
            tree,
            workspace_root,
            resolution: BTreeMap::new(),
        }
    }

    fn ensure_identity(&mut self, identity: MetadataNodeId) -> String {
        let id = identity.to_flat();
        self.resolution
            .entry(id.clone())
            .or_insert_with(|| MetadataNode::new(identity));
        id
    }

    fn ensure_package(&mut self, package_id: &'env PackageId, kind: MetadataNodeKind) -> String {
        let is_package = matches!(kind, MetadataNodeKind::Package);
        let id = MetadataNodeId::from_package_id(&self.workspace_root, package_id, kind.clone())
            .to_flat();
        self.resolution.entry(id.clone()).or_insert_with(|| {
            let package = self.tree.lock.find_by_id(package_id);
            let mut node = MetadataNode::from_package_id(&self.workspace_root, package_id, kind);
            if is_package {
                node.set_latest_version(self.tree.latest.get(package_id).cloned());
                node.set_wheels_from_package(&self.workspace_root, package);
            }
            node
        });
        id
    }

    fn ensure_extra(&mut self, package_id: &'env PackageId, extra: &ExtraName) -> String {
        let package = self.ensure_package(package_id, MetadataNodeKind::Package);
        let extra_id = self.ensure_package(package_id, MetadataNodeKind::Extra(extra.clone()));
        self.add_link(
            package.clone(),
            extra_id.clone(),
            JsonLink::Optional(extra.clone()),
        );
        self.add_link(extra_id.clone(), package, JsonLink::Dependency(None));
        extra_id
    }

    fn ensure_group(&mut self, package_id: &'env PackageId, group: &GroupName) -> String {
        let package = self.ensure_package(package_id, MetadataNodeKind::Package);
        let group_id = self.ensure_package(package_id, MetadataNodeKind::Group(group.clone()));
        self.add_link(package, group_id.clone(), JsonLink::Group(group.clone()));
        group_id
    }

    fn ensure_workspace(&mut self) -> String {
        self.ensure_identity(MetadataNodeId::from_workspace(self.workspace_root.clone()))
    }

    fn ensure_workspace_group(&mut self, group: &GroupName) -> String {
        let workspace = self.ensure_workspace();
        let group_id = self.ensure_identity(MetadataNodeId::from_workspace_group(
            self.workspace_root.clone(),
            group.clone(),
        ));
        self.add_link(workspace, group_id.clone(), JsonLink::Group(group.clone()));
        group_id
    }

    fn ensure_script(&mut self, path: &Path) -> String {
        self.ensure_identity(MetadataNodeId::from_script(PortablePathBuf::from(path)))
    }

    fn dependency_targets(
        &mut self,
        package_id: &'env PackageId,
        extras: Option<RequestedExtras<'env>>,
    ) -> Vec<String> {
        let Some(extras) = extras.filter(|extras| !extras.is_empty()) else {
            return vec![self.ensure_package(package_id, MetadataNodeKind::Package)];
        };
        extras
            .iter()
            .map(|extra| self.ensure_extra(package_id, extra))
            .collect()
    }

    fn add_package_edge(&mut self, source: NodeIndex, target: NodeIndex, edge: &Edge<'env>) {
        let (source, target) = if self.tree.invert {
            (target, source)
        } else {
            (source, target)
        };
        let (Node::Package(source), Node::Package(target)) =
            (&self.tree.graph[source], &self.tree.graph[target])
        else {
            return;
        };
        let (source, target) = (*source, *target);

        let source = match edge {
            Edge::Prod(..) => self.ensure_package(source, MetadataNodeKind::Package),
            Edge::Optional(extra, ..) => self.ensure_extra(source, extra),
            Edge::Dev(group, ..) => self.ensure_group(source, group),
        };
        let marker = self.marker(edge);
        for target in self.dependency_targets(target, edge.extras()) {
            self.add_link(source.clone(), target, JsonLink::Dependency(marker.clone()));
        }
    }

    fn add_target_edges(&mut self, target: TreeJsonTarget<'_>, traversal: &JsonTraversal) {
        // Forward edges from the synthetic root establish the target's depth-zero packages, so
        // they are retained even though they are not part of `traversal.edges`. Inverted target
        // edges must have been reached while traversing the reversed graph.
        let edges = self
            .tree
            .graph
            .edge_references()
            .filter(|edge| !self.tree.invert || traversal.edges.contains(&edge.id()))
            .filter_map(|edge| {
                let package = match (
                    &self.tree.graph[edge.source()],
                    &self.tree.graph[edge.target()],
                ) {
                    (Node::Root, Node::Package(package)) | (Node::Package(package), Node::Root) => {
                        *package
                    }
                    (Node::Root, Node::Root) | (Node::Package(_), Node::Package(_)) => return None,
                };
                Some((package, edge.weight()))
            })
            .collect::<Vec<_>>();

        match target {
            TreeJsonTarget::Script(path) => {
                let script = self.ensure_script(path);
                for (package, edge) in edges {
                    let marker = self.marker(edge);
                    for package in self.dependency_targets(package, edge.extras()) {
                        self.add_link(
                            script.clone(),
                            package,
                            JsonLink::Dependency(marker.clone()),
                        );
                    }
                }
            }
            TreeJsonTarget::Workspace(_) => {
                self.ensure_workspace();
                for (package, edge) in edges {
                    let Edge::Dev(group, ..) = edge else {
                        continue;
                    };
                    let group = self.ensure_workspace_group(group);
                    let marker = self.marker(edge);
                    for package in self.dependency_targets(package, edge.extras()) {
                        self.add_link(group.clone(), package, JsonLink::Dependency(marker.clone()));
                    }
                }
            }
        }
    }

    fn marker(&self, edge: &Edge<'_>) -> Option<String> {
        self.tree
            .lock
            .simplify_environment(edge.marker().pep508())
            .try_to_string()
    }

    fn add_link(&mut self, source: String, target: String, link: JsonLink) {
        // `optional_dependencies` and `dependency_groups` advertise related nodes; they are not
        // dependency edges. Keep those relationships attached to their owner when inverting the
        // graph, and reverse only actual dependencies.
        let (source, target) = if self.tree.invert && matches!(&link, JsonLink::Dependency(_)) {
            (target, source)
        } else {
            (source, target)
        };
        let Some(node) = self.resolution.get_mut(&source) else {
            return;
        };
        match link {
            JsonLink::Dependency(marker) => {
                node.add_resolution_dependency(target, marker);
            }
            JsonLink::Optional(name) => {
                node.add_optional_dependency(name, target);
            }
            JsonLink::Group(name) => {
                node.add_dependency_group(name, target);
            }
        }
    }

    fn add_package_roots(&mut self, roots: &mut Vec<JsonRoot>, package_id: &'env PackageId) {
        let package = self.tree.lock.find_by_id(package_id);
        let extras = package
            .optional_dependencies
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let groups = package
            .dependency_groups
            .keys()
            .filter(|group| self.tree.groups.contains(group))
            .cloned()
            .collect::<Vec<_>>();

        if self.tree.prod {
            roots.push(JsonRoot {
                id: self.ensure_package(package_id, MetadataNodeKind::Package),
            });
            for extra in &extras {
                let id = MetadataNodeId::from_package_id(
                    &self.workspace_root,
                    package_id,
                    MetadataNodeKind::Extra(extra.clone()),
                )
                .to_flat();
                if self.resolution.contains_key(&id) {
                    roots.push(JsonRoot { id });
                }
            }
        }

        for group in &groups {
            let id = MetadataNodeId::from_package_id(
                &self.workspace_root,
                package_id,
                MetadataNodeKind::Group(group.clone()),
            )
            .to_flat();
            if self.resolution.contains_key(&id) {
                roots.push(JsonRoot { id });
            }
        }
    }

    fn roots(&mut self, target: TreeJsonTarget<'_>) -> Vec<JsonRoot> {
        let mut roots = Vec::new();
        for root in &self.tree.roots {
            match self.tree.graph[*root] {
                Node::Package(package_id) => {
                    if self.tree.invert {
                        roots.push(JsonRoot {
                            id: self.ensure_package(package_id, MetadataNodeKind::Package),
                        });
                    } else {
                        self.add_package_roots(&mut roots, package_id);
                    }
                }
                Node::Root => match target {
                    TreeJsonTarget::Script(path) => {
                        let script = self.ensure_script(path);
                        roots.push(JsonRoot { id: script });
                    }
                    TreeJsonTarget::Workspace(_) => {
                        let packages = self
                            .tree
                            .graph
                            .edges_directed(*root, Direction::Outgoing)
                            .filter(|edge| matches!(edge.weight(), Edge::Prod(..)))
                            .filter_map(|edge| {
                                let Node::Package(package_id) = self.tree.graph[edge.target()]
                                else {
                                    return None;
                                };
                                Some(package_id)
                            })
                            .collect::<Vec<_>>();
                        for package_id in packages {
                            self.add_package_roots(&mut roots, package_id);
                        }
                        let groups = self
                            .tree
                            .lock
                            .dependency_groups()
                            .keys()
                            .filter(|group| self.tree.groups.contains(group))
                            .cloned()
                            .collect::<Vec<_>>();
                        for group in groups {
                            let id = MetadataNodeId::from_workspace_group(
                                self.workspace_root.clone(),
                                group,
                            )
                            .to_flat();
                            if self.resolution.contains_key(&id) {
                                roots.push(JsonRoot { id });
                            }
                        }
                    }
                },
            }
        }
        roots.sort();
        roots.dedup();
        roots
    }

    fn members(&self, target: TreeJsonTarget<'_>) -> Vec<MetadataWorkspaceMember> {
        if matches!(target, TreeJsonTarget::Script(_)) {
            return Vec::new();
        }

        let packages = if self.tree.lock.members().is_empty() {
            self.tree.lock.root().into_iter().collect::<Vec<_>>()
        } else {
            self.tree
                .lock
                .packages()
                .iter()
                .filter(|package| self.tree.lock.members().contains(&package.id.name))
                .collect::<Vec<_>>()
        };

        packages
            .into_iter()
            .filter(|package| {
                let id = MetadataNodeId::from_package_id(
                    &self.workspace_root,
                    &package.id,
                    MetadataNodeKind::Package,
                )
                .to_flat();
                self.resolution.contains_key(&id)
            })
            .filter_map(|package| {
                MetadataWorkspaceMember::from_locked_package(&self.workspace_root, &package.id)
            })
            .collect()
    }

    fn finish(mut self) -> BTreeMap<String, MetadataNode> {
        for node in self.resolution.values_mut() {
            node.normalize_resolution();
        }
        self.resolution
    }
}

enum JsonLink {
    Dependency(Option<String>),
    Optional(ExtraName),
    Group(GroupName),
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

#[derive(Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct JsonRoot {
    id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct VisitedNode<'env> {
    package_id: &'env PackageId,
    expanded_extras: BTreeSet<&'env ExtraName>,
    marker: Option<UniversalMarker>,
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
    Prod(Option<RequestedExtras<'env>>, UniversalMarker),
    Optional(
        &'env ExtraName,
        Option<RequestedExtras<'env>>,
        UniversalMarker,
    ),
    Dev(
        &'env GroupName,
        Option<RequestedExtras<'env>>,
        UniversalMarker,
    ),
}

impl<'env> Edge<'env> {
    fn extras(&self) -> Option<RequestedExtras<'env>> {
        match self {
            Self::Prod(extras, _) => *extras,
            Self::Optional(_, extras, _) => *extras,
            Self::Dev(_, extras, _) => *extras,
        }
    }

    fn required_extra(&self) -> Option<&'env ExtraName> {
        match self {
            Self::Optional(extra, ..) => Some(extra),
            Self::Prod(..) | Self::Dev(..) => None,
        }
    }

    fn marker(&self) -> UniversalMarker {
        match self {
            Self::Prod(_, marker) | Self::Optional(_, _, marker) | Self::Dev(_, _, marker) => {
                *marker
            }
        }
    }

    fn is_dev(&self) -> bool {
        matches!(self, Self::Dev(..))
    }

    fn kind(&self) -> EdgeKind<'env> {
        match self {
            Self::Prod(..) => EdgeKind::Prod,
            Self::Optional(extra, ..) => EdgeKind::Optional(extra),
            Self::Dev(group, ..) => EdgeKind::Dev(group),
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum RequestedExtras<'env> {
    Dependency(&'env BTreeSet<ExtraName>),
    Requirement(&'env [ExtraName]),
}

impl PartialEq for RequestedExtras<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for RequestedExtras<'_> {}

impl PartialOrd for RequestedExtras<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RequestedExtras<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.iter().cmp(other.iter())
    }
}

impl<'env> RequestedExtras<'env> {
    fn contains(self, extra: &ExtraName) -> bool {
        match self {
            Self::Dependency(extras) => extras.contains(extra),
            Self::Requirement(extras) => extras.contains(extra),
        }
    }

    fn is_empty(self) -> bool {
        match self {
            Self::Dependency(extras) => extras.is_empty(),
            Self::Requirement(extras) => extras.is_empty(),
        }
    }

    fn iter(self) -> impl Iterator<Item = &'env ExtraName> {
        match self {
            Self::Dependency(extras) => Either::Left(extras.iter()),
            Self::Requirement(extras) => Either::Right(extras.iter()),
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
struct Cursor(NodeIndex, Option<EdgeIndex>, UniversalMarker);

impl Cursor {
    /// Create a [`Cursor`] representing a node in the dependency tree.
    fn new(node: NodeIndex, edge: EdgeIndex, marker: UniversalMarker) -> Self {
        Self(node, Some(edge), marker)
    }

    /// Create a [`Cursor`] representing a root node in the dependency tree.
    fn root(node: NodeIndex, marker: UniversalMarker) -> Self {
        Self(node, None, marker)
    }

    /// Return the [`NodeIndex`] of the node.
    fn node(&self) -> NodeIndex {
        self.0
    }

    /// Return the [`EdgeIndex`] of the edge that led to the node, if any.
    fn edge(&self) -> Option<EdgeIndex> {
        self.1
    }

    /// Return the marker context accumulated along the path to this node.
    fn marker(&self) -> UniversalMarker {
        self.2
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

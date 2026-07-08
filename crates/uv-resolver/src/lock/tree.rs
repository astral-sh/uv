use std::cmp::Ordering;
use std::collections::{BTreeSet, VecDeque};
use std::fmt::Write;

use either::Either;
use itertools::Itertools;
use owo_colors::OwoColorize;
use petgraph::algo::tarjan_scc;
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

use crate::lock::{Package, PackageId};
use crate::{ConflictMarker, Lock, PackageMap, UniversalMarker};

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
                    // edges after reversal).
                    let mut roots: Vec<_> = graph
                        .node_indices()
                        .filter(|index| {
                            graph
                                .edges_directed(*index, Direction::Incoming)
                                .next()
                                .is_none()
                        })
                        .collect();

                    // If no node has zero incoming edges (e.g., a leaf cycle), fall
                    // back to treating each root SCC as a root. A root SCC is one
                    // whose nodes have no incoming edges from outside the SCC.
                    if roots.is_empty() {
                        let sccs = tarjan_scc(&graph);
                        for scc in &sccs {
                            let has_external_incoming = scc.iter().any(|node| {
                                graph
                                    .edges_directed(*node, Direction::Incoming)
                                    .any(|edge| !scc.contains(&edge.source()))
                            });
                            if !has_external_incoming {
                                // Pick the best node in the SCC as root: prefer
                                // packages over Root nodes.
                                let root = scc
                                    .iter()
                                    .find(|n| matches!(graph[**n], Node::Package(_)))
                                    .or_else(|| scc.first());
                                if let Some(root) = root {
                                    roots.push(*root);
                                }
                            }
                        }
                    }

                    roots
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

use std::collections::VecDeque;
use std::fmt::Write;

use anyhow::Result;
use owo_colors::OwoColorize;
use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::prelude::EdgeRef;
use petgraph::Direction;
use rustc_hash::{FxHashMap, FxHashSet};

use uv_cache::Cache;
use uv_distribution_types::{Diagnostic, Name};
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_pep508::{Requirement, VersionOrUrl};
use uv_pypi_types::{ResolutionMetadata, ResolverMarkerEnvironment, VerbatimParsedUrl};
use uv_python::{EnvironmentPreference, PythonEnvironment, PythonRequest};

use crate::commands::pip::operations::report_target_environment;
use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Display the installed packages in the current environment as a dependency tree.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) fn pip_tree(
    show_version_specifiers: bool,
    depth: u8,
    prune: &[PackageName],
    package: &[PackageName],
    no_dedupe: bool,
    invert: bool,
    strict: bool,
    python: Option<&str>,
    system: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let environment = PythonEnvironment::find(
        &python.map(PythonRequest::parse).unwrap_or_default(),
        EnvironmentPreference::from_system_flag(system, false),
        cache,
    )?;

    report_target_environment(&environment, cache, printer)?;

    // Read packages from the virtual environment.
    let site_packages = SitePackages::from_environment(&environment)?;

    let packages = {
        let mut packages: FxHashMap<_, Vec<_>> = FxHashMap::default();
        for package in site_packages.iter() {
            packages
                .entry(package.name())
                .or_default()
                .push(package.metadata()?);
        }
        packages
    };

    // Determine the markers to use for the resolution.
    let markers = environment.interpreter().resolver_marker_environment();

    // Render the tree.
    let rendered_tree = DisplayDependencyGraph::new(
        depth.into(),
        prune,
        package,
        no_dedupe,
        invert,
        show_version_specifiers,
        &markers,
        &packages,
    )
    .render()
    .join("\n");

    writeln!(printer.stdout(), "{rendered_tree}")?;

    if rendered_tree.contains("(*)") {
        let message = if no_dedupe {
            "(*) Package tree is a cycle and cannot be shown".italic()
        } else {
            "(*) Package tree already displayed".italic()
        };
        writeln!(printer.stdout(), "{message}")?;
    }

    // Validate that the environment is consistent.
    if strict {
        for diagnostic in site_packages.diagnostics(&markers)? {
            writeln!(
                printer.stderr(),
                "{}{} {}",
                "warning".yellow().bold(),
                ":".bold(),
                diagnostic.message().bold()
            )?;
        }
    }

    Ok(ExitStatus::Success)
}

#[derive(Debug)]
pub(crate) struct DisplayDependencyGraph<'env> {
    /// The constructed dependency graph.
    graph: petgraph::graph::Graph<
        &'env ResolutionMetadata,
        &'env Requirement<VerbatimParsedUrl>,
        petgraph::Directed,
    >,
    /// The packages considered as roots of the dependency tree.
    roots: Vec<NodeIndex>,
    /// Maximum display depth of the dependency tree
    depth: usize,
    /// Whether to de-duplicate the displayed dependencies.
    no_dedupe: bool,
    /// Whether to invert the dependency tree.
    invert: bool,
    /// Whether to include the version specifiers in the tree.
    show_version_specifiers: bool,
}

impl<'env> DisplayDependencyGraph<'env> {
    /// Create a new [`DisplayDependencyGraph`] for the set of installed distributions.
    pub(crate) fn new(
        depth: usize,
        prune: &[PackageName],
        package: &[PackageName],
        no_dedupe: bool,
        invert: bool,
        show_version_specifiers: bool,
        markers: &ResolverMarkerEnvironment,
        packages: &'env FxHashMap<&PackageName, Vec<ResolutionMetadata>>,
    ) -> Self {
        // Create a graph.
        let mut graph = petgraph::graph::Graph::<
            &ResolutionMetadata,
            &Requirement<VerbatimParsedUrl>,
            petgraph::Directed,
        >::new();

        // Step 1: Add each installed package.
        let mut inverse: FxHashMap<PackageName, Vec<NodeIndex>> = FxHashMap::default();
        for metadata in packages.values().flatten() {
            if prune.contains(&metadata.name) {
                continue;
            }

            let index = graph.add_node(metadata);
            inverse
                .entry(metadata.name.clone())
                .or_default()
                .push(index);
        }

        // Step 2: Add all dependencies.
        for index in graph.node_indices() {
            let metadata = &graph[index];

            for requirement in &metadata.requires_dist {
                if prune.contains(&requirement.name) {
                    continue;
                }
                if !requirement.marker.evaluate(markers, &[]) {
                    continue;
                }

                for dep_index in inverse
                    .get(&requirement.name)
                    .into_iter()
                    .flatten()
                    .copied()
                {
                    let dep = &graph[dep_index];

                    // Avoid adding an edge if the dependency is not required by the current package.
                    if let Some(VersionOrUrl::VersionSpecifier(specifier)) =
                        requirement.version_or_url.as_ref()
                    {
                        if !specifier.contains(&dep.version) {
                            continue;
                        }
                    }

                    graph.add_edge(index, dep_index, requirement);
                }
            }
        }

        // Step 2: Reverse the graph.
        if invert {
            graph.reverse();
        }

        // Step 3: Filter the graph to those nodes reachable from the target packages.
        if !package.is_empty() {
            // Perform a DFS from the root nodes to find the reachable nodes.
            let mut reachable = graph
                .node_indices()
                .filter(|index| package.contains(&graph[*index].name))
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
            roots.sort_by_key(|index| {
                let metadata = &graph[*index];
                (&metadata.name, &metadata.version)
            });

            // Re-add the removed edges.
            for (source, target, weight) in edges {
                graph.add_edge(source, target, weight);
            }

            roots
        };

        Self {
            graph,
            roots,
            depth,
            no_dedupe,
            invert,
            show_version_specifiers,
        }
    }

    /// Perform a depth-first traversal of the given distribution and its dependencies.
    fn visit(
        &self,
        cursor: Cursor,
        visited: &mut FxHashMap<&'env PackageName, Vec<PackageName>>,
        path: &mut Vec<&'env PackageName>,
    ) -> Vec<String> {
        // Short-circuit if the current path is longer than the provided depth.
        if path.len() > self.depth {
            return Vec::new();
        }

        let metadata = &self.graph[cursor.node()];
        let package_name = &metadata.name;
        let mut line = format!("{} v{}", package_name, metadata.version);

        // If the current package is not top-level (i.e., it has a parent), include the specifiers.
        if self.show_version_specifiers {
            if let Some(edge) = cursor.edge() {
                line.push(' ');

                let source = &self.graph[edge];
                if self.invert {
                    let parent = self.graph.edge_endpoints(edge).unwrap().0;
                    let parent = &self.graph[parent].name;
                    let version = match source.version_or_url.as_ref() {
                        None => "*".to_string(),
                        Some(version) => version.to_string(),
                    };
                    line.push_str(&format!("[requires: {parent} {version}]"));
                } else {
                    let version = match source.version_or_url.as_ref() {
                        None => "*".to_string(),
                        Some(version) => version.to_string(),
                    };
                    line.push_str(&format!("[required: {version}]"));
                }
            }
        }

        // Skip the traversal if:
        // 1. The package is in the current traversal path (i.e., a dependency cycle).
        // 2. The package has been visited and de-duplication is enabled (default).
        if let Some(requirements) = visited.get(package_name) {
            if !self.no_dedupe || path.contains(&package_name) {
                return if requirements.is_empty() {
                    vec![line]
                } else {
                    vec![format!("{} (*)", line)]
                };
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
            let metadata = &self.graph[node.node()];
            (&metadata.name, &metadata.version)
        });

        let mut lines = vec![line];

        // Keep track of the dependency path to avoid cycles.
        visited.insert(
            package_name,
            dependencies
                .iter()
                .map(|node| {
                    let metadata = &self.graph[node.node()];
                    metadata.name.clone()
                })
                .collect(),
        );
        path.push(package_name);

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
    pub(crate) fn render(&self) -> Vec<String> {
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

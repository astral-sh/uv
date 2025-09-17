use std::collections::BTreeSet;

use owo_colors::OwoColorize;
use petgraph::visit::EdgeRef;
use petgraph::{Directed, Direction, Graph};
use rustc_hash::{FxBuildHasher, FxHashMap};

use uv_distribution_types::{DistributionMetadata, Name, SourceAnnotation, SourceAnnotations};
use uv_normalize::PackageName;
use uv_pep508::{MarkerTree, MarkerVariantsUniversal};

use crate::resolution::{RequirementsTxtDist, ResolutionGraphNode};
use crate::{ResolverEnvironment, ResolverOutput};

/// A [`std::fmt::Display`] implementation for the resolution graph.
#[derive(Debug)]
pub struct DisplayResolutionGraph<'a> {
    /// The underlying graph.
    resolution: &'a ResolverOutput,
    /// The resolver marker environment, used to determine the markers that apply to each package.
    env: &'a ResolverEnvironment,
    /// The packages to exclude from the output.
    no_emit_packages: &'a [PackageName],
    /// Whether to include hashes in the output.
    show_hashes: bool,
    /// Whether to include extras in the output (e.g., `black[colorama]`).
    include_extras: bool,
    /// Whether to include environment markers in the output (e.g., `black ; sys_platform == "win32"`).
    include_markers: bool,
    /// Whether to include annotations in the output, to indicate which dependency or dependencies
    /// requested each package.
    include_annotations: bool,
    /// Whether to include indexes in the output, to indicate which index was used for each package.
    include_index_annotation: bool,
    /// The style of annotation comments, used to indicate the dependencies that requested each
    /// package.
    annotation_style: AnnotationStyle,
}

#[derive(Debug)]
enum DisplayResolutionGraphNode<'dist> {
    Root,
    Dist(RequirementsTxtDist<'dist>),
}

impl<'a> DisplayResolutionGraph<'a> {
    /// Create a new [`DisplayResolutionGraph`] for the given graph.
    ///
    /// Note that this panics if any of the forks in the given resolver
    /// output contain non-empty conflicting groups. That is, when using `uv
    /// pip compile`, specifying conflicts is not supported because their
    /// conditional logic cannot be encoded into a `requirements.txt`.
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn new(
        underlying: &'a ResolverOutput,
        env: &'a ResolverEnvironment,
        no_emit_packages: &'a [PackageName],
        show_hashes: bool,
        include_extras: bool,
        include_markers: bool,
        include_annotations: bool,
        include_index_annotation: bool,
        annotation_style: AnnotationStyle,
    ) -> Self {
        for fork_marker in &underlying.fork_markers {
            assert!(
                fork_marker.conflict().is_true(),
                "found fork marker {fork_marker:?} with non-trivial conflicting marker, \
                 cannot display resolver output with conflicts in requirements.txt format",
            );
        }
        Self {
            resolution: underlying,
            env,
            no_emit_packages,
            show_hashes,
            include_extras,
            include_markers,
            include_annotations,
            include_index_annotation,
            annotation_style,
        }
    }
}

/// Write the graph in the `{name}=={version}` format of requirements.txt that pip uses.
impl std::fmt::Display for DisplayResolutionGraph<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Determine the annotation sources for each package.
        let sources = if self.include_annotations {
            let mut sources = SourceAnnotations::default();

            for requirement in self.resolution.requirements.iter().filter(|requirement| {
                requirement.evaluate_markers(
                    self.env.marker_environment(),
                    MarkerVariantsUniversal,
                    &[],
                )
            }) {
                if let Some(origin) = &requirement.origin {
                    sources.add(
                        &requirement.name,
                        SourceAnnotation::Requirement(origin.clone()),
                    );
                }
            }

            for requirement in self
                .resolution
                .constraints
                .requirements()
                .filter(|requirement| {
                    requirement.evaluate_markers(
                        self.env.marker_environment(),
                        MarkerVariantsUniversal,
                        &[],
                    )
                })
            {
                if let Some(origin) = &requirement.origin {
                    sources.add(
                        &requirement.name,
                        SourceAnnotation::Constraint(origin.clone()),
                    );
                }
            }

            for requirement in self
                .resolution
                .overrides
                .requirements()
                .filter(|requirement| {
                    requirement.evaluate_markers(
                        self.env.marker_environment(),
                        MarkerVariantsUniversal,
                        &[],
                    )
                })
            {
                if let Some(origin) = &requirement.origin {
                    sources.add(
                        &requirement.name,
                        SourceAnnotation::Override(origin.clone()),
                    );
                }
            }

            sources
        } else {
            SourceAnnotations::default()
        };

        // Convert a [`petgraph::graph::Graph`] based on [`ResolutionGraphNode`] to a graph based on
        // [`DisplayResolutionGraphNode`]. In other words: converts from [`AnnotatedDist`] to
        // [`RequirementsTxtDist`].
        //
        // We assign each package its propagated markers: In `requirements.txt`, we want a flat list
        // that for each package tells us if it should be installed on the current platform, without
        // looking at which packages depend on it.
        let graph = self.resolution.graph.map(
            |_index, node| match node {
                ResolutionGraphNode::Root => DisplayResolutionGraphNode::Root,
                ResolutionGraphNode::Dist(dist) => {
                    let dist = RequirementsTxtDist::from_annotated_dist(dist);
                    DisplayResolutionGraphNode::Dist(dist)
                }
            },
            // We can drop the edge markers, while retaining their existence and direction for the
            // annotations.
            |_index, _edge| (),
        );

        // Reduce the graph, removing or combining extras for a given package.
        let graph = if self.include_extras {
            combine_extras(&graph)
        } else {
            strip_extras(&graph)
        };

        // Collect all packages.
        let mut nodes = graph
            .node_indices()
            .filter_map(|index| {
                let dist = &graph[index];
                let name = dist.name();
                if self.no_emit_packages.contains(name) {
                    return None;
                }

                Some((index, dist))
            })
            .collect::<Vec<_>>();

        // Sort the nodes by name, but with editable packages first.
        nodes.sort_unstable_by_key(|(index, node)| (node.to_comparator(), *index));

        // Print out the dependency graph.
        for (index, node) in nodes {
            // Display the node itself.
            let mut line = node
                .to_requirements_txt(&self.resolution.requires_python, self.include_markers)
                .to_string();

            // Display the distribution hashes, if any.
            let mut has_hashes = false;
            if self.show_hashes {
                for hash in node.hashes {
                    has_hashes = true;
                    line.push_str(" \\\n");
                    line.push_str("    --hash=");
                    line.push_str(&hash.to_string());
                }
            }

            // Determine the annotation comment and separator (between comment and requirement).
            let mut annotation = None;

            // If enabled, include annotations to indicate the dependencies that requested each
            // package (e.g., `# via mypy`).
            if self.include_annotations {
                // Display all dependents (i.e., all packages that depend on the current package).
                let dependents = {
                    let mut dependents = graph
                        .edges_directed(index, Direction::Incoming)
                        .map(|edge| &graph[edge.source()])
                        .map(uv_distribution_types::Name::name)
                        .collect::<Vec<_>>();
                    dependents.sort_unstable();
                    dependents.dedup();
                    dependents
                };

                // Include all external sources (e.g., requirements files).
                let default = BTreeSet::default();
                let source = sources.get(node.name()).unwrap_or(&default);

                match self.annotation_style {
                    AnnotationStyle::Line => match dependents.as_slice() {
                        [] if source.is_empty() => {}
                        [] if source.len() == 1 => {
                            let separator = if has_hashes { "\n    " } else { "  " };
                            let comment = format!("# via {}", source.iter().next().unwrap())
                                .green()
                                .to_string();
                            annotation = Some((separator, comment));
                        }
                        dependents => {
                            let separator = if has_hashes { "\n    " } else { "  " };
                            let dependents = dependents
                                .iter()
                                .map(ToString::to_string)
                                .chain(source.iter().map(ToString::to_string))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let comment = format!("# via {dependents}").green().to_string();
                            annotation = Some((separator, comment));
                        }
                    },
                    AnnotationStyle::Split => match dependents.as_slice() {
                        [] if source.is_empty() => {}
                        [] if source.len() == 1 => {
                            let separator = "\n";
                            let comment = format!("    # via {}", source.iter().next().unwrap())
                                .green()
                                .to_string();
                            annotation = Some((separator, comment));
                        }
                        [dependent] if source.is_empty() => {
                            let separator = "\n";
                            let comment = format!("    # via {dependent}").green().to_string();
                            annotation = Some((separator, comment));
                        }
                        dependents => {
                            let separator = "\n";
                            let dependent = source
                                .iter()
                                .map(ToString::to_string)
                                .chain(dependents.iter().map(ToString::to_string))
                                .map(|name| format!("    #   {name}"))
                                .collect::<Vec<_>>()
                                .join("\n");
                            let comment = format!("    # via\n{dependent}").green().to_string();
                            annotation = Some((separator, comment));
                        }
                    },
                }
            }

            if let Some((separator, comment)) = annotation {
                // Assemble the line with the annotations and remove trailing whitespaces.
                for line in format!("{line:24}{separator}{comment}").lines() {
                    let line = line.trim_end();
                    writeln!(f, "{line}")?;
                }
            } else {
                // Write the line as is.
                writeln!(f, "{line}")?;
            }

            // If enabled, include indexes to indicate which index was used for each package (e.g.,
            // `# from https://pypi.org/simple`).
            if self.include_index_annotation {
                if let Some(index) = node.dist.index() {
                    let url = index.without_credentials();
                    writeln!(f, "{}", format!("    # from {url}").green())?;
                }
            }
        }

        Ok(())
    }
}

/// Indicate the style of annotation comments, used to indicate the dependencies that requested each
/// package.
#[derive(Debug, Default, Copy, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum AnnotationStyle {
    /// Render the annotations on a single, comma-separated line.
    Line,
    /// Render each annotation on its own line.
    #[default]
    Split,
}

/// We don't need the edge markers anymore since we switched to propagated markers.
type IntermediatePetGraph<'dist> = Graph<DisplayResolutionGraphNode<'dist>, (), Directed>;

type RequirementsTxtGraph<'dist> = Graph<RequirementsTxtDist<'dist>, (), Directed>;

/// Reduce the graph, such that all nodes for a single package are combined, regardless of
/// the extras, as long as they have the same version and markers.
///
/// For example, `flask` and `flask[dotenv]` should be reduced into a single `flask[dotenv]`
/// node.
///
/// If the extras have different markers, they'll be treated as separate nodes. For example,
/// `flask[dotenv] ; sys_platform == "win32"` and `flask[async] ; sys_platform == "linux"`
/// would _not_ be combined.
///
/// We also remove the root node, to simplify the graph structure.
fn combine_extras<'dist>(graph: &IntermediatePetGraph<'dist>) -> RequirementsTxtGraph<'dist> {
    /// Return the key for a node.
    fn version_marker<'dist>(dist: &'dist RequirementsTxtDist) -> (&'dist PackageName, MarkerTree) {
        (dist.name(), dist.markers)
    }

    let mut next = RequirementsTxtGraph::with_capacity(graph.node_count(), graph.edge_count());
    let mut inverse = FxHashMap::with_capacity_and_hasher(graph.node_count(), FxBuildHasher);

    // Re-add the nodes to the reduced graph.
    for index in graph.node_indices() {
        let DisplayResolutionGraphNode::Dist(dist) = &graph[index] else {
            continue;
        };

        // In the `requirements.txt` output, we want a flat installation list, so we need to use
        // the reachability markers instead of the edge markers.
        match inverse.entry(version_marker(dist)) {
            std::collections::hash_map::Entry::Occupied(entry) => {
                let index = *entry.get();
                let node: &mut RequirementsTxtDist = &mut next[index];
                node.extras.extend(dist.extras.iter().cloned());
                node.extras.sort_unstable();
                node.extras.dedup();
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                let index = next.add_node(dist.clone());
                entry.insert(index);
            }
        }
    }

    // Re-add the edges to the reduced graph.
    for edge in graph.edge_indices() {
        let (source, target) = graph.edge_endpoints(edge).unwrap();
        let DisplayResolutionGraphNode::Dist(source_node) = &graph[source] else {
            continue;
        };
        let DisplayResolutionGraphNode::Dist(target_node) = &graph[target] else {
            continue;
        };
        let source = inverse[&version_marker(source_node)];
        let target = inverse[&version_marker(target_node)];

        next.update_edge(source, target, ());
    }

    next
}

/// Reduce the graph, such that all nodes for a single package are combined, with extras
/// removed.
///
/// For example, `flask`, `flask[async]`, and `flask[dotenv]` should be reduced into a single
/// `flask` node, with a conjunction of their markers.
///
/// We also remove the root node, to simplify the graph structure.
fn strip_extras<'dist>(graph: &IntermediatePetGraph<'dist>) -> RequirementsTxtGraph<'dist> {
    let mut next = RequirementsTxtGraph::with_capacity(graph.node_count(), graph.edge_count());
    let mut inverse = FxHashMap::with_capacity_and_hasher(graph.node_count(), FxBuildHasher);

    // Re-add the nodes to the reduced graph.
    for index in graph.node_indices() {
        let DisplayResolutionGraphNode::Dist(dist) = &graph[index] else {
            continue;
        };

        // In the `requirements.txt` output, we want a flat installation list, so we need to use
        // the reachability markers instead of the edge markers.
        match inverse.entry(dist.version_id()) {
            std::collections::hash_map::Entry::Occupied(entry) => {
                let index = *entry.get();
                let node: &mut RequirementsTxtDist = &mut next[index];
                node.extras.clear();
                // Consider:
                // ```
                // foo[bar]==1.0.0; sys_platform == 'linux'
                // foo==1.0.0; sys_platform != 'linux'
                // ```
                // In this case, we want to write `foo==1.0.0; sys_platform == 'linux' or sys_platform == 'windows'`
                node.markers.or(dist.markers);
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                let index = next.add_node(dist.clone());
                entry.insert(index);
            }
        }
    }

    // Re-add the edges to the reduced graph.
    for edge in graph.edge_indices() {
        let (source, target) = graph.edge_endpoints(edge).unwrap();
        let DisplayResolutionGraphNode::Dist(source_node) = &graph[source] else {
            continue;
        };
        let DisplayResolutionGraphNode::Dist(target_node) = &graph[target] else {
            continue;
        };
        let source = inverse[&source_node.version_id()];
        let target = inverse[&target_node.version_id()];

        next.update_edge(source, target, ());
    }

    next
}

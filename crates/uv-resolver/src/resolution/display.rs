use std::borrow::Cow;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use owo_colors::OwoColorize;
use petgraph::visit::EdgeRef;
use petgraph::{Directed, Direction, Graph};
use rustc_hash::{FxBuildHasher, FxHashMap};

use uv_distribution_types::{DistributionMetadata, Name, SourceAnnotation, SourceAnnotations};
use uv_normalize::PackageName;
use uv_pep508::MarkerTree;
use uv_pypi_types::HashDigest;

use crate::resolution::{RequirementsTxtDist, ResolutionGraphNode};
use crate::{ResolverEnvironment, ResolverOutput};

/// A `requirements.txt` view of a resolution or a set of exact-target resolutions.
#[derive(Debug)]
pub struct DisplayResolutionGraph<'a> {
    requirements: Vec<DisplayRequirement<'a>>,
    annotation_style: AnnotationStyle,
}

/// A requirement and its annotations, ready for the shared requirements formatter.
#[derive(Debug)]
struct DisplayRequirement<'a> {
    dist: RequirementsTxtDist<'a>,
    line: String,
    hashes: Cow<'a, [HashDigest]>,
    dependents: BTreeSet<PackageName>,
    sources: BTreeSet<SourceAnnotation>,
    indexes: BTreeSet<String>,
}

#[derive(Debug)]
enum DisplayResolutionGraphNode<'dist> {
    Root,
    Dist(RequirementsTxtDist<'dist>),
}

/// One exact-platform resolution and the marker that identifies its target environment.
#[derive(Debug, Clone, Copy)]
pub struct ExactTargetOutput<'a> {
    pub resolution: &'a ResolverOutput,
    pub environment: &'a ResolverEnvironment,
    pub selector: MarkerTree,
}

/// An editable requirement cannot be conditional in a `requirements.txt` file.
#[derive(Debug, thiserror::Error)]
pub enum DisplayResolutionMatrixError {
    #[error("Editable requirement `{0}` is not present in every target environment")]
    ConditionalEditable(String),
}

struct MergedRequirement<'a> {
    requirement: DisplayRequirement<'a>,
    marker: MarkerTree,
    same_intrinsic_marker: bool,
    targets: BTreeSet<usize>,
}

impl<'a> DisplayResolutionGraph<'a> {
    /// Create a new [`DisplayResolutionGraph`] for the given graph.
    ///
    /// Panics if a fork contains conflicting groups, which cannot be represented in
    /// `requirements.txt` output.
    #[expect(clippy::fn_params_excessive_bools)]
    pub fn new(
        resolution: &'a ResolverOutput,
        env: &ResolverEnvironment,
        no_emit_packages: &[PackageName],
        show_hashes: bool,
        include_extras: bool,
        include_markers: bool,
        include_annotations: bool,
        include_index_annotation: bool,
        annotation_style: AnnotationStyle,
    ) -> Self {
        for fork_marker in &resolution.fork_markers {
            assert!(
                fork_marker.conflict().is_true(),
                "found fork marker {fork_marker:?} with non-trivial conflicting marker, \
                 cannot display resolver output with conflicts in requirements.txt format",
            );
        }
        let sources = if include_annotations {
            source_annotations(resolution, env)
        } else {
            SourceAnnotations::default()
        };
        let graph = resolution.graph.map(
            |_index, node| match node {
                ResolutionGraphNode::Root => DisplayResolutionGraphNode::Root,
                ResolutionGraphNode::Dist(dist) => {
                    DisplayResolutionGraphNode::Dist(RequirementsTxtDist::from_annotated_dist(dist))
                }
            },
            |_index, _edge| (),
        );
        let graph = if include_extras {
            combine_extras(&graph)
        } else {
            strip_extras(&graph)
        };
        let mut requirements = Vec::new();
        for index in graph.node_indices() {
            let node = &graph[index];
            if no_emit_packages.contains(node.name()) {
                continue;
            }
            requirements.push(DisplayRequirement {
                dist: node.clone(),
                line: node
                    .to_requirements_txt(&resolution.requires_python, include_markers)
                    .into_owned(),
                hashes: Cow::Borrowed(if show_hashes { node.hashes } else { &[] }),
                dependents: if include_annotations {
                    graph
                        .edges_directed(index, Direction::Incoming)
                        .map(|edge| graph[edge.source()].name().clone())
                        .collect()
                } else {
                    BTreeSet::new()
                },
                sources: sources.get(node.name()).cloned().unwrap_or_default(),
                indexes: if include_index_annotation {
                    node.dist
                        .index()
                        .map(|index| index.without_credentials().to_string())
                        .into_iter()
                        .collect()
                } else {
                    BTreeSet::new()
                },
            });
        }
        // Preserve graph order for requirements whose comparators are equal.
        requirements
            .sort_by(|left, right| left.dist.to_comparator().cmp(&right.dist.to_comparator()));
        Self {
            requirements,
            annotation_style,
        }
    }

    /// Merge independently resolved targets, retaining common pins and qualifying other pins
    /// with their target selectors. Hashes and annotations are combined for identical requirements.
    ///
    /// Callers must provide pairwise-disjoint target selectors. A pin keeps its original dependency
    /// marker only when it occurs in every target with the same marker; otherwise, its markers are
    /// combined with their target selectors. Editables must occur in every target, since `-e` lines
    /// cannot carry markers.
    #[expect(clippy::fn_params_excessive_bools)]
    pub fn from_targets(
        outputs: &[ExactTargetOutput<'a>],
        no_emit_packages: &[PackageName],
        show_hashes: bool,
        include_extras: bool,
        include_annotations: bool,
        include_index_annotation: bool,
        annotation_style: AnnotationStyle,
    ) -> Result<Self, DisplayResolutionMatrixError> {
        let mut merged: BTreeMap<String, MergedRequirement<'a>> = BTreeMap::new();
        for (target_index, output) in outputs.iter().enumerate() {
            // Add selectors after formatting the base requirement, so `requires-python`
            // simplification cannot remove a target's Python-version selector.
            let display = Self::new(
                output.resolution,
                output.environment,
                no_emit_packages,
                show_hashes,
                include_extras,
                false,
                include_annotations,
                include_index_annotation,
                annotation_style,
            );
            for requirement in display.requirements {
                let marker = requirement.dist.markers.and(output.selector);
                if marker.is_false() {
                    continue;
                }
                match merged.entry(requirement.line.clone()) {
                    Entry::Vacant(entry) => {
                        entry.insert(MergedRequirement {
                            requirement,
                            marker,
                            same_intrinsic_marker: true,
                            targets: BTreeSet::from([target_index]),
                        });
                    }
                    Entry::Occupied(mut entry) => {
                        let entry = entry.get_mut();
                        entry.marker = entry.marker.or(marker);
                        entry.same_intrinsic_marker &=
                            entry.requirement.dist.markers == requirement.dist.markers;
                        entry.targets.insert(target_index);
                        let hashes = entry.requirement.hashes.to_mut();
                        hashes.extend(requirement.hashes.iter().cloned());
                        hashes.sort_unstable();
                        hashes.dedup();
                        entry.requirement.dependents.extend(requirement.dependents);
                        entry.requirement.sources.extend(requirement.sources);
                        entry.requirement.indexes.extend(requirement.indexes);
                    }
                }
            }
        }
        let mut requirements = Vec::with_capacity(merged.len());
        for MergedRequirement {
            mut requirement,
            mut marker,
            same_intrinsic_marker,
            targets,
        } in merged.into_values()
        {
            if targets.len() == outputs.len() && same_intrinsic_marker {
                // Common pins retain their original dependency markers without a target guard.
                marker = requirement.dist.markers;
            }
            if requirement.dist.dist.is_editable() {
                if targets.len() != outputs.len() {
                    return Err(DisplayResolutionMatrixError::ConditionalEditable(
                        requirement.line,
                    ));
                }
                // Editable requirements cannot carry markers.
                marker = MarkerTree::TRUE;
            }
            if let Some(marker) = marker.contents() {
                requirement.line.push_str(" ; ");
                requirement.line.push_str(&marker.to_string());
            }
            requirements.push(requirement);
        }
        requirements
            .sort_by(|left, right| left.dist.to_comparator().cmp(&right.dist.to_comparator()));
        Ok(Self {
            requirements,
            annotation_style,
        })
    }
}

impl std::fmt::Display for DisplayResolutionGraph<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for requirement in &self.requirements {
            let mut line = requirement.line.clone();
            let has_hashes = !requirement.hashes.is_empty();
            for hash in requirement.hashes.iter() {
                line.push_str(" \\\n    --hash=");
                line.push_str(&hash.to_string());
            }

            let dependents = requirement.dependents.iter().map(ToString::to_string);
            let sources = requirement.sources.iter().map(ToString::to_string);
            let via: Vec<_> = match self.annotation_style {
                AnnotationStyle::Line => dependents.chain(sources).collect(),
                AnnotationStyle::Split => sources.chain(dependents).collect(),
            };
            let annotation = if via.is_empty() {
                None
            } else {
                let (separator, comment) = match self.annotation_style {
                    AnnotationStyle::Line => (
                        if has_hashes { "\n    " } else { "  " },
                        format!("# via {}", via.join(", ")).green().to_string(),
                    ),
                    AnnotationStyle::Split => {
                        let comment = if via.len() == 1 {
                            format!("    # via {}", via[0])
                        } else {
                            format!(
                                "    # via\n{}",
                                via.iter()
                                    .map(|source| format!("    #   {source}"))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            )
                        };
                        ("\n", comment.green().to_string())
                    }
                };
                Some((separator, comment))
            };

            if let Some((separator, comment)) = annotation {
                for line in format!("{line:24}{separator}{comment}").lines() {
                    writeln!(f, "{}", line.trim_end())?;
                }
            } else {
                writeln!(f, "{line}")?;
            }

            for index in &requirement.indexes {
                writeln!(f, "{}", format!("    # from {index}").green())?;
            }
        }

        Ok(())
    }
}

/// Collect requirement, constraint, and override origins active in the resolver environment.
/// Scoped overrides contribute annotations only for matching parent-to-dependency edges in the
/// resolved graph.
fn source_annotations(resolution: &ResolverOutput, env: &ResolverEnvironment) -> SourceAnnotations {
    let mut sources = SourceAnnotations::default();

    for requirement in resolution
        .requirements
        .iter()
        .filter(|requirement| requirement.evaluate_markers(env.marker_environment(), &[]))
    {
        if let Some(origin) = &requirement.origin {
            sources.add(
                &requirement.name,
                SourceAnnotation::Requirement(origin.clone()),
            );
        }
    }

    for requirement in resolution
        .constraints
        .requirements()
        .filter(|requirement| requirement.evaluate_markers(env.marker_environment(), &[]))
    {
        if let Some(origin) = &requirement.origin {
            sources.add(
                &requirement.name,
                SourceAnnotation::Constraint(origin.clone()),
            );
        }
    }

    for requirement in resolution
        .overrides
        .global_requirements()
        .filter(|requirement| requirement.evaluate_markers(env.marker_environment(), &[]))
    {
        if let Some(origin) = &requirement.origin {
            sources.add(
                &requirement.name,
                SourceAnnotation::Override(origin.clone()),
            );
        }
    }

    for edge in resolution.graph.edge_references() {
        let (Some(ResolutionGraphNode::Dist(parent)), Some(ResolutionGraphNode::Dist(dependency))) = (
            resolution.graph.node_weight(edge.source()),
            resolution.graph.node_weight(edge.target()),
        ) else {
            continue;
        };
        for requirement in resolution
            .overrides
            .scoped_requirements_for(&parent.name, &parent.version)
            .filter(|requirement| requirement.name == dependency.name)
            .filter(|requirement| requirement.evaluate_markers(env.marker_environment(), &[]))
        {
            if let Some(origin) = &requirement.origin {
                sources.add(
                    &requirement.name,
                    SourceAnnotation::Override(origin.clone()),
                );
            }
        }
    }

    sources
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
                node.markers = node.markers.or(dist.markers);
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

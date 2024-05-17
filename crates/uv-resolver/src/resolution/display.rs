use std::borrow::Cow;
use std::collections::BTreeSet;

use itertools::Itertools;
use owo_colors::OwoColorize;
use petgraph::visit::EdgeRef;
use petgraph::Direction;

use distribution_types::{
    DistributionMetadata, IndexUrl, LocalEditable, Name, SourceAnnotations, Verbatim,
};
use pypi_types::HashDigest;
use uv_normalize::PackageName;

use crate::resolution::AnnotatedDist;
use crate::ResolutionGraph;

/// A [`std::fmt::Display`] implementation for the resolution graph.
#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct DisplayResolutionGraph<'a> {
    /// The underlying graph.
    resolution: &'a ResolutionGraph,
    /// The packages to exclude from the output.
    no_emit_packages: &'a [PackageName],
    /// Whether to include hashes in the output.
    show_hashes: bool,
    /// Whether to include extras in the output (e.g., `black[colorama]`).
    include_extras: bool,
    /// Whether to include annotations in the output, to indicate which dependency or dependencies
    /// requested each package.
    include_annotations: bool,
    /// Whether to include indexes in the output, to indicate which index was used for each package.
    include_index_annotation: bool,
    /// The style of annotation comments, used to indicate the dependencies that requested each
    /// package.
    annotation_style: AnnotationStyle,
    /// External sources for each package: requirements, constraints, and overrides.
    sources: SourceAnnotations,
}

impl<'a> From<&'a ResolutionGraph> for DisplayResolutionGraph<'a> {
    fn from(resolution: &'a ResolutionGraph) -> Self {
        Self::new(
            resolution,
            &[],
            false,
            false,
            true,
            false,
            AnnotationStyle::default(),
            SourceAnnotations::default(),
        )
    }
}

impl<'a> DisplayResolutionGraph<'a> {
    /// Create a new [`DisplayResolutionGraph`] for the given graph.
    #[allow(clippy::fn_params_excessive_bools, clippy::too_many_arguments)]
    pub fn new(
        underlying: &'a ResolutionGraph,
        no_emit_packages: &'a [PackageName],
        show_hashes: bool,
        include_extras: bool,
        include_annotations: bool,
        include_index_annotation: bool,
        annotation_style: AnnotationStyle,
        sources: SourceAnnotations,
    ) -> DisplayResolutionGraph<'a> {
        Self {
            resolution: underlying,
            no_emit_packages,
            show_hashes,
            include_extras,
            include_annotations,
            include_index_annotation,
            annotation_style,
            sources,
        }
    }
}

#[derive(Debug)]
enum Node<'a> {
    /// A node linked to an editable distribution.
    Editable(&'a LocalEditable),
    /// A node linked to a non-editable distribution.
    Distribution(&'a AnnotatedDist),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NodeKey<'a> {
    /// A node linked to an editable distribution, sorted by verbatim representation.
    Editable(Cow<'a, str>),
    /// A node linked to a non-editable distribution, sorted by package name.
    Distribution(&'a PackageName),
}

impl<'a> Node<'a> {
    /// Return a comparable key for the node.
    fn key(&self) -> NodeKey<'a> {
        match self {
            Node::Editable(editable) => NodeKey::Editable(editable.verbatim()),
            Node::Distribution(annotated) => NodeKey::Distribution(annotated.name()),
        }
    }

    /// Return the [`IndexUrl`] of the distribution, if any.
    fn index(&self) -> Option<&IndexUrl> {
        match self {
            Node::Editable(_) => None,
            Node::Distribution(annotated) => annotated.dist.index(),
        }
    }

    /// Return the hashes of the distribution.
    fn hashes(&self) -> &[HashDigest] {
        match self {
            Node::Editable(_) => &[],
            Node::Distribution(annotated) => &annotated.hashes,
        }
    }
}

/// Write the graph in the `{name}=={version}` format of requirements.txt that pip uses.
impl std::fmt::Display for DisplayResolutionGraph<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Collect all packages.
        let mut nodes = self
            .resolution
            .petgraph
            .node_indices()
            .filter_map(|index| {
                let dist = &self.resolution.petgraph[index];
                let name = dist.name();
                if self.no_emit_packages.contains(name) {
                    return None;
                }

                let node = if let Some(editable) = self.resolution.editables.get(name) {
                    Node::Editable(&editable.built)
                } else {
                    Node::Distribution(dist)
                };
                Some((index, node))
            })
            .collect::<Vec<_>>();

        // Sort the nodes by name, but with editable packages first.
        nodes.sort_unstable_by_key(|(index, node)| (node.key(), *index));

        // Print out the dependency graph.
        for (index, node) in nodes {
            // Display the node itself.
            let mut line = match node {
                Node::Editable(editable) => format!("-e {}", editable.verbatim()),
                Node::Distribution(dist) => {
                    if self.include_extras && !dist.extras.is_empty() {
                        let mut extras = dist.extras.clone();
                        extras.sort_unstable();
                        extras.dedup();
                        format!(
                            "{}[{}]{}",
                            dist.name(),
                            extras.into_iter().join(", "),
                            dist.version_or_url().verbatim()
                        )
                    } else {
                        dist.verbatim().to_string()
                    }
                }
            };

            // Display the distribution hashes, if any.
            let mut has_hashes = false;
            if self.show_hashes {
                for hash in node.hashes() {
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
                // Display all dependencies.
                let mut edges = self
                    .resolution
                    .petgraph
                    .edges_directed(index, Direction::Incoming)
                    .map(|edge| &self.resolution.petgraph[edge.source()])
                    .collect::<Vec<_>>();
                edges.sort_unstable_by_key(|package| package.name());

                // Include all external sources (e.g., requirements files).
                let default = BTreeSet::default();
                let source = match node {
                    Node::Editable(editable) => {
                        self.sources.get_editable(&editable.url).unwrap_or(&default)
                    }
                    Node::Distribution(dist) => self.sources.get(dist.name()).unwrap_or(&default),
                };

                match self.annotation_style {
                    AnnotationStyle::Line => match edges.as_slice() {
                        [] if source.is_empty() => {}
                        [] if source.len() == 1 => {
                            let separator = if has_hashes { "\n    " } else { "  " };
                            let comment = format!("# via {}", source.iter().next().unwrap())
                                .green()
                                .to_string();
                            annotation = Some((separator, comment));
                        }
                        edges => {
                            let separator = if has_hashes { "\n    " } else { "  " };
                            let deps = edges
                                .iter()
                                .map(|dependency| format!("{}", dependency.name()))
                                .chain(source.iter().map(std::string::ToString::to_string))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let comment = format!("# via {deps}").green().to_string();
                            annotation = Some((separator, comment));
                        }
                    },
                    AnnotationStyle::Split => match edges.as_slice() {
                        [] if source.is_empty() => {}
                        [] if source.len() == 1 => {
                            let separator = "\n";
                            let comment = format!("    # via {}", source.iter().next().unwrap())
                                .green()
                                .to_string();
                            annotation = Some((separator, comment));
                        }
                        [edge] if source.is_empty() => {
                            let separator = "\n";
                            let comment = format!("    # via {}", edge.name()).green().to_string();
                            annotation = Some((separator, comment));
                        }
                        edges => {
                            let separator = "\n";
                            let deps = source
                                .iter()
                                .map(std::string::ToString::to_string)
                                .chain(
                                    edges
                                        .iter()
                                        .map(|dependency| format!("{}", dependency.name())),
                                )
                                .map(|name| format!("    #   {name}"))
                                .collect::<Vec<_>>()
                                .join("\n");
                            let comment = format!("    # via\n{deps}").green().to_string();
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
                if let Some(index) = node.index() {
                    let url = index.redacted();
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

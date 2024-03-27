use std::borrow::Cow;
use std::hash::BuildHasherDefault;

use anyhow::Result;
use dashmap::DashMap;
use itertools::Itertools;
use owo_colors::OwoColorize;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use pubgrub::range::Range;
use pubgrub::solver::{Kind, State};
use pubgrub::type_aliases::SelectedDependencies;
use rustc_hash::{FxHashMap, FxHashSet};
use url::Url;

use crate::dependency_provider::UvDependencyProvider;
use distribution_types::{
    Dist, DistributionMetadata, LocalEditable, Name, PackageId, Verbatim, VersionOrUrl,
};
use once_map::OnceMap;
use pep440_rs::Version;
use pep508_rs::MarkerEnvironment;
use pypi_types::{Hashes, Metadata23};
use uv_normalize::{ExtraName, PackageName};

use crate::editables::Editables;
use crate::pins::FilePins;
use crate::preferences::Preferences;
use crate::pubgrub::{PubGrubDistribution, PubGrubPackage};
use crate::redirect::apply_redirect;
use crate::resolver::{InMemoryIndex, VersionsResponse};
use crate::{Manifest, ResolveError};

/// Indicate the style of annotation comments, used to indicate the dependencies that requested each
/// package.
#[derive(Debug, Default, Copy, Clone, PartialEq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum AnnotationStyle {
    /// Render the annotations on a single, comma-separated line.
    Line,
    /// Render each annotation on its own line.
    #[default]
    Split,
}

/// A complete resolution graph in which every node represents a pinned package and every edge
/// represents a dependency between two pinned packages.
#[derive(Debug)]
pub struct ResolutionGraph {
    /// The underlying graph.
    petgraph: petgraph::graph::Graph<Dist, Range<Version>, petgraph::Directed>,
    /// The metadata for every distribution in this resolution.
    hashes: FxHashMap<PackageName, Vec<Hashes>>,
    /// The enabled extras for every distribution in this resolution.
    extras: FxHashMap<PackageName, Vec<ExtraName>>,
    /// The set of editable requirements in this resolution.
    editables: Editables,
    /// Any diagnostics that were encountered while building the graph.
    diagnostics: Vec<Diagnostic>,
}

impl ResolutionGraph {
    /// Create a new graph from the resolved `PubGrub` state.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_state(
        selection: &SelectedDependencies<UvDependencyProvider>,
        pins: &FilePins,
        packages: &OnceMap<PackageName, VersionsResponse>,
        distributions: &OnceMap<PackageId, Metadata23>,
        redirects: &DashMap<Url, Url>,
        state: &State<UvDependencyProvider>,
        preferences: &Preferences,
        editables: Editables,
    ) -> Result<Self, ResolveError> {
        // TODO(charlie): petgraph is a really heavy and unnecessary dependency here. We should
        // write our own graph, given that our requirements are so simple.
        let mut petgraph = petgraph::graph::Graph::with_capacity(selection.len(), selection.len());
        let mut hashes =
            FxHashMap::with_capacity_and_hasher(selection.len(), BuildHasherDefault::default());
        let mut extras = FxHashMap::default();
        let mut diagnostics = Vec::new();

        // Add every package to the graph.
        let mut inverse =
            FxHashMap::with_capacity_and_hasher(selection.len(), BuildHasherDefault::default());
        for (package, version) in selection {
            match package {
                PubGrubPackage::Package(package_name, None, None) => {
                    // Create the distribution.
                    let pinned_package = if let Some((editable, _)) = editables.get(package_name) {
                        Dist::from_editable(package_name.clone(), editable.clone())?
                    } else {
                        pins.get(package_name, version)
                            .expect("Every package should be pinned")
                            .clone()
                    };

                    // Add its hashes to the index, preserving those that were already present in
                    // the lockfile if necessary.
                    if let Some(hash) = preferences.match_hashes(package_name, version) {
                        hashes.insert(package_name.clone(), hash.to_vec());
                    } else if let Some(versions_response) = packages.get(package_name) {
                        if let VersionsResponse::Found(ref version_map) = *versions_response {
                            hashes.insert(package_name.clone(), {
                                let mut hash = version_map.hashes(version);
                                hash.sort_unstable();
                                hash
                            });
                        }
                    }

                    // Add the distribution to the graph.
                    let index = petgraph.add_node(pinned_package);
                    inverse.insert(package_name, index);
                }
                PubGrubPackage::Package(package_name, None, Some(url)) => {
                    // Create the distribution.
                    let pinned_package = if let Some((editable, _)) = editables.get(package_name) {
                        Dist::from_editable(package_name.clone(), editable.clone())?
                    } else {
                        let url = redirects.get(url).map_or_else(
                            || url.clone(),
                            |precise| apply_redirect(url, precise.value()),
                        );
                        Dist::from_url(package_name.clone(), url)?
                    };

                    // Add its hashes to the index, preserving those that were already present in
                    // the lockfile if necessary.
                    if let Some(hash) = preferences.match_hashes(package_name, version) {
                        hashes.insert(package_name.clone(), hash.to_vec());
                    } else if let Some(versions_response) = packages.get(package_name) {
                        if let VersionsResponse::Found(ref version_map) = *versions_response {
                            hashes.insert(package_name.clone(), {
                                let mut hash = version_map.hashes(version);
                                hash.sort_unstable();
                                hash
                            });
                        }
                    }

                    // Add the distribution to the graph.
                    let index = petgraph.add_node(pinned_package);
                    inverse.insert(package_name, index);
                }
                PubGrubPackage::Package(package_name, Some(extra), None) => {
                    // Validate that the `extra` exists.
                    let dist = PubGrubDistribution::from_registry(package_name, version);

                    if let Some((editable, metadata)) = editables.get(package_name) {
                        if metadata.provides_extras.contains(extra) {
                            extras
                                .entry(package_name.clone())
                                .or_insert_with(Vec::new)
                                .push(extra.clone());
                        } else {
                            let pinned_package =
                                Dist::from_editable(package_name.clone(), editable.clone())?;

                            diagnostics.push(Diagnostic::MissingExtra {
                                dist: pinned_package,
                                extra: extra.clone(),
                            });
                        }
                    } else {
                        let metadata = distributions.get(&dist.package_id()).unwrap_or_else(|| {
                            panic!(
                                "Every package should have metadata: {:?}",
                                dist.package_id()
                            )
                        });

                        if metadata.provides_extras.contains(extra) {
                            extras
                                .entry(package_name.clone())
                                .or_insert_with(Vec::new)
                                .push(extra.clone());
                        } else {
                            let pinned_package = pins
                                .get(package_name, version)
                                .unwrap_or_else(|| {
                                    panic!("Every package should be pinned: {package_name:?}")
                                })
                                .clone();

                            diagnostics.push(Diagnostic::MissingExtra {
                                dist: pinned_package,
                                extra: extra.clone(),
                            });
                        }
                    }
                }
                PubGrubPackage::Package(package_name, Some(extra), Some(url)) => {
                    // Validate that the `extra` exists.
                    let dist = PubGrubDistribution::from_url(package_name, url);

                    if let Some((editable, metadata)) = editables.get(package_name) {
                        if metadata.provides_extras.contains(extra) {
                            extras
                                .entry(package_name.clone())
                                .or_insert_with(Vec::new)
                                .push(extra.clone());
                        } else {
                            let pinned_package =
                                Dist::from_editable(package_name.clone(), editable.clone())?;

                            diagnostics.push(Diagnostic::MissingExtra {
                                dist: pinned_package,
                                extra: extra.clone(),
                            });
                        }
                    } else {
                        let metadata = distributions.get(&dist.package_id()).unwrap_or_else(|| {
                            panic!(
                                "Every package should have metadata: {:?}",
                                dist.package_id()
                            )
                        });

                        if metadata.provides_extras.contains(extra) {
                            extras
                                .entry(package_name.clone())
                                .or_insert_with(Vec::new)
                                .push(extra.clone());
                        } else {
                            let url = redirects.get(url).map_or_else(
                                || url.clone(),
                                |precise| apply_redirect(url, precise.value()),
                            );
                            let pinned_package = Dist::from_url(package_name.clone(), url)?;

                            diagnostics.push(Diagnostic::MissingExtra {
                                dist: pinned_package,
                                extra: extra.clone(),
                            });
                        }
                    }
                }
                _ => {}
            };
        }

        // Add every edge to the graph.
        for (package, version) in selection {
            for id in &state.incompatibilities[package] {
                if let Kind::FromDependencyOf(
                    self_package,
                    self_version,
                    dependency_package,
                    dependency_range,
                ) = &state.incompatibility_store[*id].kind
                {
                    // `Kind::FromDependencyOf` will include inverse dependencies. That is, if we're
                    // looking for a package `A`, this list will include incompatibilities of
                    // package `B` _depending on_ `A`. We're only interested in packages that `A`
                    // depends on.
                    if package != self_package {
                        continue;
                    }

                    let PubGrubPackage::Package(self_package, _, _) = self_package else {
                        continue;
                    };
                    let PubGrubPackage::Package(dependency_package, _, _) = dependency_package
                    else {
                        continue;
                    };

                    // For extras, we include a dependency between the extra and the base package.
                    if self_package == dependency_package {
                        continue;
                    }

                    if self_version.contains(version) {
                        let self_index = &inverse[self_package];
                        let dependency_index = &inverse[dependency_package];
                        petgraph.update_edge(
                            *self_index,
                            *dependency_index,
                            dependency_range.clone(),
                        );
                    }
                }
            }
        }

        Ok(Self {
            petgraph,
            hashes,
            extras,
            editables,
            diagnostics,
        })
    }

    /// Return the number of packages in the graph.
    pub fn len(&self) -> usize {
        self.petgraph.node_count()
    }

    /// Return `true` if there are no packages in the graph.
    pub fn is_empty(&self) -> bool {
        self.petgraph.node_count() == 0
    }

    /// Returns `true` if the graph contains the given package.
    pub fn contains(&self, name: &PackageName) -> bool {
        self.petgraph
            .node_indices()
            .any(|index| self.petgraph[index].name() == name)
    }

    /// Iterate over the [`Dist`] entities in this resolution.
    pub fn into_distributions(self) -> impl Iterator<Item = Dist> {
        self.petgraph
            .into_nodes_edges()
            .0
            .into_iter()
            .map(|node| node.weight)
    }

    /// Return the [`Diagnostic`]s that were encountered while building the graph.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Return the underlying graph.
    pub fn petgraph(&self) -> &petgraph::graph::Graph<Dist, Range<Version>, petgraph::Directed> {
        &self.petgraph
    }

    /// Return the marker tree specific to this resolution.
    ///
    /// This accepts a manifest, in-memory-index and marker environment. All
    /// of which should be the same values given to the resolver that produced
    /// this graph.
    ///
    /// The marker tree returned corresponds to an expression that, when true,
    /// this resolution is guaranteed to be correct. Note though that it's
    /// possible for resolution to be correct even if the returned marker
    /// expression is false.
    ///
    /// For example, if the root package has a dependency `foo; sys_platform ==
    /// "macos"` and resolution was performed on Linux, then the marker tree
    /// returned will contain a `sys_platform == "linux"` expression. This
    /// means that whenever the marker expression evaluates to true (i.e., the
    /// current platform is Linux), then the resolution here is correct. But
    /// it is possible that the resolution is also correct on other platforms
    /// that aren't macOS, such as Windows. (It is unclear at time of writing
    /// whether this is fundamentally impossible to compute, or just impossible
    /// to compute in some cases.)
    pub fn marker_tree(
        &self,
        manifest: &Manifest,
        index: &InMemoryIndex,
        marker_env: &MarkerEnvironment,
    ) -> pep508_rs::MarkerTree {
        use pep508_rs::{
            MarkerExpression, MarkerOperator, MarkerTree, MarkerValue, MarkerValueString,
            MarkerValueVersion,
        };

        /// A subset of the possible marker values.
        ///
        /// We only track the marker parameters that are referenced in a marker
        /// expression. We'll use references to the parameter later to generate
        /// values based on the current marker environment.
        #[derive(Debug, Eq, Hash, PartialEq)]
        enum MarkerParam {
            Version(MarkerValueVersion),
            String(MarkerValueString),
        }

        /// Add all marker parameters from the given tree to the given set.
        fn add_marker_params_from_tree(marker_tree: &MarkerTree, set: &mut FxHashSet<MarkerParam>) {
            match *marker_tree {
                MarkerTree::Expression(ref expr) => {
                    add_marker_value(&expr.l_value, set);
                    add_marker_value(&expr.r_value, set);
                }
                MarkerTree::And(ref exprs) | MarkerTree::Or(ref exprs) => {
                    for expr in exprs {
                        add_marker_params_from_tree(expr, set);
                    }
                }
            }
        }

        /// Add the marker value, if it's a marker parameter, to the set
        /// given.
        fn add_marker_value(value: &MarkerValue, set: &mut FxHashSet<MarkerParam>) {
            match *value {
                MarkerValue::MarkerEnvVersion(ref value_version) => {
                    set.insert(MarkerParam::Version(value_version.clone()));
                }
                MarkerValue::MarkerEnvString(ref value_string) => {
                    set.insert(MarkerParam::String(value_string.clone()));
                }
                // We specifically don't care about these for the
                // purposes of generating a marker string for a lock
                // file. Quoted strings are marker values given by the
                // user. We don't track those here, since we're only
                // interested in which markers are used.
                MarkerValue::Extra | MarkerValue::QuotedString(_) => {}
            }
        }

        let mut seen_marker_values = FxHashSet::default();
        for i in self.petgraph.node_indices() {
            let dist = &self.petgraph[i];
            let package_id = match dist.version_or_url() {
                VersionOrUrl::Version(version) => {
                    PackageId::from_registry(dist.name().clone(), version.clone())
                }
                VersionOrUrl::Url(verbatim_url) => PackageId::from_url(verbatim_url.raw()),
            };
            let md = index
                .distributions
                .get(&package_id)
                .expect("every package in resolution graph has metadata");
            for req in &md.requires_dist {
                let Some(ref marker_tree) = req.marker else {
                    continue;
                };
                add_marker_params_from_tree(marker_tree, &mut seen_marker_values);
            }
        }
        let direct_reqs = manifest
            .requirements
            .iter()
            .chain(&manifest.constraints)
            .chain(&manifest.overrides);
        for direct_req in direct_reqs {
            let Some(ref marker_tree) = direct_req.marker else {
                continue;
            };
            add_marker_params_from_tree(marker_tree, &mut seen_marker_values);
        }

        // Generate the final marker expression as a conjunction of
        // strict equality terms.
        let mut conjuncts = vec![];
        for marker_param in seen_marker_values {
            let expr = match marker_param {
                MarkerParam::Version(value_version) => {
                    let from_env = marker_env.get_version(&value_version);
                    MarkerExpression {
                        l_value: MarkerValue::MarkerEnvVersion(value_version),
                        operator: MarkerOperator::Equal,
                        r_value: MarkerValue::QuotedString(from_env.to_string()),
                    }
                }
                MarkerParam::String(value_string) => {
                    let from_env = marker_env.get_string(&value_string);
                    MarkerExpression {
                        l_value: MarkerValue::MarkerEnvString(value_string),
                        operator: MarkerOperator::Equal,
                        r_value: MarkerValue::QuotedString(from_env.to_string()),
                    }
                }
            };
            conjuncts.push(MarkerTree::Expression(expr));
        }
        MarkerTree::And(conjuncts)
    }
}

/// A [`std::fmt::Display`] implementation for the resolution graph.
#[derive(Debug)]
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
    /// The style of annotation comments, used to indicate the dependencies that requested each
    /// package.
    annotation_style: AnnotationStyle,
}

impl<'a> From<&'a ResolutionGraph> for DisplayResolutionGraph<'a> {
    fn from(resolution: &'a ResolutionGraph) -> Self {
        Self::new(
            resolution,
            &[],
            false,
            false,
            true,
            AnnotationStyle::default(),
        )
    }
}

impl<'a> DisplayResolutionGraph<'a> {
    /// Create a new [`DisplayResolutionGraph`] for the given graph.
    pub fn new(
        underlying: &'a ResolutionGraph,
        no_emit_packages: &'a [PackageName],
        show_hashes: bool,
        include_extras: bool,
        include_annotations: bool,
        annotation_style: AnnotationStyle,
    ) -> DisplayResolutionGraph<'a> {
        Self {
            resolution: underlying,
            no_emit_packages,
            show_hashes,
            include_extras,
            include_annotations,
            annotation_style,
        }
    }
}

#[derive(Debug)]
enum Node<'a> {
    /// A node linked to an editable distribution.
    Editable(&'a PackageName, &'a LocalEditable),
    /// A node linked to a non-editable distribution.
    Distribution(&'a PackageName, &'a Dist, &'a [ExtraName]),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NodeKey<'a> {
    /// A node linked to an editable distribution, sorted by verbatim representation.
    Editable(Cow<'a, str>),
    /// A node linked to a non-editable distribution, sorted by package name.
    Distribution(&'a PackageName),
}

impl<'a> Node<'a> {
    /// Return the name of the package.
    fn name(&self) -> &'a PackageName {
        match self {
            Node::Editable(name, _) => name,
            Node::Distribution(name, _, _) => name,
        }
    }

    /// Return a comparable key for the node.
    fn key(&self) -> NodeKey<'a> {
        match self {
            Node::Editable(_, editable) => NodeKey::Editable(editable.verbatim()),
            Node::Distribution(name, _, _) => NodeKey::Distribution(name),
        }
    }
}

impl Verbatim for Node<'_> {
    fn verbatim(&self) -> Cow<'_, str> {
        match self {
            Node::Editable(_, editable) => Cow::Owned(format!("-e {}", editable.verbatim())),
            Node::Distribution(_, dist, &[]) => dist.verbatim(),
            Node::Distribution(_, dist, extras) => {
                let mut extras = extras.to_vec();
                extras.sort_unstable();
                extras.dedup();
                Cow::Owned(format!(
                    "{}[{}]{}",
                    dist.name(),
                    extras.into_iter().join(", "),
                    dist.version_or_url().verbatim()
                ))
            }
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

                let node = if let Some((editable, _)) = self.resolution.editables.get(name) {
                    Node::Editable(name, editable)
                } else if self.include_extras {
                    Node::Distribution(
                        name,
                        dist,
                        self.resolution
                            .extras
                            .get(name)
                            .map_or(&[], |extras| extras.as_slice()),
                    )
                } else {
                    Node::Distribution(name, dist, &[])
                };
                Some((index, node))
            })
            .collect::<Vec<_>>();

        // Sort the nodes by name, but with editable packages first.
        nodes.sort_unstable_by_key(|(index, node)| (node.key(), *index));

        // Print out the dependency graph.
        for (index, node) in nodes {
            // Display the node itself.
            let mut line = node.verbatim().to_string();

            // Display the distribution hashes, if any.
            let mut has_hashes = false;
            if self.show_hashes {
                if let Some(hashes) = self
                    .resolution
                    .hashes
                    .get(node.name())
                    .filter(|hashes| !hashes.is_empty())
                {
                    for hash in hashes {
                        if let Some(hash) = hash.to_string() {
                            has_hashes = true;
                            line.push_str(" \\\n");
                            line.push_str("    --hash=");
                            line.push_str(&hash);
                        }
                    }
                }
            }

            // Determine the annotation comment and separator (between comment and requirement).
            let mut annotation = None;

            if self.include_annotations {
                // Display all dependencies.
                let mut edges = self
                    .resolution
                    .petgraph
                    .edges_directed(index, Direction::Incoming)
                    .map(|edge| &self.resolution.petgraph[edge.source()])
                    .collect::<Vec<_>>();
                edges.sort_unstable_by_key(|package| package.name());

                match self.annotation_style {
                    AnnotationStyle::Line => {
                        if !edges.is_empty() {
                            let separator = if has_hashes { "\n    " } else { "  " };
                            let deps = edges
                                .into_iter()
                                .map(|dependency| dependency.name().to_string())
                                .collect::<Vec<_>>()
                                .join(", ");
                            let comment = format!("# via {deps}").green().to_string();
                            annotation = Some((separator, comment));
                        }
                    }
                    AnnotationStyle::Split => match edges.as_slice() {
                        [] => {}
                        [edge] => {
                            let separator = "\n";
                            let comment = format!("    # via {}", edge.name()).green().to_string();
                            annotation = Some((separator, comment));
                        }
                        edges => {
                            let separator = "\n";
                            let deps = edges
                                .iter()
                                .map(|dependency| format!("    #   {}", dependency.name()))
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
        }

        Ok(())
    }
}

impl From<ResolutionGraph> for distribution_types::Resolution {
    fn from(graph: ResolutionGraph) -> Self {
        Self::new(
            graph
                .petgraph
                .node_indices()
                .map(|node| {
                    (
                        graph.petgraph[node].name().clone(),
                        graph.petgraph[node].clone(),
                    )
                })
                .collect(),
        )
    }
}

#[derive(Debug)]
pub enum Diagnostic {
    MissingExtra {
        /// The distribution that was requested with an non-existent extra. For example,
        /// `black==23.10.0`.
        dist: Dist,
        /// The extra that was requested. For example, `colorama` in `black[colorama]`.
        extra: ExtraName,
    },
}

impl Diagnostic {
    /// Convert the diagnostic into a user-facing message.
    pub fn message(&self) -> String {
        match self {
            Self::MissingExtra { dist, extra } => {
                format!("The package `{dist}` does not have an extra named `{extra}`.")
            }
        }
    }

    /// Returns `true` if the [`PackageName`] is involved in this diagnostic.
    pub fn includes(&self, name: &PackageName) -> bool {
        match self {
            Self::MissingExtra { dist, .. } => name == dist.name(),
        }
    }
}

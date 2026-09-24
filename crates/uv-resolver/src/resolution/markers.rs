use indexmap::IndexSet;
use uv_distribution::{DistributionMetadataIndex, MetadataResponse};
use uv_distribution_types::Identifier;
use uv_pep440::VersionSpecifier;
use uv_pep508::{MarkerEnvironment, MarkerTree, MarkerTreeKind};
use uv_pypi_types::ParsedUrlError;
use uv_resolver_types::{ResolutionGraphNode, ResolverOutput};

/// Return the marker tree specific to this resolution.
///
/// This accepts an in-memory-index and marker environment, all
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
pub fn resolution_marker_tree(
    resolution: &ResolverOutput,
    index: &DistributionMetadataIndex,
    marker_env: &MarkerEnvironment,
) -> Result<MarkerTree, Box<ParsedUrlError>> {
    use uv_pep508::{
        CanonicalMarkerValueString, CanonicalMarkerValueVersion, MarkerExpression, MarkerOperator,
        MarkerTree,
    };

    /// A subset of the possible marker values.
    ///
    /// We only track the marker parameters that are referenced in a marker
    /// expression. We'll use references to the parameter later to generate
    /// values based on the current marker environment.
    #[derive(Debug, Eq, Hash, PartialEq)]
    enum MarkerParam {
        Version(CanonicalMarkerValueVersion),
        String(CanonicalMarkerValueString),
    }

    /// Add all marker parameters from the given tree to the given set.
    fn add_marker_params_from_tree(marker_tree: MarkerTree, set: &mut IndexSet<MarkerParam>) {
        match marker_tree.kind() {
            MarkerTreeKind::True => {}
            MarkerTreeKind::False => {}
            MarkerTreeKind::Version(marker) => {
                set.insert(MarkerParam::Version(marker.key()));
                for (_, tree) in marker.edges() {
                    add_marker_params_from_tree(tree, set);
                }
            }
            MarkerTreeKind::VersionString(marker) => {
                set.insert(MarkerParam::String(marker.key()));
                for (_, tree) in marker.edges() {
                    add_marker_params_from_tree(tree, set);
                }
            }
            MarkerTreeKind::String(marker) => {
                set.insert(MarkerParam::String(marker.key()));
                for (_, tree) in marker.children() {
                    add_marker_params_from_tree(tree, set);
                }
            }
            MarkerTreeKind::In(marker) => {
                set.insert(MarkerParam::String(marker.key()));
                for (_, tree) in marker.children() {
                    add_marker_params_from_tree(tree, set);
                }
            }
            MarkerTreeKind::Contains(marker) => {
                set.insert(MarkerParam::String(marker.key()));
                for (_, tree) in marker.children() {
                    add_marker_params_from_tree(tree, set);
                }
            }
            // We specifically don't care about these for the
            // purposes of generating a marker string for a lock
            // file. Quoted strings are marker values given by the
            // user. We don't track those here, since we're only
            // interested in which markers are used.
            MarkerTreeKind::Extra(marker) => {
                for (_, tree) in marker.children() {
                    add_marker_params_from_tree(tree, set);
                }
            }
            MarkerTreeKind::List(marker) => {
                for (_, tree) in marker.children() {
                    add_marker_params_from_tree(tree, set);
                }
            }
        }
    }

    let mut seen_marker_values = IndexSet::default();
    for i in resolution.graph.node_indices() {
        let ResolutionGraphNode::Dist(dist) = &resolution.graph[i] else {
            continue;
        };
        let metadata_id = dist.dist.distribution_id();
        let res = index
            .get(&metadata_id)
            .expect("every package in resolution graph has metadata");
        let MetadataResponse::Found(archive, ..) = &*res else {
            panic!("Every package should have metadata: {metadata_id:?}")
        };
        for req in resolution.constraints.apply(resolution.overrides.apply_for(
            &dist.name,
            &dist.version,
            archive.metadata.requires_dist.iter(),
        )) {
            add_marker_params_from_tree(req.marker, &mut seen_marker_values);
        }
    }

    // Ensure that we consider markers from direct dependencies.
    for direct_req in resolution
        .constraints
        .apply(resolution.overrides.apply(resolution.requirements.iter()))
    {
        add_marker_params_from_tree(direct_req.marker, &mut seen_marker_values);
    }

    // Generate the final marker expression as a conjunction of
    // strict equality terms.
    let mut conjunction = MarkerTree::TRUE;
    for marker_param in seen_marker_values {
        let expr = match marker_param {
            MarkerParam::Version(value_version) => {
                let from_env = marker_env.get_version(value_version);
                MarkerExpression::Version {
                    key: value_version.into(),
                    specifier: VersionSpecifier::equals_version(from_env.clone()),
                }
            }
            MarkerParam::String(value_string) => {
                let from_env = marker_env.get_string(value_string);
                MarkerExpression::String {
                    key: value_string.into(),
                    operator: MarkerOperator::Equal,
                    value: from_env.into(),
                }
            }
        };
        conjunction = conjunction.and(MarkerTree::expression(expr));
    }
    Ok(conjunction)
}

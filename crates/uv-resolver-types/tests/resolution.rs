use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use petgraph::Graph;
use uv_configuration::{Constraints, DependencyModifiers};
use uv_distribution::{RequiresDist, UnversionedMetadata};
use uv_distribution_types::{
    DerivationChain, Dist, Name, RequiresPython, Resolution, ResolvedDist,
};
use uv_pep440::VersionSpecifiers;
use uv_pep508::{Pep508Url, VerbatimUrl};
use uv_pypi_types::{HashDigests, VerbatimParsedUrl};
use uv_resolver_types::{
    AnnotatedDist, ManifestNode, Options, PackageNodeKind, PackageRole, ResolutionGraphNode,
    ResolverOutput, UniversalMarker,
};

#[test]
fn retain_virtual_member_dependencies() -> Result<(), Box<dyn Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"));
    let app = Dist::from_directory_url(
        "app".parse()?,
        VerbatimUrl::from_absolute_path(path)?,
        path,
        None,
        Some(true),
    )?;
    let manifest = ManifestNode {
        metadata: Arc::new(UnversionedMetadata {
            requirements: RequiresDist {
                name: app.name().clone(),
                requires_dist: Box::default(),
                provides_extra: Box::default(),
                dependency_groups: BTreeMap::new(),
                dynamic: true,
            },
            requires_python: None,
        }),
        dist: Arc::new(app),
        kind: PackageNodeKind::Base,
        marker: UniversalMarker::TRUE,
    };
    let url = VerbatimParsedUrl::parse_url("https://example.com/plugin-1-py3-none-any.whl", None)?;
    let plugin = Arc::new(Dist::from_url("plugin".parse()?, url)?);

    let check = |kind: PackageNodeKind| -> Result<(), Box<dyn Error>> {
        let mut graph = Graph::new();
        let root = graph.add_node(ResolutionGraphNode::Root);
        let app = graph.add_node(ResolutionGraphNode::Manifest(manifest.clone()));
        let source = if kind.is_base() {
            app
        } else {
            graph.add_node(ResolutionGraphNode::Manifest(ManifestNode {
                kind: kind.clone(),
                ..manifest.clone()
            }))
        };
        let wheel = graph.add_node(ResolutionGraphNode::Dist(AnnotatedDist {
            name: plugin.name().clone(),
            version: "1".parse()?,
            dist: ResolvedDist::Installable {
                dist: plugin.clone(),
                version: Some("1".parse()?),
            },
            kind: PackageNodeKind::Base,
            hashes: HashDigests::empty(),
            metadata: None,
            role: PackageRole::Dependency,
            marker: UniversalMarker::TRUE,
        }));
        graph.add_edge(root, app, UniversalMarker::TRUE);
        graph.add_edge(source, wheel, UniversalMarker::TRUE);
        graph.add_edge(wheel, app, UniversalMarker::TRUE);

        let resolution = Resolution::from(ResolverOutput {
            graph,
            requires_python: RequiresPython::from_specifiers(VersionSpecifiers::empty()),
            fork_markers: vec![],
            diagnostics: vec![],
            requirements: vec![],
            constraints: Constraints::default(),
            modifiers: DependencyModifiers::default(),
            options: Options::default(),
        });
        assert_eq!(
            resolution
                .distributions()
                .map(|dist| dist.name().as_str())
                .collect::<Vec<_>>(),
            ["plugin"]
        );

        let chain = DerivationChain::from_resolution(&resolution, plugin.as_ref().into())
            .ok_or("missing dependency chain for plugin")?;
        let [step] = chain.iter().as_slice() else {
            panic!("expected the virtual member in the dependency chain: {chain:?}");
        };
        assert_eq!(step.name.as_str(), "app");
        assert_eq!(step.version, None);
        assert_eq!(step.extra.as_ref(), kind.extra());
        assert_eq!(step.group.as_ref(), kind.group());
        Ok(())
    };

    check(PackageNodeKind::Base)?;
    check(PackageNodeKind::Extra("docs".parse()?))?;
    check(PackageNodeKind::Group("qa".parse()?))
}

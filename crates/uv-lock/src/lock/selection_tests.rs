//! Exercise the activation model against parsed lockfiles with complete declaration metadata.
//!
//! The adapter deliberately requires declarations alongside resolved edges. Existing lockfiles
//! can omit these declarations, so this is not a fallback for the production reader.

use std::collections::BTreeMap;

use petgraph::graph::NodeIndex;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::MarkerTree;
use uv_pypi_types::ConflictItem;
use uv_resolver_types::selection::{ActivationGraph, ActivationNode, SelectionError};
use uv_resolver_types::{ConflictMarker, UniversalMarker};

use super::{DependencyContext, Lock};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Section {
    Production,
    Extra(ExtraName),
    Group(GroupName),
}

/// Retain original requests separately from the guards choosing their resolved target versions.
fn activation_graph(lock: &Lock) -> (ActivationGraph, BTreeMap<(usize, Section), NodeIndex>) {
    let mut graph = ActivationGraph::default();
    let mut nodes = BTreeMap::new();
    for (index, package) in lock.packages.iter().enumerate() {
        let production = graph.graph.add_node(ActivationNode::default());
        nodes.insert((index, Section::Production), production);
        for extra in &package.metadata.provides_extra {
            let node = graph.graph.add_node(ActivationNode {
                requests: vec![ConflictItem::from((package.id.name.clone(), extra.clone()))],
                requires_choice: false,
            });
            graph.graph.add_edge(node, production, MarkerTree::TRUE);
            nodes.insert((index, Section::Extra(extra.clone())), node);
        }
        for group in package.metadata.dependency_groups.keys() {
            let node = graph.graph.add_node(ActivationNode {
                requests: vec![ConflictItem::from((package.id.name.clone(), group.clone()))],
                requires_choice: false,
            });
            nodes.insert((index, Section::Group(group.clone())), node);
        }
    }
    for ((index, section), parent) in &nodes {
        let package = &lock.packages[*index];
        let (requirements, context, extras) = match section {
            Section::Production => (
                &package.metadata.requires_dist,
                DependencyContext::Production,
                vec![],
            ),
            Section::Extra(extra) => (
                &package.metadata.requires_dist,
                DependencyContext::Extra(extra),
                vec![(extra.clone(), MarkerTree::TRUE)],
            ),
            Section::Group(group) => (
                &package.metadata.dependency_groups[group],
                DependencyContext::Group(group),
                vec![],
            ),
        };
        for requirement in requirements {
            let condition = requirement.marker.resolve_extras(extras.clone());
            let condition = match section {
                Section::Extra(_) => {
                    // The production node already supplies unconditional requirements.
                    condition.and(requirement.marker.resolve_extras([]).negate())
                }
                Section::Production | Section::Group(_) => condition,
            };
            if condition.is_false() {
                continue;
            }
            let request = graph.graph.add_node(ActivationNode {
                requests: requirement
                    .extras
                    .iter()
                    .map(|extra| ConflictItem::from((requirement.name.clone(), extra.clone())))
                    .collect(),
                requires_choice: true,
            });
            graph.graph.add_edge(*parent, request, condition);

            // The fixtures use a single requirement per package name in each environment.
            // A production adapter must also match source and version requirements here.
            let mut candidates = BTreeMap::new();
            for dependency in context.dependencies(package) {
                if dependency.package_id.name != requirement.name {
                    continue;
                }
                let guard = candidates
                    .entry(dependency.index.0)
                    .or_insert(MarkerTree::FALSE);
                *guard = guard.or(dependency.complexified_marker.combined());
            }
            for (target, guard) in candidates {
                let choice = graph.graph.add_node(ActivationNode::default());
                graph.graph.add_edge(request, choice, guard);
                graph.graph.add_edge(
                    choice,
                    nodes[&(target, Section::Production)],
                    MarkerTree::TRUE,
                );
                for extra in &requirement.extras {
                    if let Some(extra_node) = nodes.get(&(target, Section::Extra(extra.clone()))) {
                        graph.graph.add_edge(choice, *extra_node, MarkerTree::TRUE);
                    }
                }
            }
        }
    }
    (graph, nodes)
}

fn package_index(lock: &Lock, name: &str, version: Option<&str>) -> usize {
    lock.packages
        .iter()
        .position(|package| {
            package.id.name.as_str() == name
                && version.is_none_or(|version| {
                    package
                        .id
                        .version
                        .as_ref()
                        .is_some_and(|actual| actual.to_string() == version)
                })
        })
        .expect("fixture package")
}

#[test]
fn requests_survive_pruning_impossible_installation_edges() {
    let mut lock: Lock = toml::from_str(r#"
version = 1
revision = 5
requires-python = ">=3.12"
conflicts = [[{ package = "example", extra = "foo" }, { package = "example", extra = "bar" }]]

[[package]]
name = "example"
source = { virtual = "." }
[package.optional-dependencies]
foo = [{ name = "indirection" }]
bar = []
[package.metadata]
requires-dist = [{ name = "indirection", marker = "extra == 'foo'", virtual = "indirection" }]
provides-extras = ["foo", "bar"]

[[package]]
name = "indirection"
source = { virtual = "indirection" }
dependencies = [
    { name = "example" },
    { name = "example", extra = ["bar"], marker = "extra == 'extra-7-example-foo' and extra == 'extra-7-example-bar'" },
]
[package.metadata]
requires-dist = [{ name = "example", extras = ["bar"], virtual = "." }]
"#).expect("valid lock");

    let constraints = UniversalMarker::new(
        MarkerTree::TRUE,
        ConflictMarker::from_conflicts(&lock.conflicts),
    )
    .combined();
    let indirection = package_index(&lock, "indirection", None);
    let dependencies = &mut lock.packages[indirection].dependencies;
    assert_eq!(dependencies.len(), 2);
    for dependency in &mut *dependencies {
        dependency.complexified_marker = UniversalMarker::from_combined(
            dependency
                .complexified_marker
                .combined()
                .restrict(constraints),
        );
    }
    dependencies.retain(|dependency| !dependency.complexified_marker.is_false());
    assert_eq!(dependencies.len(), 1);
    assert!(dependencies[0].extra.is_empty());

    let environment = lock.requires_python.to_marker_tree();
    let (graph, nodes) = activation_graph(&lock);
    let example = package_index(&lock, "example", None);
    let foo = nodes[&(example, Section::Extra("foo".parse().expect("extra")))];
    assert_eq!(
        graph
            .select([(foo, MarkerTree::TRUE)], environment, &lock.conflicts)
            .expect_err("the original bar request remains despite pruning"),
        SelectionError::Conflict {
            first: ConflictItem::from((
                "example".parse::<PackageName>().expect("package"),
                "bar".parse::<ExtraName>().expect("extra")
            )),
            second: ConflictItem::from((
                "example".parse::<PackageName>().expect("package"),
                "foo".parse::<ExtraName>().expect("extra")
            )),
            environment,
        }
    );

    let selected = graph
        .select(
            [(nodes[&(indirection, Section::Production)], MarkerTree::TRUE)],
            environment,
            &lock.conflicts,
        )
        .expect("selecting indirection alone only activates bar");
    let bar = nodes[&(example, Section::Extra("bar".parse().expect("extra")))];
    assert_eq!(selected.marker(foo), MarkerTree::FALSE);
    assert_eq!(selected.marker(bar), environment);
}

#[test]
fn locked_version_choices_do_not_union_unselected_requests() {
    let lock: Lock = toml::from_str(r#"
version = 1
revision = 5
requires-python = ">=3.12"
conflicts = [[{ package = "project", extra = "foo" }, { package = "project", extra = "bar" }]]

[[package]]
name = "project"
source = { virtual = "." }
[package.optional-dependencies]
foo = [
    { name = "chooser", version = "1.0.0", source = { virtual = "one" }, marker = "extra != 'extra-7-project-foo'" },
    { name = "chooser", version = "2.0.0", source = { virtual = "two" }, marker = "extra == 'extra-7-project-foo'" },
]
bar = []
[package.metadata]
requires-dist = [{ name = "chooser", marker = "extra == 'foo'", specifier = ">=1" }]
provides-extras = ["foo", "bar"]

[[package]]
name = "chooser"
version = "1.0.0"
source = { virtual = "one" }
dependencies = [{ name = "project", extra = ["bar"] }]
[package.metadata]
requires-dist = [{ name = "project", extras = ["bar"], virtual = "." }]

[[package]]
name = "chooser"
version = "2.0.0"
source = { virtual = "two" }
[package.metadata]
requires-dist = []
"#).expect("valid lock");
    let environment = lock.requires_python.to_marker_tree();
    let (graph, nodes) = activation_graph(&lock);
    let project = package_index(&lock, "project", None);
    let foo = nodes[&(project, Section::Extra("foo".parse().expect("extra")))];
    let selected = graph
        .select([(foo, MarkerTree::TRUE)], environment, &lock.conflicts)
        .expect("version two does not request bar");
    assert_eq!(
        selected.marker(
            nodes[&(
                package_index(&lock, "chooser", Some("1.0.0")),
                Section::Production
            )]
        ),
        MarkerTree::FALSE
    );
    assert_eq!(
        selected.marker(
            nodes[&(
                package_index(&lock, "chooser", Some("2.0.0")),
                Section::Production
            )]
        ),
        environment
    );
}

#[test]
fn selecting_extra_includes_base_dependencies() {
    let lock: Lock = toml::from_str(
        r#"
version = 1
revision = 5
requires-python = ">=3.12"

[[package]]
name = "project"
source = { virtual = "." }
dependencies = [{ name = "support" }]
[package.optional-dependencies]
dev = []
[package.metadata]
requires-dist = [{ name = "support", virtual = "support" }]
provides-extras = ["dev"]

[[package]]
name = "support"
source = { virtual = "support" }
"#,
    )
    .expect("valid lock");
    let environment = lock.requires_python.to_marker_tree();
    let (graph, nodes) = activation_graph(&lock);
    let project = package_index(&lock, "project", None);
    let dev = nodes[&(project, Section::Extra("dev".parse().expect("extra")))];
    let selected = graph
        .select([(dev, MarkerTree::TRUE)], environment, &lock.conflicts)
        .expect("an extra includes the base dependencies");
    assert_eq!(
        selected.marker(nodes[&(project, Section::Production)]),
        environment
    );
    assert_eq!(
        selected.marker(nodes[&(package_index(&lock, "support", None), Section::Production)]),
        environment
    );
}

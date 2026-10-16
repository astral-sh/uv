// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main, measurement::WallTime};
use uv_normalize::{ExtraName, PackageName};
use uv_pep508::MarkerTree;
use uv_pypi_types::{ConflictItem, ConflictSet, Conflicts};
use uv_resolver_types::selection::{ActivationGraph, ActivationNode};
use uv_resolver_types::{ConflictMarker, UniversalMarker};

/// Measure validation and projection, excluding construction of the activation graph.
/// Each extra selects a distinct version; exercise both ends of the marker variable order.
fn select(criterion: &mut Criterion<WallTime>) {
    let mut group = criterion.benchmark_group("selection/select");
    let package: PackageName = "project".parse().expect("package");
    for count in [2, 32, 128] {
        let items: Vec<_> = (0..count)
            .map(|index| {
                let extra: ExtraName = format!("e{index:03}").parse().expect("extra");
                ConflictItem::from((package.clone(), extra))
            })
            .collect();
        let mut conflicts = Conflicts::empty();
        conflicts.push(ConflictSet::try_from(items.clone()).expect("conflicts"));
        for (name, selected) in [("first_extra", 0), ("last_extra", count - 1)] {
            let mut graph = ActivationGraph::default();
            let root = graph.graph.add_node(ActivationNode {
                requests: vec![items[selected].clone()],
                requires_choice: true,
            });
            let mut versions = Vec::new();
            for item in &items {
                let version = graph.graph.add_node(ActivationNode::default());
                let guard = UniversalMarker::new(
                    MarkerTree::TRUE,
                    ConflictMarker::from_conflict_item(item),
                )
                .combined();
                graph.graph.add_edge(root, version, guard);
                versions.push(version);
            }
            let selection = graph
                .select([(root, MarkerTree::TRUE)], MarkerTree::TRUE, &conflicts)
                .expect("one selected version");
            for (index, version) in versions.iter().enumerate() {
                assert_eq!(selection.marker(*version).is_true(), index == selected);
            }
            group.bench_function(BenchmarkId::new(name, count), |benchmark| {
                benchmark.iter(|| {
                    black_box(&graph)
                        .select(
                            [(black_box(root), MarkerTree::TRUE)],
                            MarkerTree::TRUE,
                            black_box(&conflicts),
                        )
                        .expect("one selected version")
                });
            });
        }
    }
    group.finish();
}

criterion_group!(uv_resolver_types, select);
criterion_main!(uv_resolver_types);

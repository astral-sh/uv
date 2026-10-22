// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main, measurement::WallTime};

use uv_normalize::{ExtraName, PackageName};
use uv_pep508::MarkerTree;
use uv_pypi_types::{ConflictItem, ConflictSet, Conflicts};
use uv_resolver_types::{ConflictMarker, UniversalMarker};

/// Build the diagrams before timing conversion and simplification. Conflict guards add a clause
/// for every pair of extras.
fn to_dnf(criterion: &mut Criterion<WallTime>) {
    let mut group = criterion.benchmark_group("marker/to_dnf");
    for count in [128, 512] {
        let mut marker = MarkerTree::FALSE;
        for index in 0..count {
            marker = marker.or(format!("platform_machine == 'arch-{index}'")
                .parse()
                .expect("valid platform marker"));
        }
        assert_eq!(marker.to_dnf().len(), count);
        group.bench_function(
            BenchmarkId::new("marker_to_dnf_sparse_clauses", count),
            |benchmark| {
                benchmark.iter(|| black_box(marker).to_dnf());
            },
        );
    }

    for count in [16, 32] {
        let extras: Vec<MarkerTree> = (0..count)
            .map(|index| {
                format!("extra == 'e{index:03}'")
                    .parse()
                    .expect("valid extra marker")
            })
            .collect();
        let mut marker: MarkerTree = "python_full_version < '3.11'"
            .parse()
            .expect("valid environment marker");
        for left in 0..count {
            for right in left + 1..count {
                marker = marker.or(extras[left].and(extras[right]));
            }
        }
        assert_eq!(marker.to_dnf().len(), 1 + count * (count - 1) / 2);

        group.bench_function(
            BenchmarkId::new("marker_to_dnf_conflicting_extras", count),
            |benchmark| {
                benchmark.iter(|| black_box(marker).to_dnf());
            },
        );
    }
    group.finish();
}

/// Measure conflict simplification and formatting for an environment-only marker and for a marker
/// that depends on a selected extra. Copy the original marker on every iteration.
fn imbibe(criterion: &mut Criterion<WallTime>) {
    let package: PackageName = "project".parse().expect("valid package name");
    let environment: MarkerTree = "python_full_version < '3.11'"
        .parse()
        .expect("valid environment marker");
    let mut group = criterion.benchmark_group("marker/imbibe_and_format");
    for (count, depends_on_extra) in [(16, true), (32, false)] {
        let items: Vec<ConflictItem> = (0..count)
            .map(|index| {
                let extra: ExtraName = format!("e{index:03}").parse().expect("valid extra name");
                ConflictItem::from((package.clone(), extra))
            })
            .collect();
        let selected = ConflictMarker::from_conflict_item(&items[0]);
        let mut conflicts = Conflicts::empty();
        conflicts.push(ConflictSet::try_from(items).expect("valid conflicts"));
        let conflicts = ConflictMarker::from_conflicts(&conflicts);
        let (name, guard) = if depends_on_extra {
            (
                "imbibe_and_format_marker_conflict_dependent",
                conflicts.and(selected),
            )
        } else {
            ("imbibe_and_format_marker_environment_only", conflicts)
        };
        let marker = UniversalMarker::new(environment, guard);
        group.bench_function(BenchmarkId::new(name, count), |benchmark| {
            benchmark.iter(|| {
                let mut marker = black_box(marker);
                marker.imbibe(black_box(conflicts));
                black_box(marker.combined().try_to_string())
            });
        });
    }
    group.finish();
}

criterion_group!(uv_pep508, to_dnf, imbibe);
criterion_main!(uv_pep508);

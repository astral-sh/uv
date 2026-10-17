// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main, measurement::WallTime};
use rustc_hash::FxHashMap;

use uv_normalize::{ExtraName, PackageName};
use uv_pep508::MarkerTree;
use uv_pypi_types::{ConflictItem, ConflictSet, Conflicts};
use uv_resolver_types::universal_marker::resolve_activated_extras;
use uv_resolver_types::{ConflictMarker, UniversalMarker};

/// Build the diagrams before timing conversion and simplification. Include small markers and
/// sparse clauses as well as conflict guards, which add a clause for every pair of extras.
fn to_dnf(criterion: &mut Criterion<WallTime>) {
    let mut group = criterion.benchmark_group("marker/to_dnf");
    for (name, expression) in [
        ("marker_to_dnf_single_clause", "python_version >= '3.10'"),
        (
            "marker_to_dnf_two_clauses",
            "python_version >= '3.10' or sys_platform == 'linux'",
        ),
        (
            "marker_to_dnf_platform_specific_python_versions",
            "(sys_platform == 'linux' and python_version >= '3.10') or (sys_platform == 'win32' and python_version >= '3.11')",
        ),
    ] {
        let marker: MarkerTree = expression.parse().expect("valid marker");
        group.bench_function(name, |benchmark| {
            benchmark.iter(|| black_box(marker).to_dnf());
        });
    }

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

    for count in [4, 16, 32] {
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

/// Each independent pair adds two decisions but doubles the number of DNF paths.
/// Pair names keep their extras adjacent in the diagram's variable order.
/// Resolve one extra in each pair to a platform condition; the unknown extra is false.
fn resolve_extras(criterion: &mut Criterion<WallTime>) {
    let mut group = criterion.benchmark_group("marker/resolve_activated_extras");
    let package: PackageName = "project".parse().expect("valid package name");
    let condition: MarkerTree = "sys_platform == 'darwin'"
        .parse()
        .expect("valid environment marker");
    for pairs in [2, 4, 8] {
        let mut marker = MarkerTree::TRUE;
        let mut known_conflicts = FxHashMap::default();
        for index in 0..pairs {
            let pair = format!("extra == 'pair-{index:02}-a' or extra == 'pair-{index:02}-b'")
                .parse()
                .expect("valid extra markers");
            marker = marker.and(pair);
            let extra: ExtraName = format!("pair-{index:02}-a")
                .parse()
                .expect("valid extra name");
            known_conflicts.insert(ConflictItem::from((package.clone(), extra)), condition);
        }
        assert_eq!(
            resolve_activated_extras(marker, Some(&package), &known_conflicts),
            condition
        );

        group.bench_function(
            BenchmarkId::new("resolve_activated_extras_independent_pairs", pairs),
            |benchmark| {
                benchmark.iter(|| {
                    resolve_activated_extras(
                        black_box(marker),
                        Some(black_box(&package)),
                        black_box(&known_conflicts),
                    )
                });
            },
        );
    }
    group.finish();
}

/// Measure conflict simplification on its own and together with formatting. Environment-only
/// markers can discard the conflict guards, while conflict-dependent markers must retain them.
/// Build the inputs before timing and copy the original marker on every iteration.
fn imbibe(criterion: &mut Criterion<WallTime>) {
    let package: PackageName = "project".parse().expect("valid package name");
    let environment: MarkerTree = "python_full_version < '3.11'"
        .parse()
        .expect("valid environment marker");
    let mut inputs = vec![(
        "no_conflicts",
        0,
        UniversalMarker::from_combined(environment),
        ConflictMarker::TRUE,
    )];
    for count in [4, 16, 32] {
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
        inputs.push((
            "environment_only",
            count,
            UniversalMarker::new(environment, conflicts),
            conflicts,
        ));
        inputs.push((
            "conflict_dependent",
            count,
            UniversalMarker::new(environment, conflicts.and(selected)),
            conflicts,
        ));
    }

    let mut group = criterion.benchmark_group("marker/imbibe");
    for &(name, count, marker, conflicts) in &inputs {
        group.bench_function(
            BenchmarkId::new(format!("imbibe_marker_{name}"), count),
            |benchmark| {
                benchmark.iter(|| {
                    let mut marker = black_box(marker);
                    marker.imbibe(black_box(conflicts));
                    black_box(marker)
                });
            },
        );
    }
    group.finish();

    let mut group = criterion.benchmark_group("marker/imbibe_and_format");
    for &(name, count, marker, conflicts) in &inputs {
        group.bench_function(
            BenchmarkId::new(format!("imbibe_and_format_marker_{name}"), count),
            |benchmark| {
                benchmark.iter(|| {
                    let mut marker = black_box(marker);
                    marker.imbibe(black_box(conflicts));
                    black_box(marker.combined().try_to_string())
                });
            },
        );
    }
    group.finish();
}

criterion_group!(uv_pep508, to_dnf, resolve_extras, imbibe);
criterion_main!(uv_pep508);

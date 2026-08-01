// Keep the production allocator active during lockfile benchmarks.
extern crate uv_performance_memory_allocator;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main, measurement::WallTime};
use uv_resolver::Lock;

fn parse_lockfiles(criterion: &mut Criterion<WallTime>) {
    for (project, snapshot) in [
        (
            "packse",
            include_str!("../../uv/tests/it/snapshots/it__ecosystem__packse-lock-file.snap"),
        ),
        (
            "jupyterlab",
            include_str!("../../uv/tests/it/snapshots/it__ecosystem__jupyterlab-lock-file.snap"),
        ),
        (
            "transformers",
            include_str!("../../uv/tests/it/snapshots/it__ecosystem__transformers-lock-file.snap"),
        ),
    ] {
        let (_, lockfile) = snapshot
            .split_once("\n---\n")
            .expect("ecosystem lock snapshot includes frontmatter");
        criterion.bench_function(&format!("parse_lockfile_{project}"), |benchmark| {
            benchmark.iter(|| {
                Lock::from_canonical_toml(black_box(lockfile))
                    .expect("ecosystem snapshot should contain a valid canonical lockfile")
            });
        });
    }
}

criterion_group!(lockfile, parse_lockfiles);
criterion_main!(lockfile);

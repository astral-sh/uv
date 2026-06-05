use std::hint::black_box;
use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main, measurement::WallTime};
use uv_bench::synthetic_workspace::SyntheticWorkspace;
use uv_cache::Cache;
use uv_workspace::{DiscoveryOptions, Workspace, WorkspaceCache};

fn discover_synthetic_workspace_from_all_members(c: &mut Criterion<WallTime>) {
    let fixture = SyntheticWorkspace::create().expect("Failed to create synthetic workspace");
    let runtime = benchmark_runtime();
    let cache = Cache::from_path(fixture.root().join(".uv-cache"));
    benchmark_workspace_discovery(
        c,
        "discover_synthetic_workspace_from_all_members",
        &runtime,
        fixture.discovery_roots(),
        &cache,
    );
}

fn benchmark_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime")
}

fn benchmark_workspace_discovery(
    c: &mut Criterion<WallTime>,
    benchmark_name: &str,
    runtime: &tokio::runtime::Runtime,
    discovery_roots: &[PathBuf],
    cache: &Cache,
) {
    let options = DiscoveryOptions::default();

    c.bench_function(benchmark_name, |b| {
        b.iter(|| {
            let workspace_cache = WorkspaceCache::default();
            for root in discovery_roots {
                let workspace = runtime
                    .block_on(Workspace::discover(root, &options, cache, &workspace_cache))
                    .expect("Failed to discover benchmark workspace");
                black_box(workspace);
            }
        });
    });
}

criterion_group!(
    synthetic_workspace,
    discover_synthetic_workspace_from_all_members
);
criterion_main!(synthetic_workspace);

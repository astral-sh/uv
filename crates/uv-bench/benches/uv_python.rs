// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main, measurement::WallTime};

use uv_python_managed::downloads::ManagedPythonDownloadList;

fn load_python_download_catalog(c: &mut Criterion<WallTime>) {
    c.bench_function("load_python_download_catalog", |b| {
        b.iter(|| {
            black_box(
                ManagedPythonDownloadList::new_only_embedded()
                    .expect("Failed to load embedded Python download catalog"),
            )
        });
    });
}

criterion_group!(uv_python, load_python_download_catalog);
criterion_main!(uv_python);

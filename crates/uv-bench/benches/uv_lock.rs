// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

use std::fmt::Write;
use std::hint::black_box;
use std::path::Path;
use std::str::FromStr;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main, measurement::WallTime};

use uv_configuration::{DependencyGroupsWithDefaults, ExtrasSpecification, InstallOptions};
use uv_lock::{Installable, Lock, RequirementsTxtExport};
use uv_normalize::{DefaultExtras, PackageName};

/// Export independent extras whose optional dependencies share a dependency cycle.
fn export_shared_transitive_extras(criterion: &mut Criterion<WallTime>) {
    let extras = ExtrasSpecification::default().with_defaults(DefaultExtras::default());
    let groups = DependencyGroupsWithDefaults::none();
    let install_options = InstallOptions::default();
    let root = PackageName::from_str("root").expect("Valid package name");
    let mut group = criterion.benchmark_group("export/shared_transitive_extras");

    for size in [100, 300] {
        let lock = shared_transitive_extras_lock(size);
        let target = ExportBenchmarkTarget {
            lock: &lock,
            root: &root,
        };
        let export = || {
            RequirementsTxtExport::from_lock(
                black_box(&target),
                &[],
                &extras,
                &groups,
                false,
                None,
                false,
                &install_options,
            )
            .expect("Valid export fixture")
        };

        let expected = ["chain", "feature"]
            .into_iter()
            .flat_map(|prefix| (0..size).map(move |index| format!("{prefix}-{index:04}==1.0.0\n")))
            .collect::<String>();
        assert_eq!(export().to_string(), expected);

        group.bench_function(BenchmarkId::from_parameter(size), |benchmark| {
            benchmark.iter(|| black_box(export()));
        });
    }

    group.finish();
}

/// Generate a frozen lock whose independent extras all activate the same dependency cycle.
fn shared_transitive_extras_lock(size: usize) -> Lock {
    let mut contents = String::from(
        r#"version = 1
revision = 3
requires-python = ">=3.9"

[manifest]
members = ["root"]

[[package]]
name = "root"
version = "0.1.0"
source = { virtual = "." }
dependencies = [
"#,
    );
    for index in 0..size {
        writeln!(
            contents,
            "{{ name = \"feature-{index:04}\", extra = [\"enabled\"] }},"
        )
        .expect("Writing to a string cannot fail");
    }
    contents.push_str("]\n");

    for index in 0..size {
        writeln!(
            contents,
            r#"
[[package]]
name = "feature-{index:04}"
version = "1.0.0"
source = {{ registry = "https://example.com/simple" }}

[package.optional-dependencies]
enabled = [{{ name = "chain-0000" }}]

[package.metadata]
provides-extras = ["enabled"]

[[package]]
name = "chain-{index:04}"
version = "1.0.0"
source = {{ registry = "https://example.com/simple" }}
dependencies = [{{ name = "chain-{next:04}" }}]
"#,
            next = (index + 1) % size,
        )
        .expect("Writing to a string cannot fail");
    }

    Lock::from_toml(&contents).expect("Valid generated lockfile")
}

struct ExportBenchmarkTarget<'lock> {
    lock: &'lock Lock,
    root: &'lock PackageName,
}

impl<'lock> Installable<'lock> for ExportBenchmarkTarget<'lock> {
    fn install_path(&self) -> &'lock Path {
        Path::new(".")
    }

    fn lock(&self) -> &'lock Lock {
        self.lock
    }

    fn roots(&self) -> impl Iterator<Item = &PackageName> {
        std::iter::once(self.root)
    }

    fn project_name(&self) -> Option<&PackageName> {
        Some(self.root)
    }
}

criterion_group!(uv_lock, export_shared_transitive_extras);
criterion_main!(uv_lock);

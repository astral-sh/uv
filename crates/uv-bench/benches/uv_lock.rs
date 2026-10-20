// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

use std::fmt::Write;
use std::hint::black_box;
use std::path::Path;
use std::str::FromStr;

use criterion::{Criterion, criterion_group, criterion_main, measurement::WallTime};

use uv_configuration::{DependencyGroupsWithDefaults, ExtrasSpecification, InstallOptions};
use uv_lock::{Installable, Lock, RequirementsTxtExport};
use uv_normalize::{DefaultExtras, PackageName};

/// Many independent extras share a cycle, stressing repeated activation propagation.
fn export_shared_transitive_extras(criterion: &mut Criterion<WallTime>) {
    const SIZE: usize = 300;

    // Request every extra from the root, then connect each one to the same dependency cycle.
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
    for index in 0..SIZE {
        writeln!(
            contents,
            r#"{{ name = "feature-{index:04}", extra = ["enabled"] }},"#,
        )
        .expect("Writing to a string cannot fail");
    }
    contents.push(']');

    for index in 0..SIZE {
        writeln!(
            contents,
            r#"

[[package]]
name = "feature-{index:04}"
version = "1.0.0"
source = {{ registry = "https://example.com/simple" }}

[package.optional-dependencies]
enabled = [{{ name = "chain-0000" }}]

[[package]]
name = "chain-{index:04}"
version = "1.0.0"
source = {{ registry = "https://example.com/simple" }}
dependencies = [{{ name = "chain-{next:04}" }}]
"#,
            next = (index + 1) % SIZE,
        )
        .expect("Writing to a string cannot fail");
    }

    // Parse the fixture before timing graph construction and export reachability.
    let lock = Lock::from_toml(&contents).expect("Valid generated lockfile");
    let root = PackageName::from_str("root").expect("Valid package name");
    let target = ExportBenchmarkTarget {
        lock: &lock,
        root: &root,
    };
    let extras = ExtrasSpecification::default().with_defaults(DefaultExtras::default());
    let groups = DependencyGroupsWithDefaults::none();
    let install_options = InstallOptions::default();
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

    // All feature and cycle packages are unconditionally reachable.
    let mut expected = String::new();
    for prefix in ["chain", "feature"] {
        for index in 0..SIZE {
            writeln!(expected, "{prefix}-{index:04}==1.0.0")
                .expect("Writing to a string cannot fail");
        }
    }
    assert_eq!(export().to_string(), expected);

    criterion.bench_function("export/shared_transitive_extras/300", |benchmark| {
        benchmark.iter(|| black_box(export()));
    });
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

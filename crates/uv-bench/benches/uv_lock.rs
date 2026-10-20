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
///
/// Explicit markers consume extras either locally or after traversing the cycle.
fn export_shared_transitive_extras(criterion: &mut Criterion<WallTime>) {
    let extras = ExtrasSpecification::default().with_defaults(DefaultExtras::default());
    let groups = DependencyGroupsWithDefaults::none();
    let install_options = InstallOptions::default();
    let root = PackageName::from_str("root").expect("Valid package name");
    for (marker_variables, local_markers) in [(0, false), (0, true), (1, false), (32, false)] {
        let name = if local_markers {
            "export/local_extra_markers".to_owned()
        } else if marker_variables == 0 {
            "export/shared_transitive_extras".to_owned()
        } else {
            format!("export/explicit_transitive_extras/{marker_variables}")
        };
        let mut group = criterion.benchmark_group(name);

        for size in [100, 300] {
            let lock = shared_transitive_extras_lock(size, marker_variables, local_markers);
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

            let mut expected = String::new();
            for prefix in ["chain", "feature"] {
                for index in 0..size {
                    write!(expected, "{prefix}-{index:04}==1.0.0")
                        .expect("Writing to a string cannot fail");
                    if prefix == "feature" && index < marker_variables {
                        write!(expected, " ; {}", feature_marker(index))
                            .expect("Writing to a string cannot fail");
                    }
                    expected.push('\n');
                }
            }
            if marker_variables > 0 {
                expected.push_str(
                    "marker-negative==1.0.0 ; sys_platform != 'linux'\n\
                     marker-positive==1.0.0 ; sys_platform == 'linux'\n",
                );
            }
            assert_eq!(export().to_string(), expected);

            group.bench_function(BenchmarkId::from_parameter(size), |benchmark| {
                benchmark.iter(|| black_box(export()));
            });
        }

        group.finish();
    }
}

/// Give referenced extras overlapping but distinct activation conditions.
fn feature_marker(index: usize) -> &'static str {
    if index.is_multiple_of(2) {
        "sys_platform == 'linux'"
    } else {
        "sys_platform == 'darwin' or sys_platform == 'linux'"
    }
}

/// Generate a frozen lock whose independent extras all activate the same dependency cycle.
fn shared_transitive_extras_lock(
    size: usize,
    marker_variables: usize,
    local_markers: bool,
) -> Lock {
    // At least one unconditional extra makes the cycle reachable in every environment.
    assert!(marker_variables < size);
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
        let marker = if index < marker_variables {
            format!(", marker = \"{}\"", feature_marker(index))
        } else {
            String::new()
        };
        writeln!(
            contents,
            "{{ name = \"feature-{index:04}\", extra = [\"enabled\"]{marker} }},"
        )
        .expect("Writing to a string cannot fail");
    }
    contents.push_str("]\n");

    let marker_dependencies = if marker_variables > 0 {
        let positive = (0..marker_variables)
            .map(|index| format!("extra == 'extra-12-feature-{index:04}-enabled'"))
            .collect::<Vec<_>>()
            .join(" and ");
        format!(
            ", {{ name = \"marker-positive\", marker = \"{positive}\" }}, \
             {{ name = \"marker-negative\", marker = \"extra != 'extra-12-feature-0000-enabled'\" }}"
        )
    } else {
        String::new()
    };
    let local_marker = if local_markers {
        r#", marker = "extra == 'enabled'""#
    } else {
        ""
    };

    for index in 0..size {
        writeln!(
            contents,
            r#"
[[package]]
name = "feature-{index:04}"
version = "1.0.0"
source = {{ registry = "https://example.com/simple" }}

[package.optional-dependencies]
enabled = [{{ name = "chain-0000"{local_marker} }}]

[package.metadata]
provides-extras = ["enabled"]

[[package]]
name = "chain-{index:04}"
version = "1.0.0"
source = {{ registry = "https://example.com/simple" }}
dependencies = [{{ name = "chain-{next:04}" }}{markers}]
"#,
            next = (index + 1) % size,
            markers = if index + 1 == size {
                marker_dependencies.as_str()
            } else {
                ""
            },
        )
        .expect("Writing to a string cannot fail");
    }

    if marker_variables > 0 {
        for name in ["marker-negative", "marker-positive"] {
            writeln!(
                contents,
                r#"
[[package]]
name = "{name}"
version = "1.0.0"
source = {{ registry = "https://example.com/simple" }}
"#,
            )
            .expect("Writing to a string cannot fail");
        }
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

//! Warm, offline `uv lock` benchmarks for conflict shapes and shared-dependency activation paths.
//!
//! These use the repository's `.venv`. Packse fixture generation, interpreter discovery, cache
//! warming, lockfile removal, and output checks are outside measurement. Each invocation resolves
//! from scratch with a fresh workspace cache, but reuses the disk cache and global marker interner.

// Don't optimize the alloc crate away due to it being otherwise unused.
extern crate uv_performance_memory_allocator;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use clap::Parser;
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use tempfile::TempDir;

use uv::GlobalInitialization;
use uv::commands::ExitStatus;
use uv_cache::Cache;
use uv_cli::Cli;
use uv_python::PythonEnvironment;
use uv_resolver::Lock;
use uv_test::packse::generate_wheel;

#[derive(Clone, Copy)]
enum Workload {
    Extras(usize),
    FanIn(usize),
}

struct Fixture {
    directory: TempDir,
    packages: BTreeSet<String>,
    conflict_count: usize,
}

impl Fixture {
    fn new(workload: Workload, conflicts: &[Vec<usize>]) -> Self {
        let (extra_count, package_count, chain_length) = match workload {
            Workload::Extras(count) => (count, count, 4),
            Workload::FanIn(count) => (4, count, 8),
        };
        let directory = tempfile::tempdir().expect("Failed to create benchmark directory");
        fs_err::create_dir(directory.path().join("wheels"))
            .expect("Failed to create wheel directory");
        let mut fixture = Self {
            directory,
            packages: BTreeSet::from(["conflict-benchmark".to_string()]),
            conflict_count: conflicts.len(),
        };

        for index in 0..chain_length {
            let dependencies = if index + 1 < chain_length {
                vec![format!("shared-{}", index + 1)]
            } else {
                Vec::new()
            };
            fixture.wheel(
                &format!("shared-{index}"),
                &dependencies,
                (index == 0).then_some(("feature", &[])),
            );
        }

        let mut extras = toml::Table::new();
        for extra_index in 0..extra_count {
            let mut packages = Vec::new();
            for index in (extra_index..package_count).step_by(extra_count) {
                let name = format!("package-{index}");
                // A requested transitive extra exercises marker projection, optimized in #19538.
                let dependencies = ["shared-0[feature]".to_string()];
                let requirement = match workload {
                    Workload::Extras(_) => {
                        fixture.wheel(&name, &dependencies, None);
                        name
                    }
                    Workload::FanIn(_) => {
                        // Both the base and the extra depend on the shared chain. This creates
                        // distinct activation paths even though these extras cannot affect the
                        // root's conflicts, exercising the filtering added in #21399.
                        fixture.wheel(&name, &dependencies, Some(("feature", &dependencies)));
                        format!("{name}[feature]")
                    }
                };
                packages.push(requirement);
            }
            extras.insert(format!("extra-{extra_index}"), packages.into());
        }
        // Deliberately keep the same versions and dependencies across conflict shapes, including
        // the no-conflict control: only the declarations should change resolution's workload.
        let conflicts = conflicts
            .iter()
            .map(|set| {
                toml::Value::Array(
                    set.iter()
                        .map(|index| toml::toml! { extra = (format!("extra-{index}")) }.into())
                        .collect(),
                )
            })
            .collect::<Vec<_>>();
        let pyproject = toml::toml! {
            [project]
            name = "conflict-benchmark"
            version = "1.0.0"
            requires-python = ">=3.11"
            optional-dependencies = (extras)

            [tool.uv]
            package = false
            conflicts = (conflicts)
        };
        fs_err::write(
            fixture.root().join("pyproject.toml"),
            toml::to_string(&pyproject).expect("Failed to serialize benchmark project"),
        )
        .expect("Failed to write benchmark project");
        fixture
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn wheel(&mut self, name: &str, dependencies: &[String], extra: Option<(&str, &[String])>) {
        let name = name.parse().expect("Invalid fixture package name");
        let version = "1.0.0".parse().expect("Invalid fixture package version");
        let requires = dependencies
            .iter()
            .map(|dependency| dependency.parse().expect("Invalid fixture dependency"))
            .collect::<Vec<_>>();
        let extras = extra
            .into_iter()
            .map(|(extra, dependencies)| {
                (
                    extra.parse().expect("Invalid fixture extra"),
                    dependencies
                        .iter()
                        .map(|dependency| dependency.parse().expect("Invalid fixture dependency"))
                        .collect(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let requires_python = ">=3.11"
            .parse()
            .expect("Invalid fixture Python requirement");
        let (filename, bytes) = generate_wheel(
            &name,
            &version,
            &requires,
            &extras,
            Some(&requires_python),
            "py3-none-any",
        );
        fs_err::write(self.root().join("wheels").join(filename), bytes)
            .expect("Failed to write wheel");
        self.packages.insert(name.to_string());
    }

    fn remove_lock(&self) {
        let path = self.root().join("uv.lock");
        // CodSpeed can call setup more than once before the measured routine.
        if path.exists() {
            fs_err::remove_file(path).expect("Failed to remove benchmark lockfile");
        }
    }

    fn check_lock(&self) {
        let contents = fs_err::read_to_string(self.root().join("uv.lock"))
            .expect("Failed to read benchmark lockfile");
        let lock: Lock = toml::from_str(&contents).expect("Failed to parse benchmark lockfile");
        let packages = lock
            .packages()
            .iter()
            .map(|package| package.name().to_string())
            .collect();
        assert_eq!(
            self.packages, packages,
            "Lock must retain every extra's packages"
        );
        assert_eq!(lock.conflicts().iter().count(), self.conflict_count);
    }
}

fn conflicts(criterion: &mut Criterion) {
    let cache = Cache::temp().expect("Failed to create interpreter cache");
    let environment = PythonEnvironment::from_root("../../.venv", &cache)
        .expect("Create the repository's .venv before running benchmarks");
    let python = environment.interpreter().sys_executable();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(256)
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");
    let mut initialization = GlobalInitialization::Initialize;

    let shapes = [4, 8, 16].into_iter().flat_map(|extra_count| {
        [
            ("none", Vec::new()),
            // Keep one pair fixed as the number of root extras grows.
            ("single_pair", vec![vec![0, 1]]),
            // Cap independent pairs at four; eight pairs already create thousands of forks.
            (
                "disjoint_pairs",
                (0..extra_count.min(8))
                    .step_by(2)
                    .map(|i| vec![i, i + 1])
                    .collect(),
            ),
            // Shared endpoints exercise dominated-fork pruning (#18094).
            (
                "overlapping_pairs",
                (1..extra_count).map(|i| vec![0, i]).collect(),
            ),
            ("mutually_exclusive", vec![(0..extra_count).collect()]),
        ]
        .into_iter()
        .map(move |(shape, sets)| {
            (
                format!("lock_conflicts_{shape}_{extra_count}_extras"),
                Workload::Extras(extra_count),
                sets,
            )
        })
    });
    // Hold the root configuration fixed while growing unrelated activation paths through
    // shared dependencies. Keep a matched no-conflict control for each package count.
    let fan_in = [16, 32, 64].into_iter().flat_map(|package_count| {
        [("none", Vec::new()), ("single_pair", vec![vec![0, 1]])]
            .into_iter()
            .map(move |(shape, sets)| {
                (
                    format!("lock_conflicts_fan_in_{shape}_{package_count}_packages"),
                    Workload::FanIn(package_count),
                    sets,
                )
            })
    });
    for (name, workload, sets) in shapes.chain(fan_in) {
        let fixture = Fixture::new(workload, &sets);
        let wheels = fixture.root().join("wheels");
        let cache = fixture.root().join("cache");
        let args = [
            "uv",
            "lock",
            "--quiet",
            "--offline",
            "--no-config",
            "--no-python-downloads",
            "--no-index",
            "--no-build",
            "--find-links",
            wheels.to_str().expect("Wheel path must be UTF-8"),
            "--project",
            fixture.root().to_str().expect("Fixture path must be UTF-8"),
            "--cache-dir",
            cache.to_str().expect("Cache path must be UTF-8"),
            "--python",
            python.to_str().expect("Python path must be UTF-8"),
        ];
        let mut run = || {
            let cli = Cli::try_parse_from(args).expect("Failed to parse benchmark arguments");
            let status = runtime
                .block_on(uv::run(cli, initialization))
                .expect("Benchmark invocation failed");
            initialization = GlobalInitialization::Reuse;
            let success = match status {
                ExitStatus::Success => true,
                ExitStatus::Failure | ExitStatus::Error | ExitStatus::External(_) => false,
            };
            assert!(success, "Benchmark invocation did not succeed");
        };

        run();
        fixture.check_lock();
        criterion.bench_function(&name, |bencher| {
            bencher.iter_batched(
                || fixture.remove_lock(),
                |()| run(),
                BatchSize::PerIteration,
            );
        });
        fixture.check_lock();
    }
}

criterion_group!(benches, conflicts);
criterion_main!(benches);

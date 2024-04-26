//! Benchmarks uv using criterion.
//!
//! The benchmarks in this file assume that the `uv` and `virtualenv` executables are available.
//!
//! To set up the required environment, run:
//!
//! ```shell
//! cargo build --release
//! ./target/release/uv venv
//! source .venv/bin/activate
//! ```

use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use bench::criterion::{criterion_group, criterion_main, measurement::WallTime, Criterion};
use criterion::BatchSize;

use fs_err as fs;

const REQUIREMENTS_DIR: &str = "../../scripts/requirements";
const REQUIREMENTS: [&str; 2] = ["trio.in", "home-assistant.in"];
const COMPILED: [&str; 2] = ["compiled/trio.txt", "home-assistant.in"];

fn resolve_warm(c: &mut Criterion<WallTime>) {
    for requirements in REQUIREMENTS {
        let input = fs::canonicalize(PathBuf::from_iter([REQUIREMENTS_DIR, requirements])).unwrap();
        let name = format!(
            "resolve_warm_{}",
            input.file_stem().unwrap().to_string_lossy()
        );

        let temp_dir = tempfile::tempdir().unwrap();
        let root = env::current_dir().unwrap();
        env::set_current_dir(&temp_dir).unwrap();

        let output = "requirements.txt";

        let setup = || fs::remove_file(output).ok();

        let run = |_| {
            Command::new("uv")
                .args(["pip", "compile"])
                .arg(&input)
                .args(["--cache-dir", ".cache"])
                .args(["--output-file", output])
                .current_dir(&temp_dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
        };

        c.bench_function(&name, |b| b.iter_batched(setup, run, BatchSize::SmallInput));
        env::set_current_dir(&root).unwrap();
    }
}

fn install_warm(c: &mut Criterion<WallTime>) {
    for requirements in COMPILED {
        let input = fs::canonicalize(PathBuf::from_iter([REQUIREMENTS_DIR, requirements])).unwrap();
        let name = format!(
            "install_warm_{}",
            input.file_stem().unwrap().to_string_lossy()
        );

        let temp_dir = tempfile::tempdir().unwrap();
        let root = env::current_dir().unwrap();
        env::set_current_dir(&temp_dir).unwrap();

        let setup = || {
            Command::new("uv")
                .args(["venv", ".venv", "-p", "3.12"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
        };

        let run = |_| {
            Command::new("uv")
                .args(["pip", "sync"])
                .arg(&input)
                .args(["--cache-dir", ".cache"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
        };

        c.bench_function(&name, |b| b.iter_batched(setup, run, BatchSize::SmallInput));
        env::set_current_dir(&root).unwrap();
    }
}

criterion_group!(uv, install_warm, resolve_warm);
criterion_main!(uv);

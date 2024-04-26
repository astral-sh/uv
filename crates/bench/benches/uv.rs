use std::path::Path;
use std::process::{Command, Stdio};
use std::{env, fs};

use bench::criterion::{criterion_group, criterion_main, measurement::WallTime, Criterion};
use criterion::BatchSize;

const REQUIREMENTS: [&'static str; 4] = [
    "../../scripts/requirements/trio.in",
    "../../scripts/requirements/boto3.in",
    "../../scripts/requirements/black.in",
    "../../scripts/requirements/jupyter.in",
];

fn resolve_warm(c: &mut Criterion<WallTime>) {
    for requirements in REQUIREMENTS {
        let requirements = fs::canonicalize(requirements).unwrap();
        let name = format!(
            "resolve_warm_{}",
            requirements.file_stem().unwrap().to_string_lossy()
        );

        let temp_dir = tempfile::tempdir().unwrap();
        let output_file = Path::new("./requirements.txt");
        let cache_dir = Path::new("./.cache");

        c.bench_function(&name, |b| {
            b.iter_batched(
                || fs::remove_file(output_file).ok(),
                |_| {
                    let mut command = Command::new("uv");
                    command
                        .args(["pip", "compile"])
                        .arg(&requirements)
                        .arg("--cache-dir")
                        .arg(&cache_dir)
                        .arg("--output-file")
                        .arg(output_file)
                        .current_dir(&temp_dir)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null());

                    command.status().unwrap();
                },
                BatchSize::SmallInput,
            )
        });
    }
}

fn install_warm(c: &mut Criterion<WallTime>) {
    for requirements in REQUIREMENTS {
        let requirements = fs::canonicalize(requirements).unwrap();
        let name = format!(
            "install_warm_{}",
            requirements.file_stem().unwrap().to_string_lossy()
        );

        let venv_dir = std::fs::canonicalize("./.venv").unwrap();
        env::set_var("VIRTUAL_ENV", &venv_dir);

        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = Path::new("./.cache");

        c.bench_function(&name, |b| {
            b.iter_batched(
                || {
                    let mut command = Command::new("virtualenv");
                    command
                        .args(["--clear", "-p", "3.12"])
                        .arg(&venv_dir)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null());

                    command.status().unwrap()
                },
                |_| {
                    let mut command = Command::new("uv");
                    command
                        .args(["pip", "sync"])
                        .arg(&requirements)
                        .arg("--cache-dir")
                        .arg(&cache_dir)
                        .current_dir(&temp_dir)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null());

                    command.status().unwrap();
                },
                BatchSize::SmallInput,
            )
        });
    }
}

criterion_group!(uv, install_warm, resolve_warm);
criterion_main!(uv);

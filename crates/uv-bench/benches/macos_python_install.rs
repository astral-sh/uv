//! Benchmark fresh macOS Python installs from a cached download.
//!
//! Build uv with the same profile first:
//! `cargo build -p uv --profile profiling`
//! `cargo bench -p uv-bench --bench macos_python_install --profile profiling`

// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

#[cfg(target_os = "macos")]
use criterion::{criterion_group, criterion_main};

#[cfg(target_os = "macos")]
mod macos {
    use std::env;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use criterion::{BatchSize, Criterion, measurement::WallTime};
    use tempfile::TempDir;

    use uv_python_managed::downloads::ManagedPythonDownloadList;

    const DYLIB: &str = "lib/libpython3.13.dylib";

    struct Installer {
        uv: PathBuf,
        cache: TempDir,
        request: String,
        key: String,
    }

    impl Installer {
        fn new() -> Self {
            let benchmark = env::current_exe().expect("Failed to find benchmark executable");
            let uv = benchmark
                .parent()
                .and_then(Path::parent)
                .expect("Benchmark executable must be in target/<profile>/deps")
                .join(format!("uv{}", env::consts::EXE_SUFFIX));
            assert!(
                uv.is_file(),
                "Build the uv binary from this checkout with the benchmark's profile first: {}",
                uv.display()
            );

            let request = format!("cpython-3.13.1-macos-{}-none", env::consts::ARCH);
            let catalog = ManagedPythonDownloadList::new_only_embedded()
                .expect("Failed to load embedded Python download catalog");
            let download = catalog
                .find(&request.parse().expect("Invalid Python request"))
                .expect("Missing Python download metadata");

            Self {
                uv,
                cache: tempfile::tempdir().expect("Failed to create archive cache"),
                request,
                key: download.key().to_string(),
            }
        }

        fn install(&self, directory: &Path, offline: bool) {
            let mut command = Command::new(&self.uv);
            command
                .current_dir(directory)
                .env("UV_PYTHON_CACHE_DIR", self.cache.path())
                .env("UV_PYTHON_INSTALL_DIR", directory.join("managed"))
                .env("UV_PYTHON_BIN_DIR", directory.join("bin"))
                .env("UV_CACHE_DIR", directory.join("cache"))
                .env_remove("UV_PYTHON_DOWNLOADS_JSON_URL")
                .env_remove("UV_PYTHON_INSTALL_MIRROR")
                .env_remove("UV_ASTRAL_MIRROR_URL")
                .env_remove("UV_PREVIEW")
                .env_remove("UV_PREVIEW_FEATURES")
                .args(["--no-config", "--no-progress"]);
            if offline {
                command.arg("--offline");
            }
            let output = command
                .args(["python", "install", &self.request])
                .output()
                .expect("Failed to run uv python install");
            assert!(
                output.status.success(),
                "uv python install failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        fn verify(&self, directory: &Path) {
            let installation = directory.join("managed").join(&self.key);
            let dylib = installation.join(DYLIB);
            let output = Command::new("/usr/bin/otool")
                .arg("-D")
                .arg(&dylib)
                .output()
                .expect("Failed to inspect installed dylib");
            assert!(output.status.success(), "otool failed: {output:?}");
            let stdout = String::from_utf8(output.stdout).expect("otool output is not UTF-8");
            assert_eq!(stdout.lines().nth(1), dylib.to_str());
            assert!(directory.join("bin/python3.13").exists());
        }
    }

    pub(super) fn warm_install(criterion: &mut Criterion<WallTime>) {
        // Instruction simulation cannot measure uv or install_name_tool child processes.
        if env::var("CODSPEED_RUNNER_MODE")
            .is_ok_and(|mode| mode == "instrumentation" || mode == "simulation")
        {
            return;
        }

        let installer = Installer::new();
        let prime = tempfile::tempdir().expect("Failed to create priming directory");
        installer.install(prime.path(), false);
        drop(prime);

        // Prove a fresh installation works without the network and that patching wasn't skipped.
        let check = tempfile::tempdir().expect("Failed to create verification directory");
        installer.install(check.path(), true);
        installer.verify(check.path());
        drop(check);

        criterion.bench_function(
            &format!("python_install_warm/install_name_tool/{}", installer.key),
            |benchmark| {
                benchmark.iter_batched_ref(
                    || tempfile::tempdir().expect("Failed to create installation directory"),
                    |directory| installer.install(directory.path(), true),
                    BatchSize::PerIteration,
                );
            },
        );
    }
}

#[cfg(target_os = "macos")]
criterion_group! {
    name = macos_python_install;
    config = criterion::Criterion::default().sample_size(10);
    targets = macos::warm_install
}
#[cfg(target_os = "macos")]
criterion_main!(macos_python_install);

#[cfg(not(target_os = "macos"))]
fn main() {}

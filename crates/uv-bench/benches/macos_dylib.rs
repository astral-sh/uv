//! Benchmark managed Python dylib patching on macOS.
//!
//! `cargo bench -p uv-bench --bench macos_dylib --profile profiling`

// Don't optimize the alloc crate away due to it being otherwise unused.
// https://github.com/rust-lang/rust/issues/64402
extern crate uv_performance_memory_allocator;

#[cfg(target_os = "macos")]
use criterion::{criterion_group, criterion_main};

#[cfg(target_os = "macos")]
mod macos {
    use std::env;
    use std::hint::black_box;
    use std::process::Command;

    use criterion::{BatchSize, Criterion, measurement::WallTime};
    use tempfile::TempDir;

    use uv_client::BaseClientBuilder;
    use uv_preview::Preview;
    use uv_python_managed::ManagedPythonInstallation;
    use uv_python_managed::downloads::{
        DownloadResult, ManagedPythonDownload, ManagedPythonDownloadList,
    };
    use uv_python_types::PythonDownloadMirrors;

    const DYLIB: &str = "lib/libpython3.13.dylib";

    struct DylibFixture<'a> {
        download: &'a ManagedPythonDownload,
        bytes: Vec<u8>,
    }

    impl<'a> DylibFixture<'a> {
        fn download(download: &'a ManagedPythonDownload) -> Self {
            let directory = tempfile::tempdir().expect("Failed to create fixture directory");
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime");
            let client_builder = BaseClientBuilder::default();
            let retry_policy = client_builder.retry_policy();
            let client = client_builder
                .retries(0)
                .build()
                .expect("Failed to create download client");

            // Download and verify the archive without running installation fixups:
            // the input must retain its original install name and code signature.
            let result = runtime
                .block_on(download.fetch_with_retry(
                    &client,
                    &retry_policy,
                    directory.path(),
                    directory.path(),
                    false,
                    PythonDownloadMirrors::default(),
                    None,
                ))
                .expect("Failed to download Python dylib fixture");
            let path = match result {
                DownloadResult::AlreadyAvailable(path) | DownloadResult::Fetched(path) => path,
            };

            Self {
                download,
                bytes: fs_err::read(path.join(DYLIB)).expect("Failed to read Python dylib fixture"),
            }
        }

        fn prepare(&self) -> (TempDir, ManagedPythonInstallation) {
            let directory = tempfile::tempdir().expect("Failed to create installation directory");
            fs_err::create_dir(directory.path().join("lib"))
                .expect("Failed to create library directory");
            fs_err::write(directory.path().join(DYLIB), &self.bytes)
                .expect("Failed to write dylib");
            let installation =
                ManagedPythonInstallation::new(directory.path().to_path_buf(), self.download)
                    .expect("Failed to construct managed installation");

            (directory, installation)
        }
    }

    pub(super) fn patch_dylib(criterion: &mut Criterion<WallTime>) {
        // Instruction simulation cannot measure the install_name_tool child process.
        if env::var("CODSPEED_RUNNER_MODE")
            .is_ok_and(|mode| mode == "instrumentation" || mode == "simulation")
        {
            return;
        }

        uv_preview::set(Preview::default()).expect("Failed to configure preview features");
        let catalog = ManagedPythonDownloadList::new_only_embedded()
            .expect("Failed to load embedded Python download catalog");
        let request = format!("cpython-3.13.1-macos-{}-none", env::consts::ARCH)
            .parse()
            .expect("Invalid Python request");
        let download = catalog
            .find(&request)
            .expect("Missing Python download metadata");
        let fixture = DylibFixture::download(download);

        // Check the operation before timing it so a skipped edit cannot appear fast.
        let (directory, installation) = fixture.prepare();
        installation
            .ensure_dylib_patched()
            .expect("Failed to patch dylib");
        let dylib = installation.path().join(DYLIB);
        let output = Command::new("/usr/bin/otool")
            .arg("-D")
            .arg(&dylib)
            .output()
            .expect("Failed to inspect patched dylib");
        assert!(output.status.success(), "otool failed: {output:?}");
        let stdout = String::from_utf8(output.stdout).expect("otool output is not UTF-8");
        assert_eq!(stdout.lines().nth(1), dylib.to_str());
        drop((directory, installation));

        criterion.bench_function(
            &format!("patch_dylib/install_name_tool/{}", download.key()),
            |benchmark| {
                benchmark.iter_batched_ref(
                    || fixture.prepare(),
                    |(_, installation)| {
                        black_box(installation)
                            .ensure_dylib_patched()
                            .expect("Failed to patch dylib");
                    },
                    BatchSize::PerIteration,
                );
            },
        );
    }
}

#[cfg(target_os = "macos")]
criterion_group!(macos_dylib, macos::patch_dylib);
#[cfg(target_os = "macos")]
criterion_main!(macos_dylib);

#[cfg(not(target_os = "macos"))]
fn main() {}

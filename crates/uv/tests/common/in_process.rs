use std::ffi::{OsStr, OsString};
use std::path::Path;

use anyhow::Result;
use clap::Parser;
use tokio::runtime::Runtime;
use uv_settings::EnvironmentOptions;
use uv_test::TestContext;

/// Run the command dispatcher on a CLI-sized stack and reuse its runtime across invocations.
pub(super) fn with_runtime(test: fn(&Runtime) -> Result<()>) -> Result<()> {
    std::thread::Builder::new()
        .stack_size(uv_threads::min_stack_size())
        .spawn(move || {
            let _preview = uv_preview::test::with_features(&[]);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            test(&runtime)
        })?
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// Dispatch the selected command and format its output like a piped uv subprocess.
pub(super) fn run(
    runtime: &Runtime,
    context: &TestContext,
    directory: &Path,
    arguments: impl IntoIterator<Item = impl AsRef<OsStr>>,
    environment: EnvironmentOptions,
) -> Result<String> {
    let cli = uv_cli::Cli::try_parse_from(
        ["uv", "--no-config", "--no-python-downloads", "--cache-dir"]
            .map(OsString::from)
            .into_iter()
            .chain([context.cache_dir.as_os_str().to_owned()])
            .chain(arguments.into_iter().map(|arg| arg.as_ref().to_owned())),
    )?;
    let (status, output) = runtime.block_on(uv_command_support::test_output::capture(Box::pin(
        uv::run_in_project(cli, directory, environment),
    )));
    Ok(uv_test::format_command_output(
        Ok(i32::from(u8::from(status?))),
        &output.stdout,
        &output.stderr,
        context.filters(),
        Some(uv_test::WindowsFilters::Platform),
    ))
}

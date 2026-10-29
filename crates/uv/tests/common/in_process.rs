use std::ffi::{OsStr, OsString};
use std::path::Path;

use anyhow::Result;
use clap::{Command, CommandFactory, FromArgMatches};
use tokio::runtime::Runtime;
use uv_settings::EnvironmentOptions;
use uv_test::TestContext;

/// Run the command dispatcher on a CLI-sized stack and reuse its runtime across invocations.
pub(super) fn with_runtime(test: fn(&Runtime) -> Result<()>) -> Result<()> {
    std::thread::Builder::new()
        .stack_size(uv_threads::min_stack_size())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            test(&runtime)
        })?
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// TestContext clears the parent’s uv options when launching the executable. Use only
/// fixture arguments for options that Clap would otherwise take from the environment.
fn fixture_arguments(command: Command) -> Command {
    command
        .mut_args(|arg| arg.env(None::<&str>))
        .mut_subcommands(fixture_arguments)
}

/// Dispatch the selected command and format its output like a piped uv subprocess.
pub(super) fn run(
    runtime: &Runtime,
    context: &TestContext,
    directory: &Path,
    arguments: impl IntoIterator<Item = impl AsRef<OsStr>>,
    environment: EnvironmentOptions,
) -> Result<String> {
    let _preview = uv_preview::test::with_features(&[]);
    let mut matches = fixture_arguments(uv_cli::Cli::command()).try_get_matches_from(
        ["uv", "--no-config", "--no-python-downloads", "--cache-dir"]
            .map(OsString::from)
            .into_iter()
            .chain([context.cache_dir.as_os_str().to_owned()])
            .chain(arguments.into_iter().map(|arg| arg.as_ref().to_owned())),
    )?;
    let cli = uv_cli::Cli::from_arg_matches_mut(&mut matches)?;
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

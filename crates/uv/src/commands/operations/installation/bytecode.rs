use std::fmt::Write;
use std::path::PathBuf;

use anyhow::Context;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_command_support::{Printer, elapsed};
use uv_configuration::Concurrency;
use uv_fs::{CWD, Simplified};
use uv_installer::{compile_files, compile_tree};
use uv_python::PythonEnvironment;

/// Compile all Python source files in site-packages to bytecode, to speed up the
/// initial run of any subsequent executions.
///
/// See the `--compile` option on `pip sync` and `pip install`.
pub(super) async fn compile_bytecode(
    venv: &PythonEnvironment,
    concurrency: &Concurrency,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let mut files = 0;
    for site_packages in venv.site_packages() {
        let site_packages = CWD.join(site_packages);
        if !site_packages.exists() {
            debug!(
                "Skipping non-existent site-packages directory: {}",
                site_packages.display()
            );
            continue;
        }
        files += compile_tree(
            &site_packages,
            venv.python_executable(),
            concurrency,
            cache.root(),
        )
        .await
        .with_context(|| {
            format!(
                "Failed to bytecode-compile Python file in: {}",
                site_packages.user_display()
            )
        })?;
    }
    write_bytecode_summary(files, start, printer)?;
    Ok(())
}

/// Compile the given Python source files to bytecode.
pub(super) async fn compile_bytecode_files(
    files: impl IntoIterator<Item = anyhow::Result<PathBuf>>,
    venv: &PythonEnvironment,
    concurrency: &Concurrency,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let files = compile_files(files, venv.python_executable(), concurrency, cache.root())
        .await
        .context("Failed to bytecode-compile installed packages")?;
    if files == 0 {
        return Ok(());
    }

    write_bytecode_summary(files, start, printer)?;
    Ok(())
}

fn write_bytecode_summary(
    files: usize,
    start: std::time::Instant,
    printer: Printer,
) -> std::fmt::Result {
    let s = if files == 1 { "" } else { "s" };
    writeln!(
        printer.stderr(),
        "{}",
        format!(
            "Bytecode compiled {} {}",
            format!("{files} file{s}").bold(),
            format!("in {}", elapsed(start.elapsed())).dimmed()
        )
        .dimmed()
    )
}

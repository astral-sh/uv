use owo_colors::OwoColorize;
use std::fmt::Write;

use uv_cache::Cache;
use uv_command_support::{ExitStatus, Printer};
use uv_fs::Simplified;

/// Show the cache directory.
pub(crate) fn cache_dir(cache: &Cache, printer: Printer) -> anyhow::Result<ExitStatus> {
    writeln!(
        printer.stdout(),
        "{}",
        cache.root().simplified_display().cyan()
    )?;
    Ok(ExitStatus::Success)
}

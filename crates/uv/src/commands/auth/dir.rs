use owo_colors::OwoColorize;
use std::fmt::Write;

use uv_auth::TextCredentialStore;
use uv_command_support::Printer;
use uv_fs::Simplified;

/// Show the credentials directory.
pub(crate) fn dir(printer: Printer) -> anyhow::Result<()> {
    let root = TextCredentialStore::directory_path()?;
    writeln!(printer.stdout(), "{}", root.simplified_display().cyan())?;
    Ok(())
}

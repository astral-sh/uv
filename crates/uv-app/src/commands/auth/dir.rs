use owo_colors::OwoColorize;
use std::fmt::Write;

use uv_auth::TextCredentialStore;
use uv_fs::Simplified;

use crate::printer::Printer;

/// Show the credentials directory.
pub(crate) fn dir(printer: Printer) -> anyhow::Result<()> {
    let root = TextCredentialStore::directory_path()?;
    writeln!(printer.stdout(), "{}", root.simplified_display().cyan())?;
    Ok(())
}

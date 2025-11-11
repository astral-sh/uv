use owo_colors::OwoColorize;
use std::fmt::Write;

use uv_auth::{PyxTokenStore, Service, TextCredentialStore};
use uv_fs::Simplified;

use crate::printer::Printer;

/// Show the credentials directory.
pub(crate) fn dir(service: Option<&Service>, printer: Printer) -> anyhow::Result<()> {
    if let Some(service) = service {
        let pyx_store = PyxTokenStore::from_settings()?;
        if pyx_store.is_known_domain(service.url()) {
            writeln!(
                printer.stdout(),
                "{}",
                pyx_store.root().simplified_display().cyan()
            )?;
            return Ok(());
        }
    }

    let root = TextCredentialStore::directory_path()?;
    writeln!(printer.stdout(), "{}", root.simplified_display().cyan())?;
    Ok(())
}

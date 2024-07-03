use std::fmt::Write;

use anyhow::Result;

use uv_configuration::PreviewMode;
use uv_tool::InstalledTools;
use uv_warnings::warn_user_once;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// List installed tools.
pub(crate) async fn list(preview: PreviewMode, printer: Printer) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv tool list` is experimental and may change without warning.");
    }

    let installed_tools = InstalledTools::from_settings()?;

    let mut tools = installed_tools.tools()?.into_iter().collect::<Vec<_>>();
    tools.sort_by_key(|(name, _)| name.clone());

    if tools.is_empty() {
        writeln!(printer.stderr(), "No tools installed")?;
        return Ok(ExitStatus::Success);
    }

    // TODO(zanieb): Track and display additional metadata, like entry points
    for (name, _tool) in tools {
        writeln!(printer.stdout(), "{name}")?;
    }

    Ok(ExitStatus::Success)
}

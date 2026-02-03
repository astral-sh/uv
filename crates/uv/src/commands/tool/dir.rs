use std::fmt::Write;

use anyhow::Context;
use owo_colors::OwoColorize;

use uv_fs::Simplified;
use uv_preview::Preview;
use uv_tool::{InstalledTools, tool_executable_dir};

use crate::printer::Printer;

/// Show the tool directory.
pub(crate) fn dir(bin: bool, _preview: Preview, printer: Printer) -> anyhow::Result<()> {
    if bin {
        let executable_directory = tool_executable_dir()?;
        writeln!(
            printer.stdout(),
            "{}",
            executable_directory.simplified_display().cyan()
        )?;
    } else {
        let installed_tools =
            InstalledTools::from_settings().context("Failed to initialize tools settings")?;
        writeln!(
            printer.stdout(),
            "{}",
            installed_tools.root().simplified_display().cyan()
        )?;
    }

    Ok(())
}

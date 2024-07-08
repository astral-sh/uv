use std::fmt::Write;

use anyhow::{bail, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_tool::InstalledTools;
use uv_warnings::warn_user_once;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Uninstall a tool.
pub(crate) async fn uninstall(
    name: PackageName,
    preview: PreviewMode,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv tool uninstall` is experimental and may change without warning.");
    }

    let installed_tools = InstalledTools::from_settings()?;
    let Some(receipt) = installed_tools.get_tool_receipt(&name)? else {
        // If the tool is not installed, attempt to remove the environment anyway.
        match installed_tools.remove_environment(&name) {
            Ok(()) => {
                writeln!(
                    printer.stderr(),
                    "Removed dangling environment for `{name}`"
                )?;
                return Ok(ExitStatus::Success);
            }
            Err(uv_tool::Error::IO(err)) if err.kind() == std::io::ErrorKind::NotFound => {
                bail!("`{name}` is not installed");
            }
            Err(err) => {
                return Err(err.into());
            }
        }
    };

    // Remove the tool itself.
    installed_tools.remove_environment(&name)?;

    // Remove the tool's entrypoints.
    let entrypoints = receipt.entrypoints();
    for entrypoint in entrypoints {
        debug!(
            "Removing executable: {}",
            entrypoint.install_path.user_display()
        );
        match fs_err::tokio::remove_file(&entrypoint.install_path).await {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                debug!(
                    "Executable not found: {}",
                    entrypoint.install_path.user_display()
                );
            }
            Err(err) => {
                return Err(err.into());
            }
        }
    }

    let s = if entrypoints.len() == 1 { "" } else { "s" };
    writeln!(
        printer.stderr(),
        "Uninstalled {} executable{s}: {}",
        entrypoints.len(),
        entrypoints
            .iter()
            .map(|entrypoint| entrypoint.name.bold())
            .join(", ")
    )?;

    Ok(ExitStatus::Success)
}

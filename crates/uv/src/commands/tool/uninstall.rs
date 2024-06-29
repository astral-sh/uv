use std::fmt::Write;

use anyhow::{bail, Result};
use itertools::Itertools;

use tracing::debug;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_tool::InstalledTools;
use uv_warnings::warn_user_once;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Uninstall a tool.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn uninstall(
    name: String,
    preview: PreviewMode,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv tool uninstall` is experimental and may change without warning.");
    }

    let installed_tools = InstalledTools::from_settings()?;
    let Some(receipt) = installed_tools.get_tool_receipt(&name)? else {
        bail!("Tool `{}` is not installed", name);
    };

    // Remove the tool itself.
    installed_tools.remove_environment(&name)?;

    // Remove the tool's entrypoints.
    let entrypoints = receipt.entrypoints();
    for entrypoint in entrypoints {
        debug!(
            "Removing entrypoint: {}",
            entrypoint.install_path.user_display()
        );
        match fs_err::tokio::remove_file(&entrypoint.install_path).await {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                debug!(
                    "Entrypoint not found: {}",
                    entrypoint.install_path.user_display()
                );
            }
            Err(err) => {
                return Err(err.into());
            }
        }
    }

    writeln!(
        printer.stderr(),
        "Uninstalled: {}",
        entrypoints
            .iter()
            .map(|entrypoint| &entrypoint.name)
            .join(", ")
    )?;

    Ok(ExitStatus::Success)
}

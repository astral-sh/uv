use std::fmt::Write;

use anyhow::Result;
use axoupdater::{AxoUpdater, AxoupdateError};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_client::WrappedReqwestError;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Attempt to update the uv binary.
pub(crate) async fn self_update(printer: Printer) -> Result<ExitStatus> {
    let mut updater = AxoUpdater::new_for("uv");
    updater.disable_installer_output();

    // Load the "install receipt" for the current binary. If the receipt is not found, then
    // uv was likely installed via a package manager.
    let Ok(updater) = updater.load_receipt() else {
        debug!("no receipt found; assuming uv was installed via a package manager");
        writeln!(
            printer.stderr(),
            "{}",
            format_args!(
                concat!(
                    "{}{} Self-update is only available for uv binaries installed via the standalone installation scripts.",
                    "\n",
                    "\n",
                    "If you installed uv with pip, brew, or another package manager, update uv with `pip install --upgrade`, `brew upgrade`, or similar."
                ),
                "warning".yellow().bold(),
                ":".bold()
            )
        )?;
        return Ok(ExitStatus::Error);
    };

    // Ensure the receipt is for the current binary. If it's not, then the user likely has multiple
    // uv binaries installed, and the current binary was _not_ installed via the standalone
    // installation scripts.
    if !updater.check_receipt_is_for_this_executable()? {
        debug!(
            "receipt is not for this executable; assuming uv was installed via a package manager"
        );
        writeln!(
            printer.stderr(),
            "{}",
            format_args!(
                concat!(
                    "{}{} Self-update is only available for uv binaries installed via the standalone installation scripts.",
                    "\n",
                    "\n",
                    "If you installed uv with pip, brew, or another package manager, update uv with `pip install --upgrade`, `brew upgrade`, or similar."
                ),
                "warning".yellow().bold(),
                ":".bold()
            )
        )?;
        return Ok(ExitStatus::Error);
    }

    writeln!(
        printer.stderr(),
        "{}",
        format_args!(
            "{}{} Checking for updates...",
            "info".cyan().bold(),
            ":".bold()
        )
    )?;

    // Run the updater. This involves a network request, since we need to determine the latest
    // available version of uv.
    match updater.run().await {
        Ok(Some(result)) => {
            writeln!(
                printer.stderr(),
                "{}",
                format_args!(
                    "{}{} Upgraded uv to {}! {}",
                    "success".green().bold(),
                    ":".bold(),
                    format!("v{}", result.new_version).bold().white(),
                    format!(
                        "https://github.com/astral-sh/uv/releases/tag/{}",
                        result.new_version_tag
                    )
                    .cyan()
                )
            )?;
        }
        Ok(None) => {
            writeln!(
                printer.stderr(),
                "{}",
                format_args!(
                    "{}{} You're on the latest version of uv ({})",
                    "success".green().bold(),
                    ":".bold(),
                    format!("v{}", env!("CARGO_PKG_VERSION")).bold().white()
                )
            )?;
        }
        Err(err) => {
            return Err(if let AxoupdateError::Reqwest(err) = err {
                WrappedReqwestError::from(err).into()
            } else {
                err.into()
            });
        }
    }

    Ok(ExitStatus::Success)
}

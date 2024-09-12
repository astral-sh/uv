use std::fmt::Write;

use anyhow::Result;
use axoupdater::{AxoUpdater, AxoupdateError, UpdateRequest};
use owo_colors::OwoColorize;
use tracing::debug;

use uv_client::WrappedReqwestError;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Attempt to update the uv binary.
pub(crate) async fn self_update(
    version: Option<String>,
    token: Option<String>,
    printer: Printer,
) -> Result<ExitStatus> {
    let mut updater = AxoUpdater::new_for("uv");
    updater.disable_installer_output();

    if let Some(ref token) = token {
        updater.set_github_token(token);
    }

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

    let update_request = if let Some(version) = version {
        UpdateRequest::SpecificTag(version)
    } else {
        UpdateRequest::Latest
    };

    updater.configure_version_specifier(update_request);

    // Run the updater. This involves a network request, since we need to determine the latest
    // available version of uv.
    match updater.run().await {
        Ok(Some(result)) => {
            let version_information = if let Some(old_version) = result.old_version {
                format!(
                    "from {} to {}",
                    format!("v{old_version}").bold().white(),
                    format!("v{}", result.new_version).bold().white(),
                )
            } else {
                format!("to {}", format!("v{}", result.new_version).bold().white())
            };

            writeln!(
                printer.stderr(),
                "{}",
                format_args!(
                    "{}{} Upgraded uv {}! {}",
                    "success".green().bold(),
                    ":".bold(),
                    version_information,
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
            return if let AxoupdateError::Reqwest(err) = err {
                if err.status() == Some(http::StatusCode::FORBIDDEN) && token.is_none() {
                    writeln!(
                        printer.stderr(),
                        "{}",
                        format_args!(
                            "{}{} GitHub API rate limit exceeded. Please provide a GitHub token via the {} option.",
                            "error".red().bold(),
                            ":".bold(),
                            "`--token`".green().bold()
                        )
                    )?;
                    Ok(ExitStatus::Error)
                } else {
                    Err(WrappedReqwestError::from(err).into())
                }
            } else {
                Err(err.into())
            };
        }
    }

    Ok(ExitStatus::Success)
}

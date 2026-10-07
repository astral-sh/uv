use std::fmt::Write;

use anyhow::Result;
use owo_colors::OwoColorize;

use crate::commands::ExitStatus;
use crate::printer::Printer;
use uv_cli::VersionFormat;
use uv_cli::version::uv_self_version;

/// Display version information for uv itself (`uv self version`)
pub(crate) fn self_version(
    short: bool,
    output_format: VersionFormat,
    printer: Printer,
) -> Result<ExitStatus> {
    let version_info = uv_self_version();
    match output_format {
        VersionFormat::Text => {
            if short {
                writeln!(printer.stdout(), "{}", version_info.version().cyan())?;
            } else {
                writeln!(printer.stdout(), "uv {}", version_info.cyan())?;
            }
        }
        VersionFormat::Json => {
            let string = serde_json::to_string_pretty(&version_info)?;
            writeln!(printer.stdout(), "{string}")?;
        }
    }

    Ok(ExitStatus::Success)
}

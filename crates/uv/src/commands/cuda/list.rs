use std::fmt::Write;
use std::str::FromStr;

use anyhow::Result;
use owo_colors::OwoColorize;

use uv_cuda::{CudaRequest, find_cuda_installations};

use crate::commands::ExitStatus;
use crate::printer::Printer;

pub(crate) async fn cuda_list(
    version: Option<String>,
    only_installed: bool,
    printer: Printer,
) -> Result<ExitStatus> {
    let request = if let Some(version_str) = version {
        CudaRequest::from_str(&version_str)
            .map_err(|e| anyhow::anyhow!("Invalid CUDA version request: {}", e))?
    } else {
        CudaRequest::any()
    };

    let installations = find_cuda_installations(&request)?;

    if installations.is_empty() {
        if only_installed {
            writeln!(printer.stderr(), "No CUDA installations found")?;
        } else {
            writeln!(printer.stderr(), "No CUDA installations found")?;
            writeln!(printer.stderr(), "")?;
            writeln!(printer.stderr(), "Use {} to install CUDA.", "uv cuda install".bold())?;
        }
        return Ok(ExitStatus::Success);
    }

    // display the installations
    for (source, installation) in &installations {
        let status = if installation.is_valid() {
            "✓".green().to_string()
        } else {
            "✗".red().to_string()
        };

        writeln!(
            printer.stdout(),
            "{} {} {} ({})",
            status,
            installation.version(),
            source,
            installation.path().display()
        )?;
    }

    if !only_installed {
        writeln!(printer.stderr(), "")?;
        writeln!(printer.stderr(), "Available CUDA versions for download:")?;

        // show some popular CUDA versions that can be downloaded
        let available_versions = ["12.9", "12.8", "12.6", "12.5", "11.8"];
        for version in available_versions {
            let version_request = CudaRequest::from_str(version)?;

            // check if this version is already installed
            let already_installed = installations
                .iter()
                .any(|(_, installation)| installation.version().matches(version_request.version.as_ref().unwrap()));

            if !already_installed {
                writeln!(
                    printer.stdout(),
                    "  {} {} (available for download)",
                    "•".cyan(),
                    version
                )?;
            }
        }

        writeln!(printer.stderr(), "")?;
        writeln!(printer.stderr(), "Use {} to install a CUDA version.", "uv cuda install <version>".bold())?;
    }

    Ok(ExitStatus::Success)
}
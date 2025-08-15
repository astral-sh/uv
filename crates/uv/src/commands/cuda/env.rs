use std::fmt::Write;

use anyhow::Result;
use owo_colors::OwoColorize;

use uv_cuda::{CudaVersion, ManagedCudaInstallations};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// show the environment file path for a CUDA installation
pub(crate) async fn cuda_env(
    version_str: String,
    printer: Printer,
) -> Result<ExitStatus> {
    let version = version_str.parse::<CudaVersion>()
        .map_err(|e| anyhow::anyhow!("Invalid CUDA version '{}': {}", version_str, e))?;

    let installations = ManagedCudaInstallations::from_settings(None)?;

    match installations.find_version(&version)? {
        Some(installation) => {
            if !installation.is_valid() {
                writeln!(
                    printer.stderr(),
                    "CUDA {} installation at {} appears to be corrupted",
                    version.red(),
                    installation.path().display()
                )?;
                writeln!(printer.stderr(), "Try reinstalling with: {}",
                        format!("uv cuda install {} --force", version).bold())?;
                return Ok(ExitStatus::Failure);
            }

            let env_file_path = installation.env_file_path();

            if env_file_path.exists() {
                writeln!(printer.stdout(), "{}", env_file_path.display())?;
            } else {
                writeln!(
                    printer.stderr(),
                    "Environment file not found for CUDA {}",
                    version.yellow()
                )?;
                writeln!(printer.stderr(), "Run {} to create it.",
                        format!("uv cuda use {}", version).bold())?;
                return Ok(ExitStatus::Failure);
            }
        }
        None => {
            writeln!(
                printer.stderr(),
                "CUDA {} is not installed",
                version.yellow()
            )?;
            writeln!(printer.stderr(), "Install it with: {}",
                    format!("uv cuda install {}", version).bold())?;
            return Ok(ExitStatus::Failure);
        }
    }

    Ok(ExitStatus::Success)
}
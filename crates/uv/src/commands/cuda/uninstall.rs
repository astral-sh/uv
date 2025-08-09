use std::fmt::Write;

use anyhow::Result;
use fs_err as fs;
use owo_colors::OwoColorize;

use uv_cuda::{CudaVersion, ManagedCudaInstallations};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// uninstall CUDA versions
pub(crate) async fn cuda_uninstall(
    versions: Vec<String>,
    all: bool,
    printer: Printer,
) -> Result<ExitStatus> {
    let installations = ManagedCudaInstallations::from_settings(None)?.init()?;
    let _lock = installations.lock().await?;

    if all {
        writeln!(printer.stderr(), "Uninstalling all CUDA versions...")?;

        let all_installations: Vec<_> = installations.find_all()?.collect();
        if all_installations.is_empty() {
            writeln!(printer.stderr(), "No CUDA installations found")?;
            return Ok(ExitStatus::Success);
        }

        for installation in all_installations {
            writeln!(
                printer.stderr(),
                "Removing CUDA {} from {}",
                installation.version().cyan(),
                installation.path().display()
            )?;

            if let Err(e) = fs::remove_dir_all(installation.path()) {
                writeln!(
                    printer.stderr(),
                    "Failed to remove CUDA {}: {}",
                    installation.version().red(),
                    e
                )?;
            }
        }

        writeln!(printer.stderr(), "Successfully uninstalled all CUDA versions")?;
        return Ok(ExitStatus::Success);
    }

    if versions.is_empty() {
        writeln!(printer.stderr(), "No CUDA versions specified")?;
        writeln!(printer.stderr(), "Use {} to specify versions, or {} to uninstall all.",
                "uv cuda uninstall <version>".bold(),
                "--all".bold())?;
        return Ok(ExitStatus::Failure);
    }

    for version_str in versions {
        let version = version_str.parse::<CudaVersion>()
            .map_err(|e| anyhow::anyhow!("Invalid CUDA version '{}': {}", version_str, e))?;

        match installations.find_version(&version)? {
            Some(installation) => {
                writeln!(
                    printer.stderr(),
                    "Uninstalling CUDA {} from {}",
                    version.cyan(),
                    installation.path().display()
                )?;

                if let Err(e) = fs::remove_dir_all(installation.path()) {
                    writeln!(
                        printer.stderr(),
                        "Failed to uninstall CUDA {}: {}",
                        version.red(),
                        e
                    )?;
                    return Ok(ExitStatus::Failure);
                }

                writeln!(
                    printer.stderr(),
                    "Successfully uninstalled CUDA {}",
                    version.cyan()
                )?;
            }
            None => {
                writeln!(
                    printer.stderr(),
                    "CUDA {} is not installed",
                    version.yellow()
                )?;
            }
        }
    }

    Ok(ExitStatus::Success)
}
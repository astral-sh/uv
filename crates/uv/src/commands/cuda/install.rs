use std::fmt::Write;

use anyhow::Result;
use owo_colors::OwoColorize;

use uv_client::BaseClientBuilder;
use uv_cuda::{CudaDownloadRequest, CudaPlatformRequest, CudaVersion, ManagedCudaInstallations, SimpleProgressReporter};

use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::NetworkSettings;

pub(crate) async fn cuda_install(
    versions: Vec<String>,
    force: bool,
    network_settings: NetworkSettings,
    printer: Printer,
) -> Result<ExitStatus> {
    if versions.is_empty() {
        writeln!(printer.stderr(), "No CUDA versions specified")?;
        writeln!(printer.stderr(), "Use {} to specify a version.", "uv cuda install <version>".bold())?;
        return Ok(ExitStatus::Failure);
    }

    let installations = ManagedCudaInstallations::from_settings(None)?.init()?;
    let _lock = installations.lock().await?;

    let platform = CudaPlatformRequest::from_env()
        .map_err(|e| anyhow::anyhow!("Unsupported platform: {}", e))?;

    let client_builder = BaseClientBuilder::new()
        .connectivity(network_settings.connectivity);

    for version_str in versions {
        let version = if version_str == "latest" {
            // 12.8 is the latest version that supports Blackwell GPUs
            // TODO(alpin): Figure out if we need a different version
            CudaVersion::new(12, 8, None)
        } else {
            version_str.parse::<CudaVersion>()
                .map_err(|e| anyhow::anyhow!("Invalid CUDA version '{}': {}", version_str, e))?
        };

        writeln!(printer.stderr(), "Installing CUDA {}", version.cyan())?;

        // check if already installed and not forcing
        if let Ok(Some(existing)) = installations.find_version(&version) {
            if !force {
                writeln!(
                    printer.stderr(),
                    "CUDA {} is already installed at {}",
                    version,
                    existing.path().display()
                )?;
                writeln!(printer.stderr(), "Use {} to reinstall.", "--force".bold())?;
                continue;
            } else {
                writeln!(printer.stderr(), "Reinstalling CUDA {}", version)?;
            }
        }

        let download_request = CudaDownloadRequest::new(version.clone(), platform.clone());
        let reporter = SimpleProgressReporter::new();

        match download_request
            .install(
                &client_builder,
                installations.root(),
                &installations.scratch(),
                Some(&reporter),
            )
            .await
        {
            Ok(installation) => {
                writeln!(
                    printer.stderr(),
                    "Successfully installed CUDA {} to {}",
                    installation.version().cyan(),
                    installation.path().display()
                )?;

                writeln!(printer.stderr(), "")?;
                writeln!(printer.stderr(), "To use this CUDA installation, run:")?;
                writeln!(printer.stderr(), "  {}", format!("uv cuda use {}", version).bold())?;
            }
            Err(e) => {
                writeln!(
                    printer.stderr(),
                    "Failed to install CUDA {}: {}",
                    version.red(),
                    e
                )?;
                return Ok(ExitStatus::Failure);
            }
        }
    }

    Ok(ExitStatus::Success)
}
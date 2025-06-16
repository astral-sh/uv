use std::str::FromStr;

use tracing::debug;

use uv_pep440::Version;
use uv_static::EnvVars;

#[derive(Debug, thiserror::Error)]
pub enum AcceleratorError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Version(#[from] uv_pep440::VersionParseError),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Accelerator {
    Cuda { driver_version: Version },
}

impl std::fmt::Display for Accelerator {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Cuda { driver_version } => write!(f, "CUDA {driver_version}"),
        }
    }
}

impl Accelerator {
    /// Detect the CUDA driver version from the system.
    ///
    /// Query, in order:
    /// 1. The `UV_CUDA_DRIVER_VERSION` environment variable.
    /// 2. `/sys/module/nvidia/version`, which contains the driver version (e.g., `550.144.03`).
    /// 3. `/proc/driver/nvidia/version`, which contains the driver version among other information.
    /// 4. `nvidia-smi --query-gpu=driver_version --format=csv,noheader`.
    pub fn detect() -> Result<Option<Self>, AcceleratorError> {
        // Read from `UV_CUDA_DRIVER_VERSION`.
        if let Ok(driver_version) = std::env::var(EnvVars::UV_CUDA_DRIVER_VERSION) {
            let driver_version = Version::from_str(&driver_version)?;
            debug!("Detected CUDA driver version from `UV_CUDA_DRIVER_VERSION`: {driver_version}");
            return Ok(Some(Self::Cuda { driver_version }));
        }

        // Read from `/sys/module/nvidia/version`.
        match fs_err::read_to_string("/sys/module/nvidia/version") {
            Ok(content) => {
                return match parse_sys_module_nvidia_version(&content) {
                    Ok(driver_version) => {
                        debug!(
                            "Detected CUDA driver version from `/sys/module/nvidia/version`: {driver_version}"
                        );
                        Ok(Some(Self::Cuda { driver_version }))
                    }
                    Err(e) => Err(e),
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        // Read from `/proc/driver/nvidia/version`
        match fs_err::read_to_string("/proc/driver/nvidia/version") {
            Ok(content) => match parse_proc_driver_nvidia_version(&content) {
                Ok(Some(driver_version)) => {
                    debug!(
                        "Detected CUDA driver version from `/proc/driver/nvidia/version`: {driver_version}"
                    );
                    return Ok(Some(Self::Cuda { driver_version }));
                }
                Ok(None) => {
                    debug!(
                        "Failed to parse CUDA driver version from `/proc/driver/nvidia/version`"
                    );
                }
                Err(e) => return Err(e),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        // Query `nvidia-smi`.
        if let Ok(output) = std::process::Command::new("nvidia-smi")
            .arg("--query-gpu=driver_version")
            .arg("--format=csv,noheader")
            .output()
        {
            if output.status.success() {
                let driver_version = Version::from_str(&String::from_utf8(output.stdout)?)?;
                debug!("Detected CUDA driver version from `nvidia-smi`: {driver_version}");
                return Ok(Some(Self::Cuda { driver_version }));
            }

            debug!(
                "Failed to query CUDA driver version with `nvidia-smi` with status `{}`: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        debug!("Failed to detect CUDA driver version");
        Ok(None)
    }
}

/// Parse the CUDA driver version from the content of `/sys/module/nvidia/version`.
fn parse_sys_module_nvidia_version(content: &str) -> Result<Version, AcceleratorError> {
    // Parse, e.g.:
    // ```text
    // 550.144.03
    // ```
    let driver_version = Version::from_str(content.trim())?;
    Ok(driver_version)
}

/// Parse the CUDA driver version from the content of `/proc/driver/nvidia/version`.
fn parse_proc_driver_nvidia_version(content: &str) -> Result<Option<Version>, AcceleratorError> {
    // Parse, e.g.:
    // ```text
    // NVRM version: NVIDIA UNIX Open Kernel Module for x86_64  550.144.03  Release Build  (dvs-builder@U16-I3-D08-1-2)  Mon Dec 30 17:26:13 UTC 2024
    // GCC version:  gcc version 12.3.0 (Ubuntu 12.3.0-1ubuntu1~22.04)
    // ```
    let Some(version) = content.split("  ").nth(1) else {
        return Ok(None);
    };
    let driver_version = Version::from_str(version.trim())?;
    Ok(Some(driver_version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_driver_nvidia_version() {
        let content = "NVRM version: NVIDIA UNIX Open Kernel Module for x86_64  550.144.03  Release Build  (dvs-builder@U16-I3-D08-1-2)  Mon Dec 30 17:26:13 UTC 2024\nGCC version:  gcc version 12.3.0 (Ubuntu 12.3.0-1ubuntu1~22.04)";
        let result = parse_proc_driver_nvidia_version(content).unwrap();
        assert_eq!(result, Some(Version::from_str("550.144.03").unwrap()));

        let content = "NVRM version: NVIDIA UNIX x86_64 Kernel Module  375.74  Wed Jun 14 01:39:39 PDT 2017\nGCC version:  gcc version 5.4.0 20160609 (Ubuntu 5.4.0-6ubuntu1~16.04.4)";
        let result = parse_proc_driver_nvidia_version(content).unwrap();
        assert_eq!(result, Some(Version::from_str("375.74").unwrap()));
    }
}

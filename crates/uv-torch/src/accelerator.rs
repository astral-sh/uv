use std::path::Path;
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
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    #[error("Unknown AMD GPU architecture: {0}")]
    UnknownAmdGpuArchitecture(String),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Accelerator {
    /// The CUDA driver version (e.g., `550.144.03`).
    ///
    /// This is in contrast to the CUDA toolkit version (e.g., `12.8.0`).
    Cuda { driver_version: Version },
    /// The AMD GPU architecture (e.g., `gfx906`).
    ///
    /// This is in contrast to the user-space ROCm version (e.g., `6.4.0-47`) or the kernel-mode
    /// driver version (e.g., `6.12.12`).
    Amd {
        gpu_architecture: AmdGpuArchitecture,
    },
    /// The Intel GPU (XPU).
    ///
    /// Currently, Intel GPUs do not depend on a driver or toolkit version at this level.
    Xpu,
}

impl std::fmt::Display for Accelerator {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Cuda { driver_version } => write!(f, "CUDA {driver_version}"),
            Self::Amd { gpu_architecture } => write!(f, "AMD {gpu_architecture}"),
            Self::Xpu => write!(f, "Intel GPU (XPU)"),
        }
    }
}

impl Accelerator {
    /// Detect the GPU driver and/or architecture version from the system.
    ///
    /// Query, in order:
    /// 1. The `UV_CUDA_DRIVER_VERSION` environment variable.
    /// 2. The `UV_AMD_GPU_ARCHITECTURE` environment variable.
    /// 3. `/sys/module/nvidia/version`, which contains the driver version (e.g., `550.144.03`).
    /// 4. `/proc/driver/nvidia/version`, which contains the driver version among other information.
    /// 5. `nvidia-smi --query-gpu=driver_version --format=csv,noheader`.
    /// 6. `rocm_agent_enumerator`, which lists the AMD GPU architectures.
    /// 7. `/sys/bus/pci/devices`, filtering for the Intel GPU via PCI.
    pub fn detect() -> Result<Option<Self>, AcceleratorError> {
        // Constants used for PCI device detection.
        const PCI_BASE_CLASS_MASK: u32 = 0x00ff_0000;
        const PCI_BASE_CLASS_DISPLAY: u32 = 0x0003_0000;
        const PCI_VENDOR_ID_INTEL: u32 = 0x8086;

        // Read from `UV_CUDA_DRIVER_VERSION`.
        if let Ok(driver_version) = std::env::var(EnvVars::UV_CUDA_DRIVER_VERSION) {
            let driver_version = Version::from_str(&driver_version)?;
            debug!("Detected CUDA driver version from `UV_CUDA_DRIVER_VERSION`: {driver_version}");
            return Ok(Some(Self::Cuda { driver_version }));
        }

        // Read from `UV_AMD_GPU_ARCHITECTURE`.
        if let Ok(gpu_architecture) = std::env::var(EnvVars::UV_AMD_GPU_ARCHITECTURE) {
            let gpu_architecture = AmdGpuArchitecture::from_str(&gpu_architecture)?;
            debug!(
                "Detected AMD GPU architecture from `UV_AMD_GPU_ARCHITECTURE`: {gpu_architecture}"
            );
            return Ok(Some(Self::Amd { gpu_architecture }));
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

        // Query `rocm_agent_enumerator` to detect the AMD GPU architecture.
        //
        // See: https://rocm.docs.amd.com/projects/rocminfo/en/latest/how-to/use-rocm-agent-enumerator.html
        if let Ok(output) = std::process::Command::new("rocm_agent_enumerator").output() {
            if output.status.success() {
                let stdout = String::from_utf8(output.stdout)?;
                if let Some(gpu_architecture) = stdout
                    .lines()
                    .map(str::trim)
                    .filter_map(|line| AmdGpuArchitecture::from_str(line).ok())
                    .min()
                {
                    debug!(
                        "Detected AMD GPU architecture from `rocm_agent_enumerator`: {gpu_architecture}"
                    );
                    return Ok(Some(Self::Amd { gpu_architecture }));
                }
            } else {
                debug!(
                    "Failed to query AMD GPU architecture with `rocm_agent_enumerator` with status `{}`: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }

        // Read from `/sys/bus/pci/devices` to filter for Intel GPU via PCI.
        match fs_err::read_dir("/sys/bus/pci/devices") {
            Ok(entries) => {
                for entry in entries.flatten() {
                    match parse_pci_device_ids(&entry.path()) {
                        Ok((class, vendor)) => {
                            if (class & PCI_BASE_CLASS_MASK) == PCI_BASE_CLASS_DISPLAY
                                && vendor == PCI_VENDOR_ID_INTEL
                            {
                                debug!("Detected Intel GPU from PCI: vendor=0x{:04x}", vendor);
                                return Ok(Some(Self::Xpu));
                            }
                        }
                        Err(e) => {
                            debug!("Failed to parse PCI device IDs: {e}");
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        debug!("Failed to detect GPU driver version");

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

/// Reads and parses the PCI class and vendor ID from a given device path under `/sys/bus/pci/devices`.
fn parse_pci_device_ids(device_path: &Path) -> Result<(u32, u32), AcceleratorError> {
    // Parse, e.g.:
    // ```text
    // - `class`: a hexadecimal string such as `0x030000`
    // - `vendor`: a hexadecimal string such as `0x8086`
    // ```
    let class_content = fs_err::read_to_string(device_path.join("class"))?;
    let pci_class = u32::from_str_radix(class_content.trim().trim_start_matches("0x"), 16)?;

    let vendor_content = fs_err::read_to_string(device_path.join("vendor"))?;
    let pci_vendor = u32::from_str_radix(vendor_content.trim().trim_start_matches("0x"), 16)?;

    Ok((pci_class, pci_vendor))
}

/// A GPU architecture for AMD GPUs.
///
/// See: <https://rocm.docs.amd.com/projects/install-on-linux/en/latest/reference/system-requirements.html>
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum AmdGpuArchitecture {
    Gfx900,
    Gfx906,
    Gfx908,
    Gfx90a,
    Gfx942,
    Gfx1030,
    Gfx1100,
    Gfx1101,
    Gfx1102,
    Gfx1200,
    Gfx1201,
}

impl FromStr for AmdGpuArchitecture {
    type Err = AcceleratorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "gfx900" => Ok(Self::Gfx900),
            "gfx906" => Ok(Self::Gfx906),
            "gfx908" => Ok(Self::Gfx908),
            "gfx90a" => Ok(Self::Gfx90a),
            "gfx942" => Ok(Self::Gfx942),
            "gfx1030" => Ok(Self::Gfx1030),
            "gfx1100" => Ok(Self::Gfx1100),
            "gfx1101" => Ok(Self::Gfx1101),
            "gfx1102" => Ok(Self::Gfx1102),
            "gfx1200" => Ok(Self::Gfx1200),
            "gfx1201" => Ok(Self::Gfx1201),
            _ => Err(AcceleratorError::UnknownAmdGpuArchitecture(s.to_string())),
        }
    }
}

impl std::fmt::Display for AmdGpuArchitecture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gfx900 => write!(f, "gfx900"),
            Self::Gfx906 => write!(f, "gfx906"),
            Self::Gfx908 => write!(f, "gfx908"),
            Self::Gfx90a => write!(f, "gfx90a"),
            Self::Gfx942 => write!(f, "gfx942"),
            Self::Gfx1030 => write!(f, "gfx1030"),
            Self::Gfx1100 => write!(f, "gfx1100"),
            Self::Gfx1101 => write!(f, "gfx1101"),
            Self::Gfx1102 => write!(f, "gfx1102"),
            Self::Gfx1200 => write!(f, "gfx1200"),
            Self::Gfx1201 => write!(f, "gfx1201"),
        }
    }
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

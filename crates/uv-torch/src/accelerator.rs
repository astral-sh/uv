use std::path::Path;
use std::str::FromStr;

use tracing::debug;

use uv_pep440::Version;

#[cfg(windows)]
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_GETIDLIST_FILTER_CLASS, CM_GETIDLIST_FILTER_PRESENT, CM_Get_Device_ID_List_SizeW,
    CM_Get_Device_ID_ListW, CR_SUCCESS,
};

use crate::nvidia::NvidiaGpu;

// Constants used for PCI device detection.
const PCI_BASE_CLASS_MASK: u32 = 0x00ff_0000;
const PCI_BASE_CLASS_DISPLAY: u32 = 0x0003_0000;
const PCI_VENDOR_ID_INTEL: u32 = 0x8086;
// https://learn.microsoft.com/en-us/windows-hardware/drivers/install/system-defined-device-setup-classes-available-to-vendors
#[cfg(windows)]
const WINDOWS_DISPLAY_ADAPTER_CLASS_GUID: windows::core::PCWSTR =
    windows::core::w!("{4d36e968-e325-11ce-bfc1-08002be10318}");

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
pub(crate) enum Accelerator {
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
    /// The `cuda_driver_version` and `amd_gpu_architecture` overrides, if provided, take
    /// precedence over system detection and correspond to the `UV_CUDA_DRIVER_VERSION` and
    /// `UV_AMD_GPU_ARCHITECTURE` environment variables respectively.
    ///
    /// Query, in order:
    /// 1. The `cuda_driver_version` override (from `UV_CUDA_DRIVER_VERSION`).
    /// 2. The `amd_gpu_architecture` override (from `UV_AMD_GPU_ARCHITECTURE`).
    /// 3. [`NvidiaGpu::detect_driver_version`].
    /// 4. `rocm_agent_enumerator`, which lists the AMD GPU architectures.
    /// 5. `/sys/bus/pci/devices`, filtering for the Intel GPU via PCI.
    /// 6. The Windows device tree, filtering for present Intel display adapters via PCI.
    pub(crate) fn detect(
        cuda_driver_version: Option<Version>,
        amd_gpu_architecture: Option<AmdGpuArchitecture>,
    ) -> Result<Option<Self>, AcceleratorError> {
        // Use the `UV_CUDA_DRIVER_VERSION` override, if provided.
        if let Some(driver_version) = cuda_driver_version {
            debug!("Detected CUDA driver version from `UV_CUDA_DRIVER_VERSION`: {driver_version}");
            return Ok(Some(Self::Cuda { driver_version }));
        }

        // Use the `UV_AMD_GPU_ARCHITECTURE` override, if provided.
        if let Some(gpu_architecture) = amd_gpu_architecture {
            debug!(
                "Detected AMD GPU architecture from `UV_AMD_GPU_ARCHITECTURE`: {gpu_architecture}"
            );
            return Ok(Some(Self::Amd { gpu_architecture }));
        }

        if let Some(driver_version) = NvidiaGpu::detect_driver_version()? {
            return Ok(Some(Self::Cuda { driver_version }));
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

        // Detect Intel GPU via present Windows display adapters.
        // TODO: Consider rejecting display adapters with disabled/problem devnode status.
        // See: `CM_Locate_DevNodeW` + `CM_Get_DevNode_Status`
        #[cfg(windows)]
        if detect_intel_gpu_from_windows_devices() {
            return Ok(Some(Self::Xpu));
        }

        debug!("Failed to detect GPU driver version");

        Ok(None)
    }
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

#[cfg(windows)]
#[allow(unsafe_code)]
fn detect_intel_gpu_from_windows_devices() -> bool {
    const FLAGS: u32 = CM_GETIDLIST_FILTER_CLASS | CM_GETIDLIST_FILTER_PRESENT;

    let mut device_ids_len = 0;
    // SAFETY: The class GUID is a static null-terminated UTF-16 string, `device_ids_len` is a
    // valid out pointer, and Configuration Manager writes only that scalar result here.
    let result = unsafe {
        CM_Get_Device_ID_List_SizeW(
            &raw mut device_ids_len,
            WINDOWS_DISPLAY_ADAPTER_CLASS_GUID,
            FLAGS,
        )
    };
    if result != CR_SUCCESS {
        debug!("Failed to query Windows display adapter device list length: {result:?}");
        return false;
    }

    let Ok(device_ids_len) = usize::try_from(device_ids_len) else {
        debug!("Windows display adapter device list length does not fit in memory");
        return false;
    };
    let mut encoded_device_ids = vec![0; device_ids_len];

    // SAFETY: The class GUID is a static null-terminated UTF-16 string, and the writable buffer
    // length matches the size returned by `CM_Get_Device_ID_List_SizeW` for the same filter.
    let result = unsafe {
        CM_Get_Device_ID_ListW(
            WINDOWS_DISPLAY_ADAPTER_CLASS_GUID,
            &mut encoded_device_ids,
            FLAGS,
        )
    };
    if result != CR_SUCCESS {
        debug!("Failed to query present Windows display adapters: {result:?}");
        return false;
    }

    for device_id in windows_device_ids(&encoded_device_ids) {
        if contains_intel_vendor_id(&device_id) {
            debug!("Detected Intel GPU from present Windows display adapter: {device_id}");
            return true;
        }
    }

    false
}

#[cfg(any(windows, test))]
fn contains_intel_vendor_id(pnp_device_id: &str) -> bool {
    pnp_device_id
        .split(['\\', '&'])
        .any(|segment| segment.eq_ignore_ascii_case("VEN_8086"))
}

#[cfg(any(windows, test))]
fn windows_device_ids(encoded_device_ids: &[u16]) -> impl Iterator<Item = String> + '_ {
    encoded_device_ids
        .split(|code_unit| *code_unit == 0)
        .filter(|device_id| !device_id.is_empty())
        .filter_map(|device_id| match String::from_utf16(device_id) {
            Ok(device_id) => Some(device_id),
            Err(err) => {
                debug!("Failed to decode Windows device instance ID: {err}");
                None
            }
        })
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
    Gfx950,
    Gfx1030,
    Gfx1100,
    Gfx1101,
    Gfx1102,
    Gfx1150,
    Gfx1151,
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
            "gfx950" => Ok(Self::Gfx950),
            "gfx1030" => Ok(Self::Gfx1030),
            "gfx1100" => Ok(Self::Gfx1100),
            "gfx1101" => Ok(Self::Gfx1101),
            "gfx1102" => Ok(Self::Gfx1102),
            "gfx1150" => Ok(Self::Gfx1150),
            "gfx1151" => Ok(Self::Gfx1151),
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
            Self::Gfx950 => write!(f, "gfx950"),
            Self::Gfx1030 => write!(f, "gfx1030"),
            Self::Gfx1100 => write!(f, "gfx1100"),
            Self::Gfx1101 => write!(f, "gfx1101"),
            Self::Gfx1102 => write!(f, "gfx1102"),
            Self::Gfx1150 => write!(f, "gfx1150"),
            Self::Gfx1151 => write!(f, "gfx1151"),
            Self::Gfx1200 => write!(f, "gfx1200"),
            Self::Gfx1201 => write!(f, "gfx1201"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intel_vendor_id_from_pnp_device_id() {
        assert!(contains_intel_vendor_id(
            r"PCI\VEN_8086&DEV_9A49&SUBSYS_00000000"
        ));
        assert!(contains_intel_vendor_id(
            r"pci\ven_8086&dev_9a49&subsys_00000000"
        ));
        assert!(!contains_intel_vendor_id(
            r"PCI\VEN_10DE&DEV_2504&SUBSYS_00000000"
        ));
        assert!(!contains_intel_vendor_id(r"PCI\DEV_8086&SUBSYS_00000000"));
    }

    #[test]
    fn windows_device_instance_ids() {
        let intel_gpu = r"PCI\VEN_8086&DEV_9A49&SUBSYS_00000000\3&11583659&0&10";
        let nvidia_gpu = r"PCI\VEN_10DE&DEV_2504&SUBSYS_00000000\4&12AB34CD&0&0008";

        let mut encoded_device_ids = Vec::new();
        encoded_device_ids.extend(intel_gpu.encode_utf16());
        encoded_device_ids.push(0);
        encoded_device_ids.extend(nvidia_gpu.encode_utf16());
        encoded_device_ids.extend([0, 0]);

        assert_eq!(
            windows_device_ids(&encoded_device_ids).collect::<Vec<_>>(),
            vec![intel_gpu.to_string(), nvidia_gpu.to_string()]
        );
    }
}

use std::env::VarError;
use std::process::Command;

use tracing::debug;
use uv_pep440::Version;
use uv_static::EnvVars;

use crate::AcceleratorError;

/// Minimum driver major version that supports enumerating multiple MIG devices.
/// See <https://docs.nvidia.com/datacenter/tesla/mig-user-guide/mig-device-names.html>.
const MULTI_MIG_MIN_DRIVER_MAJOR: u64 = 570;

/// NVIDIA GPU capabilities.
#[derive(Debug)]
pub struct NvidiaGpu {
    /// The NVIDIA kernel-mode driver version (e.g., `570.86.10`).
    pub driver_version: Version,
    /// The CUDA user-mode driver version (e.g., `12.8`), as distinct from the kernel driver
    /// version used to select PyTorch indexes (e.g., `570.86.10`).
    pub cuda_version: Version,
    /// The highest compute capability among the GPUs visible to CUDA.
    pub compute_capability: Option<(u8, u8)>,
}

impl NvidiaGpu {
    /// Detect the NVIDIA driver, CUDA user-mode driver version, and GPU compute capability.
    pub fn detect() -> Result<Option<Self>, AcceleratorError> {
        let Some(driver_version) = Self::detect_driver_version()? else {
            return Ok(None);
        };
        let Some(output) = run_nvidia_smi(&["-q"])? else {
            return Ok(None);
        };
        let Some(cuda_version) = output.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            match key.trim() {
                "CUDA Version" | "CUDA UMD Version" => value.trim().parse::<Version>().ok(),
                _ => None,
            }
        }) else {
            return Ok(None);
        };
        let compute_capability = match std::env::var(EnvVars::CUDA_VISIBLE_DEVICES) {
            Ok(visible_devices) if !visible_devices.is_empty() => {
                let Some(output) = query_gpu("index,uuid,compute_cap")? else {
                    return Ok(None);
                };
                let mig_listing = if visible_devices
                    .split(',')
                    .any(|device| device.trim().starts_with("MIG-"))
                {
                    run_nvidia_smi(&["-L"])?
                } else {
                    None
                };
                let Some(gpu) = select_visible_gpu(
                    &output,
                    &visible_devices,
                    mig_listing.as_deref(),
                    &driver_version,
                ) else {
                    return Ok(None);
                };
                gpu.compute_capability
            }
            Ok(_) | Err(VarError::NotUnicode(_)) => return Ok(None),
            Err(VarError::NotPresent) => query_gpu("compute_cap")?.and_then(|output| {
                output
                    .lines()
                    .filter_map(Self::parse_compute_capability)
                    .max()
            }),
        };
        Ok(Some(Self {
            driver_version,
            cuda_version,
            compute_capability,
        }))
    }

    /// Parse a CUDA compute capability in `major.minor` form (e.g., `8.9`).
    pub fn parse_compute_capability(value: &str) -> Option<(u8, u8)> {
        let (major, minor) = value.split_once('.')?;
        Some((major.trim().parse().ok()?, minor.trim().parse().ok()?))
    }

    /// Detect the NVIDIA kernel-mode driver version (e.g., `570.86.10`).
    ///
    /// Query `/sys/module/nvidia/version`, `/proc/driver/nvidia/version`, and `nvidia-smi`, in
    /// that order.
    pub(crate) fn detect_driver_version() -> Result<Option<Version>, AcceleratorError> {
        match fs_err::read_to_string("/sys/module/nvidia/version") {
            Ok(content) => {
                let driver_version = content.trim().parse()?;
                debug!(
                    "Detected CUDA driver version from `/sys/module/nvidia/version`: {driver_version}"
                );
                return Ok(Some(driver_version));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }

        match fs_err::read_to_string("/proc/driver/nvidia/version") {
            Ok(content) => {
                if let Some(driver_version) = parse_proc_driver_nvidia_version(&content)? {
                    debug!(
                        "Detected CUDA driver version from `/proc/driver/nvidia/version`: {driver_version}"
                    );
                    return Ok(Some(driver_version));
                }
                debug!("Failed to parse CUDA driver version from `/proc/driver/nvidia/version`");
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }

        if let Some(output) = query_gpu("driver_version")?
            && let Some(driver_version) = parse_nvidia_smi_driver_version(&output)?
        {
            debug!("Detected CUDA driver version from `nvidia-smi`: {driver_version}");
            return Ok(Some(driver_version));
        }
        Ok(None)
    }
}

/// A GPU reported by `nvidia-smi --query-gpu=index,uuid,compute_cap`.
#[derive(Clone, Copy)]
struct QueriedGpu<'a> {
    index: usize,
    uuid: &'a str,
    compute_capability: Option<(u8, u8)>,
}

impl<'a> QueriedGpu<'a> {
    fn parse(line: &'a str) -> Option<Self> {
        let mut fields = line.split(',').map(str::trim);
        Some(Self {
            index: fields.next()?.parse().ok()?,
            uuid: fields.next()?,
            compute_capability: NvidiaGpu::parse_compute_capability(fields.next()?),
        })
    }
}

/// Select the highest-capability GPU visible to CUDA.
fn select_visible_gpu<'a>(
    output: &'a str,
    visible_devices: &str,
    mig_listing: Option<&str>,
    driver_version: &Version,
) -> Option<QueriedGpu<'a>> {
    let gpus = output
        .lines()
        .map(QueriedGpu::parse)
        .collect::<Option<Vec<_>>>()?;
    let mut selected: Option<QueriedGpu<'a>> = None;
    for identifier in visible_devices.split(',').map(str::trim) {
        // CUDA ignores every device after the first invalid identifier.
        let Some(gpu) = resolve_visible_gpu(&gpus, identifier, mig_listing) else {
            break;
        };
        // Earlier drivers enumerate only one MIG device and hide non-MIG GPUs.
        if identifier.starts_with("MIG-")
            && driver_version < &Version::new([MULTI_MIG_MIN_DRIVER_MAJOR])
        {
            return Some(gpu);
        }
        if selected.is_none_or(|selected| gpu.compute_capability > selected.compute_capability) {
            selected = Some(gpu);
        }
    }
    selected
}

fn resolve_visible_gpu<'a>(
    gpus: &[QueriedGpu<'a>],
    identifier: &str,
    mig_listing: Option<&str>,
) -> Option<QueriedGpu<'a>> {
    if identifier.starts_with("MIG-") {
        let uuid = mig_parent_uuid(mig_listing?, identifier)?;
        gpus.iter().copied().find(|gpu| gpu.uuid == uuid)
    } else if let Some(prefix) = identifier.strip_prefix("GPU-") {
        if prefix.is_empty() {
            return None;
        }
        let mut matches = gpus
            .iter()
            .copied()
            .filter(|gpu| gpu.uuid.starts_with(identifier));
        let gpu = matches.next()?;
        if matches.next().is_some() {
            None
        } else {
            Some(gpu)
        }
    } else {
        let index = identifier.parse::<usize>().ok()?;
        gpus.iter().copied().find(|gpu| gpu.index == index)
    }
}

/// Resolve a MIG device to its parent GPU in `nvidia-smi -L` output.
fn mig_parent_uuid<'a>(listing: &'a str, identifier: &str) -> Option<&'a str> {
    let mut parent_uuid = None;
    for line in listing.lines().map(str::trim) {
        let uuid = line
            .rsplit_once("(UUID: ")
            .and_then(|(_, uuid)| uuid.strip_suffix(')'));
        if line.starts_with("GPU ") {
            parent_uuid = uuid;
        } else if line.starts_with("MIG ") && uuid == Some(identifier) {
            return parent_uuid;
        }
    }
    None
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
    Ok(Some(version.trim().parse()?))
}

fn parse_nvidia_smi_driver_version(output: &str) -> Result<Option<Version>, AcceleratorError> {
    output
        .lines()
        .next()
        .map(|line| line.trim().parse())
        .transpose()
        .map_err(Into::into)
}

fn query_gpu(field: &str) -> Result<Option<String>, AcceleratorError> {
    run_nvidia_smi(&[&format!("--query-gpu={field}"), "--format=csv,noheader"])
}

fn run_nvidia_smi(args: &[&str]) -> Result<Option<String>, AcceleratorError> {
    let Ok(output) = Command::new("nvidia-smi").args(args).output() else {
        return Ok(None);
    };
    if output.status.success() {
        Ok(Some(String::from_utf8(output.stdout)?))
    } else {
        debug!(
            "Failed to query `nvidia-smi` with status `{}`: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use uv_pep440::Version;

    use crate::AcceleratorError;
    use crate::nvidia::{
        parse_nvidia_smi_driver_version, parse_proc_driver_nvidia_version, select_visible_gpu,
    };

    #[test]
    fn proc_driver_nvidia_version() -> Result<(), AcceleratorError> {
        let content = indoc! {"
            NVRM version: NVIDIA UNIX Open Kernel Module for x86_64  550.144.03  Release Build  \
            (dvs-builder@U16-I3-D08-1-2)  Mon Dec 30 17:26:13 UTC 2024
            GCC version:  gcc version 12.3.0 (Ubuntu 12.3.0-1ubuntu1~22.04)
        "};
        assert_eq!(
            parse_proc_driver_nvidia_version(content)?,
            Some(Version::new([550, 144, 3]))
        );

        let content = indoc! {"
            NVRM version: NVIDIA UNIX x86_64 Kernel Module  375.74  Wed Jun 14 01:39:39 PDT 2017
            GCC version:  gcc version 5.4.0 20160609 (Ubuntu 5.4.0-6ubuntu1~16.04.4)
        "};
        assert_eq!(
            parse_proc_driver_nvidia_version(content)?,
            Some(Version::new([375, 74]))
        );
        Ok(())
    }

    #[test]
    fn nvidia_smi_multi_gpu() -> Result<(), AcceleratorError> {
        assert_eq!(
            parse_nvidia_smi_driver_version("572.60\n")?,
            Some(Version::new([572, 60]))
        );
        assert_eq!(
            parse_nvidia_smi_driver_version("572.60\n572.60\n")?,
            Some(Version::new([572, 60]))
        );
        Ok(())
    }

    #[test]
    fn nvidia_smi_visible_gpus() {
        let output = indoc! {"
            0, GPU-aaaa1111-1111-1111-1111-111111111111, 8.0
            1, GPU-aaaa2222-2222-2222-2222-222222222222, 9.0
            2, GPU-bbbb3333-3333-3333-3333-333333333333, [N/A]
        "};
        let driver_version = Version::new([570]);
        assert_eq!(
            select_visible_gpu(output, "0,-1,1", None, &driver_version).map(|gpu| gpu.index),
            Some(0)
        );
        assert_eq!(
            select_visible_gpu(output, "GPU-aaaa1", None, &driver_version).map(|gpu| gpu.index),
            Some(0)
        );
        assert!(select_visible_gpu(output, "GPU-aaaa", None, &driver_version).is_none());
        assert_eq!(
            select_visible_gpu(output, "2", None, &driver_version).map(|gpu| gpu.index),
            Some(2)
        );
    }

    #[test]
    fn nvidia_smi_visible_mig_gpu() {
        let output = indoc! {"
            0, GPU-aaaa1111-1111-1111-1111-111111111111, 8.0
            1, GPU-aaaa2222-2222-2222-2222-222222222222, 9.0
        "};
        let listing = indoc! {"
            GPU 0: NVIDIA A100 (UUID: GPU-aaaa1111-1111-1111-1111-111111111111)
              MIG 1g.5gb Device 0: (UUID: MIG-cccc4444-4444-4444-4444-444444444444)
            GPU 1: NVIDIA H100 (UUID: GPU-aaaa2222-2222-2222-2222-222222222222)
        "};
        let visible_devices = "1,MIG-cccc4444-4444-4444-4444-444444444444";
        assert_eq!(
            select_visible_gpu(output, visible_devices, Some(listing), &Version::new([550]))
                .and_then(|gpu| gpu.compute_capability),
            Some((8, 0))
        );
        assert_eq!(
            select_visible_gpu(output, visible_devices, Some(listing), &Version::new([570]))
                .and_then(|gpu| gpu.compute_capability),
            Some((9, 0))
        );
    }
}

use std::process::Command;

use tracing::debug;
use uv_pep440::Version;

use crate::AcceleratorError;

#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
mod linux;

/// NVIDIA GPU detection.
pub struct NvidiaGpu;

impl NvidiaGpu {
    /// Detect the CUDA user-mode driver version independently of the kernel driver version.
    pub fn detect_cuda_version() -> Option<Version> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
        if let Some(version) = linux::cuda_version() {
            return Some(version);
        }
        run_nvidia_smi(&["-q"]).ok()??.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            match key.trim() {
                "CUDA Version" | "CUDA UMD Version" => value.trim().parse().ok(),
                _ => None,
            }
        })
    }

    /// Detect the highest compute capability among the visible GPUs.
    pub fn detect_compute_capability() -> Option<(u8, u8)> {
        let visibility = std::env::var_os("CUDA_VISIBLE_DEVICES");
        let visibility = match &visibility {
            Some(value) => Some(value.to_str()?),
            None => None,
        };
        if let Some(visibility) = visibility {
            if visibility.is_empty() {
                return None;
            }
            let output = query_gpu("uuid,pci.bus_id,compute_cap").ok()??;
            return visible_capability(
                &output,
                visibility,
                std::env::var("CUDA_DEVICE_ORDER").ok().as_deref(),
            );
        }
        #[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
        if let Some(capability) = linux::compute_capability() {
            return Some(capability);
        }
        query_gpu("compute_cap")
            .ok()??
            .lines()
            .map(Self::parse_compute_capability)
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .max()
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

/// CUDA ordinals are only predictable without CUDA's fastest-first heuristic for a single GPU or
/// when PCI bus ordering is explicitly selected. UUID prefixes must identify exactly one GPU.
fn visible_capability(output: &str, visibility: &str, order: Option<&str>) -> Option<(u8, u8)> {
    struct Device<'a> {
        uuid: &'a str,
        bus_id: &'a str,
        capability: Option<(u8, u8)>,
    }
    let mut devices = output
        .lines()
        .map(|line| {
            let mut fields = line.split(',').map(str::trim);
            let device = Device {
                uuid: fields.next()?,
                bus_id: fields.next()?,
                capability: NvidiaGpu::parse_compute_capability(fields.next()?),
            };
            fields.next().is_none().then_some(device)
        })
        .collect::<Option<Vec<_>>>()?;
    devices.sort_unstable_by_key(|device| device.bus_id);
    let mut selected = Vec::new();
    let mut maximum = None;
    for token in visibility.split(',') {
        let device = if token.starts_with("GPU-") {
            let mut matches = devices
                .iter()
                .filter(|device| device.uuid.starts_with(token));
            let device = matches.next()?;
            if matches.next().is_some() {
                return None;
            }
            device
        } else {
            let index = token.parse::<i64>().ok()?;
            let Some(device) = usize::try_from(index)
                .ok()
                .and_then(|index| devices.get(index))
            else {
                // CUDA ignores this ordinal and every device after it.
                break;
            };
            if devices.len() != 1 && order != Some("PCI_BUS_ID") {
                return None;
            }
            device
        };
        if selected.contains(&device.bus_id) {
            return None;
        }
        selected.push(device.bus_id);
        maximum = maximum.max(Some(device.capability?));
    }
    maximum
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
        parse_nvidia_smi_driver_version, parse_proc_driver_nvidia_version, visible_capability,
    };

    #[test]
    fn nvidia_smi_visibility_mask() {
        let output = "GPU-aabb, 00000000:01:00.0, 8.0\nGPU-bbcc, 00000000:02:00.0, 9.0\n";
        assert_eq!(visible_capability(output, "GPU-aa", None), Some((8, 0)));
        assert_eq!(visible_capability(output, "0", None), None);
        assert_eq!(
            visible_capability(output, "0,-1,1", Some("PCI_BUS_ID")),
            Some((8, 0))
        );
    }

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
}

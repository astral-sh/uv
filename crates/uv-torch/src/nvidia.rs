use std::process::Command;

use tracing::debug;
use uv_pep440::Version;

use crate::AcceleratorError;

/// NVIDIA GPU capabilities.
#[derive(Debug)]
pub struct NvidiaGpu {
    /// The NVIDIA kernel-mode driver version (e.g., `570.86.10`).
    pub driver_version: Version,
    /// The CUDA user-mode driver version (e.g., `12.8`), as distinct from the kernel driver
    /// version used to select PyTorch indexes (e.g., `570.86.10`).
    pub cuda_version: Version,
    /// The highest compute capability among the detected GPUs.
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
        // TODO(konsti): Filter GPUs by `CUDA_VISIBLE_DEVICES` before choosing compute capability.
        let compute_capability = query_gpu("compute_cap")?.and_then(|output| {
            output
                .lines()
                .filter_map(Self::parse_compute_capability)
                .max()
        });
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
    use crate::nvidia::{parse_nvidia_smi_driver_version, parse_proc_driver_nvidia_version};

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

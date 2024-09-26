//! Fetches CPU information.

use anyhow::Error;
use procfs::{CpuInfo, Current};

pub(crate) fn detect_hardware_floating_point_support() -> Result<bool, Error> {
    let cpu_info = CpuInfo::current()?;
    for num in 0..cpu_info.num_cores() {
        if let Some(flags) = cpu_info.flags(num) {
            if flags.contains(&"vfp") {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

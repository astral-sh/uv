//! The `aarch64::version` feature of the [WheelNext provider] on Apple Silicon.
//! The M-series profiles follow the provider's pinned [archspec] data.
//!
//! [WheelNext provider]:
//!   https://github.com/wheelnext/aarch64-variant-provider/blob/v0.2.0/aarch64_variant_provider/plugin.py
//! [archspec]:
//!   https://github.com/archspec/archspec-json/blob/90b453f4053b6b139d1d2cb78c463b02e71ef363/cpu/microarchitectures.json#L4062-L4317

use sysctl::{Ctl, Sysctl};

pub(super) const FEATURE: &str = "version";
pub(super) const BASELINE: &str = "8a";

const M1: &[&str] = &["8.4a", "8.3a", "8.2a", "8.1a", "8a"];
const M2: &[&str] = &["8.5a", "8.4a", "8.3a", "8.2a", "8.1a", "8a"];
const M3: &[&str] = &["8.6a", "8.5a", "8.4a", "8.3a", "8.2a", "8.1a", "8a"];

pub(super) fn values() -> impl Iterator<Item = String> {
    host_versions()
        .unwrap_or(&[BASELINE])
        .iter()
        .copied()
        .map(str::to_owned)
}

fn host_versions() -> Option<&'static [&'static str]> {
    let brand = Ctl::new("machdep.cpu.brand_string")
        .ok()?
        .value_string()
        .ok()?;
    let brand = brand.trim().strip_prefix("Apple ")?;
    // Early macOS versions reported "Apple processor" for the M1.
    let generation: u32 = if brand == "processor" {
        1
    } else {
        brand
            .strip_prefix('M')?
            .split_ascii_whitespace()
            .next()?
            .parse()
            .ok()?
    };
    // archspec uses the newest known profile for later M-series chips.
    match generation {
        0 => None,
        1 => Some(M1),
        2 => Some(M2),
        3.. => Some(M3),
    }
}

use serde::{Deserialize, Serialize};

use uv_platform_tags::{LibcTag, LibcVersion, PlatformTag};

/// The oldest required release of each Linux libc.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct MinimumLibcVersion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    pub glibc: Option<LibcVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schemars", schemars(with = "Option<String>"))]
    pub musl: Option<LibcVersion>,
}

impl MinimumLibcVersion {
    /// Include a wheel tag when finding the oldest available version of each libc.
    pub fn include(&mut self, tag: LibcTag) {
        let (minimum, version) = match tag {
            LibcTag::Glibc(version) => (&mut self.glibc, version),
            LibcTag::Musl(version) => (&mut self.musl, version),
        };
        *minimum = Some(minimum.map_or(version, |current| current.min(version)));
    }

    /// Combine the libc versions advertised by two sets of wheels, taking the oldest
    /// release of each family for which either set has a wheel.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            glibc: self.glibc.into_iter().chain(other.glibc).min(),
            musl: self.musl.into_iter().chain(other.musl).min(),
        }
    }

    /// Whether a platform tag covers the glibc and musl baselines, respectively.
    ///
    /// Unconfigured baselines accept any platform. Generic Linux tags do not constrain libc,
    /// matching installation behavior. Non-Linux tags are unaffected.
    pub fn platform_coverage(self, platform: &PlatformTag) -> [bool; 2] {
        match platform.linux_libc() {
            Some((_, LibcTag::Glibc(minimum))) => [
                self.glibc.is_none_or(|baseline| minimum <= baseline),
                self.musl.is_none(),
            ],
            Some((_, LibcTag::Musl(minimum))) => [
                self.glibc.is_none(),
                self.musl.is_none_or(|baseline| minimum <= baseline),
            ],
            None => [true; 2],
        }
    }
}

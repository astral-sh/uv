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
    /// Whether a platform tag covers the glibc and musl baselines, respectively.
    ///
    /// Unconfigured baselines accept any platform. Generic Linux tags do not constrain libc,
    /// matching installation behavior. Non-Linux tags are unaffected.
    pub fn platform_coverage(self, platform: &PlatformTag) -> [bool; 2] {
        match platform.libc_tag() {
            Some(LibcTag::Glibc(_, minimum)) => [
                self.glibc.is_none_or(|baseline| minimum <= baseline),
                self.musl.is_none(),
            ],
            Some(LibcTag::Musl(_, minimum)) => [
                self.glibc.is_none(),
                self.musl.is_none_or(|baseline| minimum <= baseline),
            ],
            None => [true; 2],
        }
    }
}

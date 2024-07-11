use tracing::debug;

use pep508_rs::MarkerEnvironment;
use platform_tags::{Arch, Os, Platform};

/// The supported target triples. Each triple consists of an architecture, vendor, and operating
/// system.
///
/// See: <https://doc.rust-lang.org/nightly/rustc/platform-support.html>
#[derive(Debug, Clone, Copy, Eq, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum TargetTriple {
    /// An alias for `x86_64-pc-windows-msvc`, the default target for Windows.
    Windows,

    /// An alias for `x86_64-unknown-linux-gnu`, the default target for Linux.
    Linux,

    /// An alias for `aarch64-apple-darwin`, the default target for macOS.
    Macos,

    /// An x86 Windows target.
    #[cfg_attr(feature = "clap", value(name = "x86_64-pc-windows-msvc"))]
    #[cfg_attr(feature = "schemars", schemars(rename = "x86_64-pc-windows-msvc"))]
    X8664PcWindowsMsvc,

    /// An x86 Linux target. Equivalent to `x86_64-manylinux_2_17`.
    #[cfg_attr(feature = "clap", value(name = "x86_64-unknown-linux-gnu"))]
    #[cfg_attr(feature = "schemars", schemars(rename = "x86_64-unknown-linux-gnu"))]
    X8664UnknownLinuxGnu,

    /// An ARM-based macOS target, as seen on Apple Silicon devices
    ///
    /// By default, assumes the least-recent, non-EOL macOS version (12.0), but respects
    /// the `MACOSX_DEPLOYMENT_TARGET` environment variable if set.
    #[cfg_attr(feature = "clap", value(name = "aarch64-apple-darwin"))]
    #[cfg_attr(feature = "schemars", schemars(rename = "aarch64-apple-darwin"))]
    Aarch64AppleDarwin,

    /// An x86 macOS target.
    ///
    /// By default, assumes the least-recent, non-EOL macOS version (12.0), but respects
    /// the `MACOSX_DEPLOYMENT_TARGET` environment variable if set.
    #[cfg_attr(feature = "clap", value(name = "x86_64-apple-darwin"))]
    #[cfg_attr(feature = "schemars", schemars(rename = "x86_64-apple-darwin"))]
    X8664AppleDarwin,

    /// An ARM64 Linux target. Equivalent to `aarch64-manylinux_2_17`.
    #[cfg_attr(feature = "clap", value(name = "aarch64-unknown-linux-gnu"))]
    #[cfg_attr(feature = "schemars", schemars(rename = "aarch64-unknown-linux-gnu"))]
    Aarch64UnknownLinuxGnu,

    /// An ARM64 Linux target.
    #[cfg_attr(feature = "clap", value(name = "aarch64-unknown-linux-musl"))]
    #[cfg_attr(feature = "schemars", schemars(rename = "aarch64-unknown-linux-musl"))]
    Aarch64UnknownLinuxMusl,

    /// An `x86_64` Linux target.
    #[cfg_attr(feature = "clap", value(name = "x86_64-unknown-linux-musl"))]
    #[cfg_attr(feature = "schemars", schemars(rename = "x86_64-unknown-linux-musl"))]
    X8664UnknownLinuxMusl,

    /// An `x86_64` target for the `manylinux_2_17` platform.
    #[cfg_attr(feature = "clap", value(name = "x86_64-manylinux_2_17"))]
    #[cfg_attr(feature = "schemars", schemars(rename = "x86_64-manylinux_2_17"))]
    X8664Manylinux217,

    /// An `x86_64` target for the `manylinux_2_28` platform.
    #[cfg_attr(feature = "clap", value(name = "x86_64-manylinux_2_28"))]
    #[cfg_attr(feature = "schemars", schemars(rename = "x86_64-manylinux_2_28"))]
    X8664Manylinux228,

    /// An `x86_64` target for the `manylinux_2_31` platform.
    #[cfg_attr(feature = "clap", value(name = "x86_64-manylinux_2_31"))]
    #[cfg_attr(feature = "schemars", schemars(rename = "x86_64-manylinux_2_31"))]
    X8664Manylinux231,

    /// An ARM64 target for the `manylinux_2_17` platform.
    #[cfg_attr(feature = "clap", value(name = "aarch64-manylinux_2_17"))]
    #[cfg_attr(feature = "schemars", schemars(rename = "aarch64-manylinux_2_17"))]
    Aarch64Manylinux217,

    /// An ARM64 target for the `manylinux_2_28` platform.
    #[cfg_attr(feature = "clap", value(name = "aarch64-manylinux_2_28"))]
    #[cfg_attr(feature = "schemars", schemars(rename = "aarch64-manylinux_2_28"))]
    Aarch64Manylinux228,

    /// An ARM64 target for the `manylinux_2_31` platform.
    #[cfg_attr(feature = "clap", value(name = "aarch64-manylinux_2_31"))]
    #[cfg_attr(feature = "schemars", schemars(rename = "aarch64-manylinux_2_31"))]
    Aarch64Manylinux231,
}

impl TargetTriple {
    /// Return the [`Platform`] for the target.
    pub fn platform(self) -> Platform {
        match self {
            Self::Windows | Self::X8664PcWindowsMsvc => Platform::new(Os::Windows, Arch::X86_64),
            Self::Linux | Self::X8664UnknownLinuxGnu => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 17,
                },
                Arch::X86_64,
            ),
            Self::Macos | Self::Aarch64AppleDarwin => {
                let (major, minor) = macos_deployment_target().map_or((12, 0), |(major, minor)| {
                    debug!("Found macOS deployment target: {}.{}", major, minor);
                    (major, minor)
                });
                Platform::new(Os::Macos { major, minor }, Arch::Aarch64)
            }
            Self::X8664AppleDarwin => {
                let (major, minor) = macos_deployment_target().map_or((12, 0), |(major, minor)| {
                    debug!("Found macOS deployment target: {}.{}", major, minor);
                    (major, minor)
                });
                Platform::new(Os::Macos { major, minor }, Arch::X86_64)
            }
            Self::Aarch64UnknownLinuxGnu => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 17,
                },
                Arch::Aarch64,
            ),
            Self::Aarch64UnknownLinuxMusl => {
                Platform::new(Os::Musllinux { major: 1, minor: 2 }, Arch::Aarch64)
            }
            Self::X8664UnknownLinuxMusl => {
                Platform::new(Os::Musllinux { major: 1, minor: 2 }, Arch::X86_64)
            }
            Self::X8664Manylinux217 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 17,
                },
                Arch::X86_64,
            ),
            Self::X8664Manylinux228 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 28,
                },
                Arch::X86_64,
            ),
            Self::X8664Manylinux231 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 31,
                },
                Arch::X86_64,
            ),
            Self::Aarch64Manylinux217 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 17,
                },
                Arch::Aarch64,
            ),
            Self::Aarch64Manylinux228 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 28,
                },
                Arch::Aarch64,
            ),
            Self::Aarch64Manylinux231 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 31,
                },
                Arch::Aarch64,
            ),
        }
    }

    /// Return the `platform_machine` value for the target.
    pub fn platform_machine(self) -> &'static str {
        match self {
            Self::Windows | Self::X8664PcWindowsMsvc => "x86_64",
            Self::Linux | Self::X8664UnknownLinuxGnu => "x86_64",
            Self::Macos | Self::Aarch64AppleDarwin => "arm64",
            Self::X8664AppleDarwin => "x86_64",
            Self::Aarch64UnknownLinuxGnu => "aarch64",
            Self::Aarch64UnknownLinuxMusl => "aarch64",
            Self::X8664UnknownLinuxMusl => "x86_64",
            Self::X8664Manylinux217 => "x86_64",
            Self::X8664Manylinux228 => "x86_64",
            Self::X8664Manylinux231 => "x86_64",
            Self::Aarch64Manylinux217 => "aarch64",
            Self::Aarch64Manylinux228 => "aarch64",
            Self::Aarch64Manylinux231 => "aarch64",
        }
    }

    /// Return the `platform_system` value for the target.
    pub fn platform_system(self) -> &'static str {
        match self {
            Self::Windows | Self::X8664PcWindowsMsvc => "Windows",
            Self::Linux | Self::X8664UnknownLinuxGnu => "Linux",
            Self::Macos | Self::Aarch64AppleDarwin => "Darwin",
            Self::X8664AppleDarwin => "Darwin",
            Self::Aarch64UnknownLinuxGnu => "Linux",
            Self::Aarch64UnknownLinuxMusl => "Linux",
            Self::X8664UnknownLinuxMusl => "Linux",
            Self::X8664Manylinux217 => "Linux",
            Self::X8664Manylinux228 => "Linux",
            Self::X8664Manylinux231 => "Linux",
            Self::Aarch64Manylinux217 => "Linux",
            Self::Aarch64Manylinux228 => "Linux",
            Self::Aarch64Manylinux231 => "Linux",
        }
    }

    /// Return the `platform_version` value for the target.
    pub fn platform_version(self) -> &'static str {
        match self {
            Self::Windows | Self::X8664PcWindowsMsvc => "",
            Self::Linux | Self::X8664UnknownLinuxGnu => "",
            Self::Macos | Self::Aarch64AppleDarwin => "",
            Self::X8664AppleDarwin => "",
            Self::Aarch64UnknownLinuxGnu => "",
            Self::Aarch64UnknownLinuxMusl => "",
            Self::X8664UnknownLinuxMusl => "",
            Self::X8664Manylinux217 => "",
            Self::X8664Manylinux228 => "",
            Self::X8664Manylinux231 => "",
            Self::Aarch64Manylinux217 => "",
            Self::Aarch64Manylinux228 => "",
            Self::Aarch64Manylinux231 => "",
        }
    }

    /// Return the `platform_release` value for the target.
    pub fn platform_release(self) -> &'static str {
        match self {
            Self::Windows | Self::X8664PcWindowsMsvc => "",
            Self::Linux | Self::X8664UnknownLinuxGnu => "",
            Self::Macos | Self::Aarch64AppleDarwin => "",
            Self::X8664AppleDarwin => "",
            Self::Aarch64UnknownLinuxGnu => "",
            Self::Aarch64UnknownLinuxMusl => "",
            Self::X8664UnknownLinuxMusl => "",
            Self::X8664Manylinux217 => "",
            Self::X8664Manylinux228 => "",
            Self::X8664Manylinux231 => "",
            Self::Aarch64Manylinux217 => "",
            Self::Aarch64Manylinux228 => "",
            Self::Aarch64Manylinux231 => "",
        }
    }

    /// Return the `os_name` value for the target.
    pub fn os_name(self) -> &'static str {
        match self {
            Self::Windows | Self::X8664PcWindowsMsvc => "nt",
            Self::Linux | Self::X8664UnknownLinuxGnu => "posix",
            Self::Macos | Self::Aarch64AppleDarwin => "posix",
            Self::X8664AppleDarwin => "posix",
            Self::Aarch64UnknownLinuxGnu => "posix",
            Self::Aarch64UnknownLinuxMusl => "posix",
            Self::X8664UnknownLinuxMusl => "posix",
            Self::X8664Manylinux217 => "posix",
            Self::X8664Manylinux228 => "posix",
            Self::X8664Manylinux231 => "posix",
            Self::Aarch64Manylinux217 => "posix",
            Self::Aarch64Manylinux228 => "posix",
            Self::Aarch64Manylinux231 => "posix",
        }
    }

    /// Return the `sys_platform` value for the target.
    pub fn sys_platform(self) -> &'static str {
        match self {
            Self::Windows | Self::X8664PcWindowsMsvc => "win32",
            Self::Linux | Self::X8664UnknownLinuxGnu => "linux",
            Self::Macos | Self::Aarch64AppleDarwin => "darwin",
            Self::X8664AppleDarwin => "darwin",
            Self::Aarch64UnknownLinuxGnu => "linux",
            Self::Aarch64UnknownLinuxMusl => "linux",
            Self::X8664UnknownLinuxMusl => "linux",
            Self::X8664Manylinux217 => "linux",
            Self::X8664Manylinux228 => "linux",
            Self::X8664Manylinux231 => "linux",
            Self::Aarch64Manylinux217 => "linux",
            Self::Aarch64Manylinux228 => "linux",
            Self::Aarch64Manylinux231 => "linux",
        }
    }

    /// Return a [`MarkerEnvironment`] compatible with the given [`TargetTriple`], based on
    /// a base [`MarkerEnvironment`].
    ///
    /// The returned [`MarkerEnvironment`] will preserve the base environment's Python version
    /// markers, but override its platform markers.
    pub fn markers(self, base: &MarkerEnvironment) -> MarkerEnvironment {
        base.clone()
            .with_os_name(self.os_name())
            .with_platform_machine(self.platform_machine())
            .with_platform_system(self.platform_system())
            .with_sys_platform(self.sys_platform())
            .with_platform_release(self.platform_release())
            .with_platform_version(self.platform_version())
    }
}

/// Return the macOS deployment target as parsed from the environment.
fn macos_deployment_target() -> Option<(u16, u16)> {
    let version = std::env::var("MACOSX_DEPLOYMENT_TARGET").ok()?;
    let mut parts = version.split('.');

    // Parse the major version (e.g., `12` in `12.0`).
    let major = parts.next()?.parse::<u16>().ok()?;

    // Parse the minor version (e.g., `0` in `12.0`), with a default of `0`.
    let minor = parts.next().unwrap_or("0").parse::<u16>().ok()?;

    Some((major, minor))
}

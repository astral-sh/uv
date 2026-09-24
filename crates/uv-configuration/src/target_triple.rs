use tracing::debug;

use uv_pep508::MarkerEnvironment;
use uv_platform_tags::{Arch, Os, Platform};
use uv_static::EnvVars;

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

    /// A 64-bit x86 Windows target.
    #[cfg_attr(feature = "clap", value(name = "x86_64-pc-windows-msvc"))]
    #[serde(rename = "x86_64-pc-windows-msvc")]
    #[serde(alias = "x8664-pc-windows-msvc")]
    X8664PcWindowsMsvc,

    /// An ARM64 Windows target.
    #[cfg_attr(feature = "clap", value(name = "aarch64-pc-windows-msvc"))]
    #[serde(rename = "aarch64-pc-windows-msvc")]
    #[serde(alias = "arm64-pc-windows-msvc")]
    Aarch64PcWindowsMsvc,

    /// A 32-bit x86 Windows target.
    #[cfg_attr(feature = "clap", value(name = "i686-pc-windows-msvc"))]
    #[serde(rename = "i686-pc-windows-msvc")]
    I686PcWindowsMsvc,

    /// An x86 Linux target. Equivalent to `x86_64-manylinux_2_28`.
    #[cfg_attr(feature = "clap", value(name = "x86_64-unknown-linux-gnu"))]
    #[serde(rename = "x86_64-unknown-linux-gnu")]
    #[serde(alias = "x8664-unknown-linux-gnu")]
    X8664UnknownLinuxGnu,

    /// An ARM-based macOS target, as seen on Apple Silicon devices
    ///
    /// By default, assumes the least-recent, non-EOL macOS version (13.0), but respects
    /// the `MACOSX_DEPLOYMENT_TARGET` environment variable if set.
    #[cfg_attr(feature = "clap", value(name = "aarch64-apple-darwin"))]
    #[serde(rename = "aarch64-apple-darwin")]
    Aarch64AppleDarwin,

    /// An x86 macOS target.
    ///
    /// By default, assumes the least-recent, non-EOL macOS version (13.0), but respects
    /// the `MACOSX_DEPLOYMENT_TARGET` environment variable if set.
    #[cfg_attr(feature = "clap", value(name = "x86_64-apple-darwin"))]
    #[serde(rename = "x86_64-apple-darwin")]
    #[serde(alias = "x8664-apple-darwin")]
    X8664AppleDarwin,

    /// An ARM64 Linux target. Equivalent to `aarch64-manylinux_2_28`.
    #[cfg_attr(feature = "clap", value(name = "aarch64-unknown-linux-gnu"))]
    #[serde(rename = "aarch64-unknown-linux-gnu")]
    Aarch64UnknownLinuxGnu,

    /// An ARM64 Linux target.
    #[cfg_attr(feature = "clap", value(name = "aarch64-unknown-linux-musl"))]
    #[serde(rename = "aarch64-unknown-linux-musl")]
    Aarch64UnknownLinuxMusl,

    /// An `x86_64` Linux target.
    #[cfg_attr(feature = "clap", value(name = "x86_64-unknown-linux-musl"))]
    #[serde(rename = "x86_64-unknown-linux-musl")]
    #[serde(alias = "x8664-unknown-linux-musl")]
    X8664UnknownLinuxMusl,

    /// An s390x Linux target. Equivalent to `s390x-manylinux_2_28`.
    #[cfg_attr(feature = "clap", value(name = "s390x-unknown-linux-gnu"))]
    #[serde(rename = "s390x-unknown-linux-gnu")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    S390XUnknownLinuxGnu,

    /// A little-endian `PowerPC64` Linux target. Equivalent to `ppc64le-manylinux_2_28`.
    #[cfg_attr(feature = "clap", value(name = "powerpc64le-unknown-linux-gnu"))]
    #[serde(rename = "powerpc64le-unknown-linux-gnu")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    Powerpc64LeUnknownLinuxGnu,

    /// A `LoongArch64` Linux target. Equivalent to `loongarch64-manylinux_2_36`.
    #[cfg_attr(feature = "clap", value(name = "loongarch64-unknown-linux-gnu"))]
    #[serde(rename = "loongarch64-unknown-linux-gnu")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    LoongArch64UnknownLinuxGnu,

    /// A RISCV64 Linux target.
    #[cfg_attr(feature = "clap", value(name = "riscv64-unknown-linux"))]
    #[serde(rename = "riscv64-unknown-linux")]
    Riscv64UnknownLinuxGnu,

    /// An `x86_64` target for the `manylinux2014` platform. Equivalent to `x86_64-manylinux_2_17`.
    #[cfg_attr(
        feature = "clap",
        value(name = "x86_64-manylinux2014", alias = "manylinux2014_x86_64")
    )]
    #[serde(rename = "x86_64-manylinux2014")]
    #[serde(alias = "x8664-manylinux2014")]
    #[serde(alias = "manylinux2014_x86_64")]
    X8664Manylinux2014,

    /// An `x86_64` target for the `manylinux_2_17` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "x86_64-manylinux_2_17", alias = "manylinux_2_17_x86_64")
    )]
    #[serde(rename = "x86_64-manylinux_2_17")]
    #[serde(alias = "x8664-manylinux217")]
    #[serde(alias = "manylinux_2_17_x86_64")]
    X8664Manylinux217,

    /// An `x86_64` target for the `manylinux_2_28` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "x86_64-manylinux_2_28", alias = "manylinux_2_28_x86_64")
    )]
    #[serde(rename = "x86_64-manylinux_2_28")]
    #[serde(alias = "x8664-manylinux228")]
    #[serde(alias = "manylinux_2_28_x86_64")]
    X8664Manylinux228,

    /// An `x86_64` target for the `manylinux_2_31` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "x86_64-manylinux_2_31", alias = "manylinux_2_31_x86_64")
    )]
    #[serde(rename = "x86_64-manylinux_2_31")]
    #[serde(alias = "x8664-manylinux231")]
    #[serde(alias = "manylinux_2_31_x86_64")]
    X8664Manylinux231,

    /// An `x86_64` target for the `manylinux_2_32` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "x86_64-manylinux_2_32", alias = "manylinux_2_32_x86_64")
    )]
    #[serde(rename = "x86_64-manylinux_2_32")]
    #[serde(alias = "x8664-manylinux232")]
    #[serde(alias = "manylinux_2_32_x86_64")]
    X8664Manylinux232,

    /// An `x86_64` target for the `manylinux_2_33` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "x86_64-manylinux_2_33", alias = "manylinux_2_33_x86_64")
    )]
    #[serde(rename = "x86_64-manylinux_2_33")]
    #[serde(alias = "x8664-manylinux233")]
    #[serde(alias = "manylinux_2_33_x86_64")]
    X8664Manylinux233,

    /// An `x86_64` target for the `manylinux_2_34` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "x86_64-manylinux_2_34", alias = "manylinux_2_34_x86_64")
    )]
    #[serde(rename = "x86_64-manylinux_2_34")]
    #[serde(alias = "x8664-manylinux234")]
    #[serde(alias = "manylinux_2_34_x86_64")]
    X8664Manylinux234,

    /// An `x86_64` target for the `manylinux_2_35` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "x86_64-manylinux_2_35", alias = "manylinux_2_35_x86_64")
    )]
    #[serde(rename = "x86_64-manylinux_2_35")]
    #[serde(alias = "x8664-manylinux235")]
    #[serde(alias = "manylinux_2_35_x86_64")]
    X8664Manylinux235,

    /// An `x86_64` target for the `manylinux_2_36` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "x86_64-manylinux_2_36", alias = "manylinux_2_36_x86_64")
    )]
    #[serde(rename = "x86_64-manylinux_2_36")]
    #[serde(alias = "x8664-manylinux236")]
    #[serde(alias = "manylinux_2_36_x86_64")]
    X8664Manylinux236,

    /// An `x86_64` target for the `manylinux_2_37` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "x86_64-manylinux_2_37", alias = "manylinux_2_37_x86_64")
    )]
    #[serde(rename = "x86_64-manylinux_2_37")]
    #[serde(alias = "x8664-manylinux237")]
    #[serde(alias = "manylinux_2_37_x86_64")]
    X8664Manylinux237,

    /// An `x86_64` target for the `manylinux_2_38` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "x86_64-manylinux_2_38", alias = "manylinux_2_38_x86_64")
    )]
    #[serde(rename = "x86_64-manylinux_2_38")]
    #[serde(alias = "x8664-manylinux238")]
    #[serde(alias = "manylinux_2_38_x86_64")]
    X8664Manylinux238,

    /// An `x86_64` target for the `manylinux_2_39` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "x86_64-manylinux_2_39", alias = "manylinux_2_39_x86_64")
    )]
    #[serde(rename = "x86_64-manylinux_2_39")]
    #[serde(alias = "x8664-manylinux239")]
    #[serde(alias = "manylinux_2_39_x86_64")]
    X8664Manylinux239,

    /// An `x86_64` target for the `manylinux_2_40` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "x86_64-manylinux_2_40", alias = "manylinux_2_40_x86_64")
    )]
    #[serde(rename = "x86_64-manylinux_2_40")]
    #[serde(alias = "x8664-manylinux240")]
    #[serde(alias = "manylinux_2_40_x86_64")]
    X8664Manylinux240,

    /// An ARM64 target for the `manylinux2014` platform. Equivalent to `aarch64-manylinux_2_17`.
    #[cfg_attr(
        feature = "clap",
        value(name = "aarch64-manylinux2014", alias = "manylinux2014_aarch64")
    )]
    #[serde(rename = "aarch64-manylinux2014")]
    #[serde(alias = "manylinux2014_aarch64")]
    Aarch64Manylinux2014,

    /// An ARM64 target for the `manylinux_2_17` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "aarch64-manylinux_2_17", alias = "manylinux_2_17_aarch64")
    )]
    #[serde(rename = "aarch64-manylinux_2_17")]
    #[serde(alias = "aarch64-manylinux217")]
    #[serde(alias = "manylinux_2_17_aarch64")]
    Aarch64Manylinux217,

    /// An ARM64 target for the `manylinux_2_28` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "aarch64-manylinux_2_28", alias = "manylinux_2_28_aarch64")
    )]
    #[serde(rename = "aarch64-manylinux_2_28")]
    #[serde(alias = "aarch64-manylinux228")]
    #[serde(alias = "manylinux_2_28_aarch64")]
    Aarch64Manylinux228,

    /// An ARM64 target for the `manylinux_2_31` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "aarch64-manylinux_2_31", alias = "manylinux_2_31_aarch64")
    )]
    #[serde(rename = "aarch64-manylinux_2_31")]
    #[serde(alias = "aarch64-manylinux231")]
    #[serde(alias = "manylinux_2_31_aarch64")]
    Aarch64Manylinux231,

    /// An ARM64 target for the `manylinux_2_32` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "aarch64-manylinux_2_32", alias = "manylinux_2_32_aarch64")
    )]
    #[serde(rename = "aarch64-manylinux_2_32")]
    #[serde(alias = "aarch64-manylinux232")]
    #[serde(alias = "manylinux_2_32_aarch64")]
    Aarch64Manylinux232,

    /// An ARM64 target for the `manylinux_2_33` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "aarch64-manylinux_2_33", alias = "manylinux_2_33_aarch64")
    )]
    #[serde(rename = "aarch64-manylinux_2_33")]
    #[serde(alias = "aarch64-manylinux233")]
    #[serde(alias = "manylinux_2_33_aarch64")]
    Aarch64Manylinux233,

    /// An ARM64 target for the `manylinux_2_34` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "aarch64-manylinux_2_34", alias = "manylinux_2_34_aarch64")
    )]
    #[serde(rename = "aarch64-manylinux_2_34")]
    #[serde(alias = "aarch64-manylinux234")]
    #[serde(alias = "manylinux_2_34_aarch64")]
    Aarch64Manylinux234,

    /// An ARM64 target for the `manylinux_2_35` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "aarch64-manylinux_2_35", alias = "manylinux_2_35_aarch64")
    )]
    #[serde(rename = "aarch64-manylinux_2_35")]
    #[serde(alias = "aarch64-manylinux235")]
    #[serde(alias = "manylinux_2_35_aarch64")]
    Aarch64Manylinux235,

    /// An ARM64 target for the `manylinux_2_36` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "aarch64-manylinux_2_36", alias = "manylinux_2_36_aarch64")
    )]
    #[serde(rename = "aarch64-manylinux_2_36")]
    #[serde(alias = "aarch64-manylinux236")]
    #[serde(alias = "manylinux_2_36_aarch64")]
    Aarch64Manylinux236,

    /// An ARM64 target for the `manylinux_2_37` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "aarch64-manylinux_2_37", alias = "manylinux_2_37_aarch64")
    )]
    #[serde(rename = "aarch64-manylinux_2_37")]
    #[serde(alias = "aarch64-manylinux237")]
    #[serde(alias = "manylinux_2_37_aarch64")]
    Aarch64Manylinux237,

    /// An ARM64 target for the `manylinux_2_38` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "aarch64-manylinux_2_38", alias = "manylinux_2_38_aarch64")
    )]
    #[serde(rename = "aarch64-manylinux_2_38")]
    #[serde(alias = "aarch64-manylinux238")]
    #[serde(alias = "manylinux_2_38_aarch64")]
    Aarch64Manylinux238,

    /// An ARM64 target for the `manylinux_2_39` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "aarch64-manylinux_2_39", alias = "manylinux_2_39_aarch64")
    )]
    #[serde(rename = "aarch64-manylinux_2_39")]
    #[serde(alias = "aarch64-manylinux239")]
    #[serde(alias = "manylinux_2_39_aarch64")]
    Aarch64Manylinux239,

    /// An ARM64 target for the `manylinux_2_40` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "aarch64-manylinux_2_40", alias = "manylinux_2_40_aarch64")
    )]
    #[serde(rename = "aarch64-manylinux_2_40")]
    #[serde(alias = "aarch64-manylinux240")]
    #[serde(alias = "manylinux_2_40_aarch64")]
    Aarch64Manylinux240,

    /// An s390x target for the `manylinux2014` platform. Equivalent to `s390x-manylinux_2_17`.
    #[cfg_attr(
        feature = "clap",
        value(name = "s390x-manylinux2014", alias = "manylinux2014_s390x")
    )]
    #[serde(rename = "s390x-manylinux2014")]
    #[serde(alias = "manylinux2014_s390x")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    S390XManylinux2014,

    /// An s390x target for the `manylinux_2_17` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "s390x-manylinux_2_17", alias = "manylinux_2_17_s390x")
    )]
    #[serde(rename = "s390x-manylinux_2_17")]
    #[serde(alias = "manylinux_2_17_s390x")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    S390XManylinux217,

    /// An s390x target for the `manylinux_2_28` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "s390x-manylinux_2_28", alias = "manylinux_2_28_s390x")
    )]
    #[serde(rename = "s390x-manylinux_2_28")]
    #[serde(alias = "manylinux_2_28_s390x")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    S390XManylinux228,

    /// An s390x target for the `manylinux_2_31` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "s390x-manylinux_2_31", alias = "manylinux_2_31_s390x")
    )]
    #[serde(rename = "s390x-manylinux_2_31")]
    #[serde(alias = "manylinux_2_31_s390x")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    S390XManylinux231,

    /// An s390x target for the `manylinux_2_32` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "s390x-manylinux_2_32", alias = "manylinux_2_32_s390x")
    )]
    #[serde(rename = "s390x-manylinux_2_32")]
    #[serde(alias = "manylinux_2_32_s390x")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    S390XManylinux232,

    /// An s390x target for the `manylinux_2_33` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "s390x-manylinux_2_33", alias = "manylinux_2_33_s390x")
    )]
    #[serde(rename = "s390x-manylinux_2_33")]
    #[serde(alias = "manylinux_2_33_s390x")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    S390XManylinux233,

    /// An s390x target for the `manylinux_2_34` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "s390x-manylinux_2_34", alias = "manylinux_2_34_s390x")
    )]
    #[serde(rename = "s390x-manylinux_2_34")]
    #[serde(alias = "manylinux_2_34_s390x")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    S390XManylinux234,

    /// An s390x target for the `manylinux_2_35` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "s390x-manylinux_2_35", alias = "manylinux_2_35_s390x")
    )]
    #[serde(rename = "s390x-manylinux_2_35")]
    #[serde(alias = "manylinux_2_35_s390x")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    S390XManylinux235,

    /// An s390x target for the `manylinux_2_36` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "s390x-manylinux_2_36", alias = "manylinux_2_36_s390x")
    )]
    #[serde(rename = "s390x-manylinux_2_36")]
    #[serde(alias = "manylinux_2_36_s390x")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    S390XManylinux236,

    /// An s390x target for the `manylinux_2_37` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "s390x-manylinux_2_37", alias = "manylinux_2_37_s390x")
    )]
    #[serde(rename = "s390x-manylinux_2_37")]
    #[serde(alias = "manylinux_2_37_s390x")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    S390XManylinux237,

    /// An s390x target for the `manylinux_2_38` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "s390x-manylinux_2_38", alias = "manylinux_2_38_s390x")
    )]
    #[serde(rename = "s390x-manylinux_2_38")]
    #[serde(alias = "manylinux_2_38_s390x")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    S390XManylinux238,

    /// An s390x target for the `manylinux_2_39` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "s390x-manylinux_2_39", alias = "manylinux_2_39_s390x")
    )]
    #[serde(rename = "s390x-manylinux_2_39")]
    #[serde(alias = "manylinux_2_39_s390x")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    S390XManylinux239,

    /// An s390x target for the `manylinux_2_40` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "s390x-manylinux_2_40", alias = "manylinux_2_40_s390x")
    )]
    #[serde(rename = "s390x-manylinux_2_40")]
    #[serde(alias = "manylinux_2_40_s390x")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    S390XManylinux240,

    /// A little-endian `PowerPC64` target for the `manylinux2014` platform. Equivalent to `ppc64le-manylinux_2_17`.
    #[cfg_attr(
        feature = "clap",
        value(name = "ppc64le-manylinux2014", alias = "manylinux2014_ppc64le")
    )]
    #[serde(rename = "ppc64le-manylinux2014")]
    #[serde(alias = "manylinux2014_ppc64le")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    Powerpc64LeManylinux2014,

    /// A little-endian `PowerPC64` target for the `manylinux_2_17` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "ppc64le-manylinux_2_17", alias = "manylinux_2_17_ppc64le")
    )]
    #[serde(rename = "ppc64le-manylinux_2_17")]
    #[serde(alias = "manylinux_2_17_ppc64le")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    Powerpc64LeManylinux217,

    /// A little-endian `PowerPC64` target for the `manylinux_2_28` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "ppc64le-manylinux_2_28", alias = "manylinux_2_28_ppc64le")
    )]
    #[serde(rename = "ppc64le-manylinux_2_28")]
    #[serde(alias = "manylinux_2_28_ppc64le")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    Powerpc64LeManylinux228,

    /// A little-endian `PowerPC64` target for the `manylinux_2_31` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "ppc64le-manylinux_2_31", alias = "manylinux_2_31_ppc64le")
    )]
    #[serde(rename = "ppc64le-manylinux_2_31")]
    #[serde(alias = "manylinux_2_31_ppc64le")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    Powerpc64LeManylinux231,

    /// A little-endian `PowerPC64` target for the `manylinux_2_32` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "ppc64le-manylinux_2_32", alias = "manylinux_2_32_ppc64le")
    )]
    #[serde(rename = "ppc64le-manylinux_2_32")]
    #[serde(alias = "manylinux_2_32_ppc64le")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    Powerpc64LeManylinux232,

    /// A little-endian `PowerPC64` target for the `manylinux_2_33` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "ppc64le-manylinux_2_33", alias = "manylinux_2_33_ppc64le")
    )]
    #[serde(rename = "ppc64le-manylinux_2_33")]
    #[serde(alias = "manylinux_2_33_ppc64le")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    Powerpc64LeManylinux233,

    /// A little-endian `PowerPC64` target for the `manylinux_2_34` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "ppc64le-manylinux_2_34", alias = "manylinux_2_34_ppc64le")
    )]
    #[serde(rename = "ppc64le-manylinux_2_34")]
    #[serde(alias = "manylinux_2_34_ppc64le")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    Powerpc64LeManylinux234,

    /// A little-endian `PowerPC64` target for the `manylinux_2_35` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "ppc64le-manylinux_2_35", alias = "manylinux_2_35_ppc64le")
    )]
    #[serde(rename = "ppc64le-manylinux_2_35")]
    #[serde(alias = "manylinux_2_35_ppc64le")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    Powerpc64LeManylinux235,

    /// A little-endian `PowerPC64` target for the `manylinux_2_36` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "ppc64le-manylinux_2_36", alias = "manylinux_2_36_ppc64le")
    )]
    #[serde(rename = "ppc64le-manylinux_2_36")]
    #[serde(alias = "manylinux_2_36_ppc64le")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    Powerpc64LeManylinux236,

    /// A little-endian `PowerPC64` target for the `manylinux_2_37` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "ppc64le-manylinux_2_37", alias = "manylinux_2_37_ppc64le")
    )]
    #[serde(rename = "ppc64le-manylinux_2_37")]
    #[serde(alias = "manylinux_2_37_ppc64le")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    Powerpc64LeManylinux237,

    /// A little-endian `PowerPC64` target for the `manylinux_2_38` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "ppc64le-manylinux_2_38", alias = "manylinux_2_38_ppc64le")
    )]
    #[serde(rename = "ppc64le-manylinux_2_38")]
    #[serde(alias = "manylinux_2_38_ppc64le")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    Powerpc64LeManylinux238,

    /// A little-endian `PowerPC64` target for the `manylinux_2_39` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "ppc64le-manylinux_2_39", alias = "manylinux_2_39_ppc64le")
    )]
    #[serde(rename = "ppc64le-manylinux_2_39")]
    #[serde(alias = "manylinux_2_39_ppc64le")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    Powerpc64LeManylinux239,

    /// A little-endian `PowerPC64` target for the `manylinux_2_40` platform.
    #[cfg_attr(
        feature = "clap",
        value(name = "ppc64le-manylinux_2_40", alias = "manylinux_2_40_ppc64le")
    )]
    #[serde(rename = "ppc64le-manylinux_2_40")]
    #[serde(alias = "manylinux_2_40_ppc64le")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    Powerpc64LeManylinux240,

    /// A `LoongArch64` target for the `manylinux_2_36` platform.
    #[cfg_attr(
        feature = "clap",
        value(
            name = "loongarch64-manylinux_2_36",
            alias = "manylinux_2_36_loongarch64"
        )
    )]
    #[serde(rename = "loongarch64-manylinux_2_36")]
    #[serde(alias = "manylinux_2_36_loongarch64")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    LoongArch64Manylinux236,

    /// A `LoongArch64` target for the `manylinux_2_37` platform.
    #[cfg_attr(
        feature = "clap",
        value(
            name = "loongarch64-manylinux_2_37",
            alias = "manylinux_2_37_loongarch64"
        )
    )]
    #[serde(rename = "loongarch64-manylinux_2_37")]
    #[serde(alias = "manylinux_2_37_loongarch64")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    LoongArch64Manylinux237,

    /// A `LoongArch64` target for the `manylinux_2_38` platform.
    #[cfg_attr(
        feature = "clap",
        value(
            name = "loongarch64-manylinux_2_38",
            alias = "manylinux_2_38_loongarch64"
        )
    )]
    #[serde(rename = "loongarch64-manylinux_2_38")]
    #[serde(alias = "manylinux_2_38_loongarch64")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    LoongArch64Manylinux238,

    /// A `LoongArch64` target for the `manylinux_2_39` platform.
    #[cfg_attr(
        feature = "clap",
        value(
            name = "loongarch64-manylinux_2_39",
            alias = "manylinux_2_39_loongarch64"
        )
    )]
    #[serde(rename = "loongarch64-manylinux_2_39")]
    #[serde(alias = "manylinux_2_39_loongarch64")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    LoongArch64Manylinux239,

    /// A `LoongArch64` target for the `manylinux_2_40` platform.
    #[cfg_attr(
        feature = "clap",
        value(
            name = "loongarch64-manylinux_2_40",
            alias = "manylinux_2_40_loongarch64"
        )
    )]
    #[serde(rename = "loongarch64-manylinux_2_40")]
    #[serde(alias = "manylinux_2_40_loongarch64")]
    #[cfg_attr(feature = "clap", value(hide = true))]
    LoongArch64Manylinux240,

    /// An ARM64 Android target.
    ///
    /// By default uses Android API level 24, but respects
    /// the `ANDROID_API_LEVEL` environment variable if set.
    #[cfg_attr(feature = "clap", value(name = "aarch64-linux-android"))]
    #[serde(rename = "aarch64-linux-android")]
    Aarch64LinuxAndroid,

    /// An `x86_64` Android target.
    ///
    /// By default uses Android API level 24, but respects
    /// the `ANDROID_API_LEVEL` environment variable if set.
    #[cfg_attr(feature = "clap", value(name = "x86_64-linux-android"))]
    #[serde(rename = "x86_64-linux-android")]
    X8664LinuxAndroid,

    /// A wasm32 target using the Pyodide 2024 platform. Meant for use with Python 3.12.
    /// See <https://pyodide.org/en/stable/development/abi/312.html>
    #[cfg_attr(feature = "clap", value(name = "wasm32-pyodide2024"))]
    Wasm32Pyodide2024,

    /// A wasm32 target using the Pyodide 2025 platform. Meant for use with Python 3.13.
    /// See <https://pyodide.org/en/stable/development/abi/313.html>
    #[cfg_attr(feature = "clap", value(name = "wasm32-pyodide2025"))]
    Wasm32Pyodide2025,

    /// An ARM64 target for iOS device
    ///
    /// By default, iOS 13.0 is used, but respects the `IPHONEOS_DEPLOYMENT_TARGET`
    /// environment variable if set.
    #[cfg_attr(feature = "clap", value(name = "arm64-apple-ios"))]
    #[serde(rename = "arm64-apple-ios")]
    Arm64Ios,

    /// An ARM64 target for iOS simulator
    ///
    /// By default, iOS 13.0 is used, but respects the `IPHONEOS_DEPLOYMENT_TARGET`
    /// environment variable if set.
    #[cfg_attr(feature = "clap", value(name = "arm64-apple-ios-simulator"))]
    #[serde(rename = "arm64-apple-ios-simulator")]
    Arm64IosSimulator,

    /// An `x86_64` target for iOS simulator
    ///
    /// By default, iOS 13.0 is used, but respects the `IPHONEOS_DEPLOYMENT_TARGET`
    /// environment variable if set.
    #[cfg_attr(feature = "clap", value(name = "x86_64-apple-ios-simulator"))]
    #[serde(rename = "x86_64-apple-ios-simulator")]
    X8664IosSimulator,
}

impl TargetTriple {
    /// Return the [`Platform`] for the target.
    pub fn platform(self) -> Platform {
        match self {
            Self::Windows | Self::X8664PcWindowsMsvc => Platform::new(Os::Windows, Arch::X86_64),
            Self::Aarch64PcWindowsMsvc => Platform::new(Os::Windows, Arch::Aarch64),
            Self::Linux | Self::X8664UnknownLinuxGnu => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 28,
                },
                Arch::X86_64,
            ),
            Self::Macos | Self::Aarch64AppleDarwin => {
                let (major, minor) = macos_deployment_target().map_or((13, 0), |(major, minor)| {
                    debug!("Found macOS deployment target: {}.{}", major, minor);
                    (major, minor)
                });
                Platform::new(Os::Macos { major, minor }, Arch::Aarch64)
            }
            Self::I686PcWindowsMsvc => Platform::new(Os::Windows, Arch::X86),
            Self::X8664AppleDarwin => {
                let (major, minor) = macos_deployment_target().map_or((13, 0), |(major, minor)| {
                    debug!("Found macOS deployment target: {}.{}", major, minor);
                    (major, minor)
                });
                Platform::new(Os::Macos { major, minor }, Arch::X86_64)
            }
            Self::Aarch64UnknownLinuxGnu => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 28,
                },
                Arch::Aarch64,
            ),
            Self::S390XUnknownLinuxGnu => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 28,
                },
                Arch::S390X,
            ),
            Self::Powerpc64LeUnknownLinuxGnu => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 28,
                },
                Arch::Powerpc64Le,
            ),
            Self::LoongArch64UnknownLinuxGnu => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 36,
                },
                Arch::LoongArch64,
            ),
            Self::Riscv64UnknownLinuxGnu => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 39,
                },
                Arch::Riscv64,
            ),
            Self::Aarch64UnknownLinuxMusl => {
                Platform::new(Os::Musllinux { major: 1, minor: 2 }, Arch::Aarch64)
            }
            Self::X8664UnknownLinuxMusl => {
                Platform::new(Os::Musllinux { major: 1, minor: 2 }, Arch::X86_64)
            }
            Self::X8664Manylinux2014 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 17,
                },
                Arch::X86_64,
            ),
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
            Self::X8664Manylinux232 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 32,
                },
                Arch::X86_64,
            ),
            Self::X8664Manylinux233 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 33,
                },
                Arch::X86_64,
            ),
            Self::X8664Manylinux234 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 34,
                },
                Arch::X86_64,
            ),
            Self::X8664Manylinux235 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 35,
                },
                Arch::X86_64,
            ),
            Self::X8664Manylinux236 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 36,
                },
                Arch::X86_64,
            ),
            Self::X8664Manylinux237 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 37,
                },
                Arch::X86_64,
            ),
            Self::X8664Manylinux238 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 38,
                },
                Arch::X86_64,
            ),
            Self::X8664Manylinux239 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 39,
                },
                Arch::X86_64,
            ),
            Self::X8664Manylinux240 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 40,
                },
                Arch::X86_64,
            ),
            Self::Aarch64Manylinux2014 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 17,
                },
                Arch::Aarch64,
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
            Self::Aarch64Manylinux232 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 32,
                },
                Arch::Aarch64,
            ),
            Self::Aarch64Manylinux233 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 33,
                },
                Arch::Aarch64,
            ),
            Self::Aarch64Manylinux234 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 34,
                },
                Arch::Aarch64,
            ),
            Self::Aarch64Manylinux235 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 35,
                },
                Arch::Aarch64,
            ),
            Self::Aarch64Manylinux236 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 36,
                },
                Arch::Aarch64,
            ),
            Self::Aarch64Manylinux237 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 37,
                },
                Arch::Aarch64,
            ),
            Self::Aarch64Manylinux238 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 38,
                },
                Arch::Aarch64,
            ),
            Self::Aarch64Manylinux239 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 39,
                },
                Arch::Aarch64,
            ),
            Self::Aarch64Manylinux240 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 40,
                },
                Arch::Aarch64,
            ),
            Self::S390XManylinux2014 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 17,
                },
                Arch::S390X,
            ),
            Self::S390XManylinux217 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 17,
                },
                Arch::S390X,
            ),
            Self::S390XManylinux228 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 28,
                },
                Arch::S390X,
            ),
            Self::S390XManylinux231 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 31,
                },
                Arch::S390X,
            ),
            Self::S390XManylinux232 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 32,
                },
                Arch::S390X,
            ),
            Self::S390XManylinux233 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 33,
                },
                Arch::S390X,
            ),
            Self::S390XManylinux234 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 34,
                },
                Arch::S390X,
            ),
            Self::S390XManylinux235 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 35,
                },
                Arch::S390X,
            ),
            Self::S390XManylinux236 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 36,
                },
                Arch::S390X,
            ),
            Self::S390XManylinux237 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 37,
                },
                Arch::S390X,
            ),
            Self::S390XManylinux238 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 38,
                },
                Arch::S390X,
            ),
            Self::S390XManylinux239 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 39,
                },
                Arch::S390X,
            ),
            Self::S390XManylinux240 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 40,
                },
                Arch::S390X,
            ),
            Self::Powerpc64LeManylinux2014 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 17,
                },
                Arch::Powerpc64Le,
            ),
            Self::Powerpc64LeManylinux217 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 17,
                },
                Arch::Powerpc64Le,
            ),
            Self::Powerpc64LeManylinux228 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 28,
                },
                Arch::Powerpc64Le,
            ),
            Self::Powerpc64LeManylinux231 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 31,
                },
                Arch::Powerpc64Le,
            ),
            Self::Powerpc64LeManylinux232 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 32,
                },
                Arch::Powerpc64Le,
            ),
            Self::Powerpc64LeManylinux233 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 33,
                },
                Arch::Powerpc64Le,
            ),
            Self::Powerpc64LeManylinux234 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 34,
                },
                Arch::Powerpc64Le,
            ),
            Self::Powerpc64LeManylinux235 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 35,
                },
                Arch::Powerpc64Le,
            ),
            Self::Powerpc64LeManylinux236 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 36,
                },
                Arch::Powerpc64Le,
            ),
            Self::Powerpc64LeManylinux237 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 37,
                },
                Arch::Powerpc64Le,
            ),
            Self::Powerpc64LeManylinux238 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 38,
                },
                Arch::Powerpc64Le,
            ),
            Self::Powerpc64LeManylinux239 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 39,
                },
                Arch::Powerpc64Le,
            ),
            Self::Powerpc64LeManylinux240 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 40,
                },
                Arch::Powerpc64Le,
            ),
            Self::LoongArch64Manylinux236 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 36,
                },
                Arch::LoongArch64,
            ),
            Self::LoongArch64Manylinux237 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 37,
                },
                Arch::LoongArch64,
            ),
            Self::LoongArch64Manylinux238 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 38,
                },
                Arch::LoongArch64,
            ),
            Self::LoongArch64Manylinux239 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 39,
                },
                Arch::LoongArch64,
            ),
            Self::LoongArch64Manylinux240 => Platform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 40,
                },
                Arch::LoongArch64,
            ),
            Self::Wasm32Pyodide2024 => Platform::new(
                Os::Pyodide {
                    major: 2024,
                    minor: 0,
                },
                Arch::Wasm32,
            ),
            Self::Wasm32Pyodide2025 => Platform::new(
                Os::Pyodide {
                    major: 2025,
                    minor: 0,
                },
                Arch::Wasm32,
            ),
            Self::Aarch64LinuxAndroid => {
                let api_level = android_api_level().map_or(24, |api_level| {
                    debug!("Found Android API level: {}", api_level);
                    api_level
                });
                Platform::new(Os::Android { api_level }, Arch::Aarch64)
            }
            Self::X8664LinuxAndroid => {
                let api_level = android_api_level().map_or(24, |api_level| {
                    debug!("Found Android API level: {}", api_level);
                    api_level
                });
                Platform::new(Os::Android { api_level }, Arch::X86_64)
            }
            Self::Arm64Ios => {
                let (major, minor) = ios_deployment_target().map_or((13, 0), |(major, minor)| {
                    debug!("Found iOS deployment target: {}.{}", major, minor);
                    (major, minor)
                });
                Platform::new(
                    Os::Ios {
                        major,
                        minor,
                        simulator: false,
                    },
                    Arch::Aarch64,
                )
            }
            Self::Arm64IosSimulator => {
                let (major, minor) = ios_deployment_target().map_or((13, 0), |(major, minor)| {
                    debug!("Found iOS deployment target: {}.{}", major, minor);
                    (major, minor)
                });
                Platform::new(
                    Os::Ios {
                        major,
                        minor,
                        simulator: true,
                    },
                    Arch::Aarch64,
                )
            }
            Self::X8664IosSimulator => {
                let (major, minor) = ios_deployment_target().map_or((13, 0), |(major, minor)| {
                    debug!("Found iOS deployment target: {}.{}", major, minor);
                    (major, minor)
                });
                Platform::new(
                    Os::Ios {
                        major,
                        minor,
                        simulator: true,
                    },
                    Arch::X86_64,
                )
            }
        }
    }

    /// Return the `platform_machine` value for the target.
    fn platform_machine(self) -> &'static str {
        match self {
            Self::Windows | Self::X8664PcWindowsMsvc => "x86_64",
            Self::Aarch64PcWindowsMsvc => "ARM64",
            Self::Linux | Self::X8664UnknownLinuxGnu => "x86_64",
            Self::Macos | Self::Aarch64AppleDarwin => "arm64",
            Self::I686PcWindowsMsvc => "x86",
            Self::X8664AppleDarwin => "x86_64",
            Self::Aarch64UnknownLinuxGnu => "aarch64",
            Self::Aarch64UnknownLinuxMusl => "aarch64",
            Self::X8664UnknownLinuxMusl => "x86_64",
            Self::Riscv64UnknownLinuxGnu => "riscv64",
            Self::S390XUnknownLinuxGnu => "s390x",
            Self::Powerpc64LeUnknownLinuxGnu => "ppc64le",
            Self::LoongArch64UnknownLinuxGnu => "loongarch64",
            Self::X8664Manylinux2014 => "x86_64",
            Self::X8664Manylinux217 => "x86_64",
            Self::X8664Manylinux228 => "x86_64",
            Self::X8664Manylinux231 => "x86_64",
            Self::X8664Manylinux232 => "x86_64",
            Self::X8664Manylinux233 => "x86_64",
            Self::X8664Manylinux234 => "x86_64",
            Self::X8664Manylinux235 => "x86_64",
            Self::X8664Manylinux236 => "x86_64",
            Self::X8664Manylinux237 => "x86_64",
            Self::X8664Manylinux238 => "x86_64",
            Self::X8664Manylinux239 => "x86_64",
            Self::X8664Manylinux240 => "x86_64",
            Self::Aarch64Manylinux2014 => "aarch64",
            Self::Aarch64Manylinux217 => "aarch64",
            Self::Aarch64Manylinux228 => "aarch64",
            Self::Aarch64Manylinux231 => "aarch64",
            Self::Aarch64Manylinux232 => "aarch64",
            Self::Aarch64Manylinux233 => "aarch64",
            Self::Aarch64Manylinux234 => "aarch64",
            Self::Aarch64Manylinux235 => "aarch64",
            Self::Aarch64Manylinux236 => "aarch64",
            Self::Aarch64Manylinux237 => "aarch64",
            Self::Aarch64Manylinux238 => "aarch64",
            Self::Aarch64Manylinux239 => "aarch64",
            Self::Aarch64Manylinux240 => "aarch64",
            Self::S390XManylinux2014 => "s390x",
            Self::S390XManylinux217 => "s390x",
            Self::S390XManylinux228 => "s390x",
            Self::S390XManylinux231 => "s390x",
            Self::S390XManylinux232 => "s390x",
            Self::S390XManylinux233 => "s390x",
            Self::S390XManylinux234 => "s390x",
            Self::S390XManylinux235 => "s390x",
            Self::S390XManylinux236 => "s390x",
            Self::S390XManylinux237 => "s390x",
            Self::S390XManylinux238 => "s390x",
            Self::S390XManylinux239 => "s390x",
            Self::S390XManylinux240 => "s390x",
            Self::Powerpc64LeManylinux2014 => "ppc64le",
            Self::Powerpc64LeManylinux217 => "ppc64le",
            Self::Powerpc64LeManylinux228 => "ppc64le",
            Self::Powerpc64LeManylinux231 => "ppc64le",
            Self::Powerpc64LeManylinux232 => "ppc64le",
            Self::Powerpc64LeManylinux233 => "ppc64le",
            Self::Powerpc64LeManylinux234 => "ppc64le",
            Self::Powerpc64LeManylinux235 => "ppc64le",
            Self::Powerpc64LeManylinux236 => "ppc64le",
            Self::Powerpc64LeManylinux237 => "ppc64le",
            Self::Powerpc64LeManylinux238 => "ppc64le",
            Self::Powerpc64LeManylinux239 => "ppc64le",
            Self::Powerpc64LeManylinux240 => "ppc64le",
            Self::LoongArch64Manylinux236 => "loongarch64",
            Self::LoongArch64Manylinux237 => "loongarch64",
            Self::LoongArch64Manylinux238 => "loongarch64",
            Self::LoongArch64Manylinux239 => "loongarch64",
            Self::LoongArch64Manylinux240 => "loongarch64",
            Self::Aarch64LinuxAndroid => "aarch64",
            Self::X8664LinuxAndroid => "x86_64",
            Self::Wasm32Pyodide2024 => "wasm32",
            Self::Wasm32Pyodide2025 => "wasm32",
            Self::Arm64Ios => "arm64",
            Self::Arm64IosSimulator => "arm64",
            Self::X8664IosSimulator => "x86_64",
        }
    }

    /// Return the `platform_system` value for the target.
    fn platform_system(self) -> &'static str {
        match self {
            Self::Windows | Self::X8664PcWindowsMsvc => "Windows",
            Self::Aarch64PcWindowsMsvc => "Windows",
            Self::Linux | Self::X8664UnknownLinuxGnu => "Linux",
            Self::Macos | Self::Aarch64AppleDarwin => "Darwin",
            Self::I686PcWindowsMsvc => "Windows",
            Self::X8664AppleDarwin => "Darwin",
            Self::Aarch64UnknownLinuxGnu => "Linux",
            Self::Aarch64UnknownLinuxMusl => "Linux",
            Self::X8664UnknownLinuxMusl => "Linux",
            Self::Riscv64UnknownLinuxGnu => "Linux",
            Self::S390XUnknownLinuxGnu => "Linux",
            Self::Powerpc64LeUnknownLinuxGnu => "Linux",
            Self::LoongArch64UnknownLinuxGnu => "Linux",
            Self::X8664Manylinux2014 => "Linux",
            Self::X8664Manylinux217 => "Linux",
            Self::X8664Manylinux228 => "Linux",
            Self::X8664Manylinux231 => "Linux",
            Self::X8664Manylinux232 => "Linux",
            Self::X8664Manylinux233 => "Linux",
            Self::X8664Manylinux234 => "Linux",
            Self::X8664Manylinux235 => "Linux",
            Self::X8664Manylinux236 => "Linux",
            Self::X8664Manylinux237 => "Linux",
            Self::X8664Manylinux238 => "Linux",
            Self::X8664Manylinux239 => "Linux",
            Self::X8664Manylinux240 => "Linux",
            Self::Aarch64Manylinux2014 => "Linux",
            Self::Aarch64Manylinux217 => "Linux",
            Self::Aarch64Manylinux228 => "Linux",
            Self::Aarch64Manylinux231 => "Linux",
            Self::Aarch64Manylinux232 => "Linux",
            Self::Aarch64Manylinux233 => "Linux",
            Self::Aarch64Manylinux234 => "Linux",
            Self::Aarch64Manylinux235 => "Linux",
            Self::Aarch64Manylinux236 => "Linux",
            Self::Aarch64Manylinux237 => "Linux",
            Self::Aarch64Manylinux238 => "Linux",
            Self::Aarch64Manylinux239 => "Linux",
            Self::Aarch64Manylinux240 => "Linux",
            Self::S390XManylinux2014 => "Linux",
            Self::S390XManylinux217 => "Linux",
            Self::S390XManylinux228 => "Linux",
            Self::S390XManylinux231 => "Linux",
            Self::S390XManylinux232 => "Linux",
            Self::S390XManylinux233 => "Linux",
            Self::S390XManylinux234 => "Linux",
            Self::S390XManylinux235 => "Linux",
            Self::S390XManylinux236 => "Linux",
            Self::S390XManylinux237 => "Linux",
            Self::S390XManylinux238 => "Linux",
            Self::S390XManylinux239 => "Linux",
            Self::S390XManylinux240 => "Linux",
            Self::Powerpc64LeManylinux2014 => "Linux",
            Self::Powerpc64LeManylinux217 => "Linux",
            Self::Powerpc64LeManylinux228 => "Linux",
            Self::Powerpc64LeManylinux231 => "Linux",
            Self::Powerpc64LeManylinux232 => "Linux",
            Self::Powerpc64LeManylinux233 => "Linux",
            Self::Powerpc64LeManylinux234 => "Linux",
            Self::Powerpc64LeManylinux235 => "Linux",
            Self::Powerpc64LeManylinux236 => "Linux",
            Self::Powerpc64LeManylinux237 => "Linux",
            Self::Powerpc64LeManylinux238 => "Linux",
            Self::Powerpc64LeManylinux239 => "Linux",
            Self::Powerpc64LeManylinux240 => "Linux",
            Self::LoongArch64Manylinux236 => "Linux",
            Self::LoongArch64Manylinux237 => "Linux",
            Self::LoongArch64Manylinux238 => "Linux",
            Self::LoongArch64Manylinux239 => "Linux",
            Self::LoongArch64Manylinux240 => "Linux",
            Self::Aarch64LinuxAndroid => "Android",
            Self::X8664LinuxAndroid => "Android",
            Self::Wasm32Pyodide2024 => "Emscripten",
            Self::Wasm32Pyodide2025 => "Emscripten",
            Self::Arm64Ios => "iOS",
            Self::Arm64IosSimulator => "iOS",
            Self::X8664IosSimulator => "iOS",
        }
    }

    /// Return the `platform_version` value for the target.
    fn platform_version(self) -> &'static str {
        match self {
            Self::Windows | Self::X8664PcWindowsMsvc => "",
            Self::Aarch64PcWindowsMsvc => "",
            Self::Linux | Self::X8664UnknownLinuxGnu => "",
            Self::Macos | Self::Aarch64AppleDarwin => "",
            Self::I686PcWindowsMsvc => "",
            Self::X8664AppleDarwin => "",
            Self::Aarch64UnknownLinuxGnu => "",
            Self::Aarch64UnknownLinuxMusl => "",
            Self::X8664UnknownLinuxMusl => "",
            Self::Riscv64UnknownLinuxGnu => "",
            Self::S390XUnknownLinuxGnu => "",
            Self::Powerpc64LeUnknownLinuxGnu => "",
            Self::LoongArch64UnknownLinuxGnu => "",
            Self::X8664Manylinux2014 => "",
            Self::X8664Manylinux217 => "",
            Self::X8664Manylinux228 => "",
            Self::X8664Manylinux231 => "",
            Self::X8664Manylinux232 => "",
            Self::X8664Manylinux233 => "",
            Self::X8664Manylinux234 => "",
            Self::X8664Manylinux235 => "",
            Self::X8664Manylinux236 => "",
            Self::X8664Manylinux237 => "",
            Self::X8664Manylinux238 => "",
            Self::X8664Manylinux239 => "",
            Self::X8664Manylinux240 => "",
            Self::Aarch64Manylinux2014 => "",
            Self::Aarch64Manylinux217 => "",
            Self::Aarch64Manylinux228 => "",
            Self::Aarch64Manylinux231 => "",
            Self::Aarch64Manylinux232 => "",
            Self::Aarch64Manylinux233 => "",
            Self::Aarch64Manylinux234 => "",
            Self::Aarch64Manylinux235 => "",
            Self::Aarch64Manylinux236 => "",
            Self::Aarch64Manylinux237 => "",
            Self::Aarch64Manylinux238 => "",
            Self::Aarch64Manylinux239 => "",
            Self::Aarch64Manylinux240 => "",
            Self::S390XManylinux2014 => "",
            Self::S390XManylinux217 => "",
            Self::S390XManylinux228 => "",
            Self::S390XManylinux231 => "",
            Self::S390XManylinux232 => "",
            Self::S390XManylinux233 => "",
            Self::S390XManylinux234 => "",
            Self::S390XManylinux235 => "",
            Self::S390XManylinux236 => "",
            Self::S390XManylinux237 => "",
            Self::S390XManylinux238 => "",
            Self::S390XManylinux239 => "",
            Self::S390XManylinux240 => "",
            Self::Powerpc64LeManylinux2014 => "",
            Self::Powerpc64LeManylinux217 => "",
            Self::Powerpc64LeManylinux228 => "",
            Self::Powerpc64LeManylinux231 => "",
            Self::Powerpc64LeManylinux232 => "",
            Self::Powerpc64LeManylinux233 => "",
            Self::Powerpc64LeManylinux234 => "",
            Self::Powerpc64LeManylinux235 => "",
            Self::Powerpc64LeManylinux236 => "",
            Self::Powerpc64LeManylinux237 => "",
            Self::Powerpc64LeManylinux238 => "",
            Self::Powerpc64LeManylinux239 => "",
            Self::Powerpc64LeManylinux240 => "",
            Self::LoongArch64Manylinux236 => "",
            Self::LoongArch64Manylinux237 => "",
            Self::LoongArch64Manylinux238 => "",
            Self::LoongArch64Manylinux239 => "",
            Self::LoongArch64Manylinux240 => "",
            Self::Aarch64LinuxAndroid => "",
            Self::X8664LinuxAndroid => "",
            // This is the value Emscripten gives for its version:
            // https://github.com/emscripten-core/emscripten/blob/4.0.8/system/lib/libc/emscripten_syscall_stubs.c#L63
            // It doesn't really seem to mean anything? But for completeness we include it here.
            Self::Wasm32Pyodide2024 => "#1",
            Self::Wasm32Pyodide2025 => "#1",
            Self::Arm64Ios => "",
            Self::Arm64IosSimulator => "",
            Self::X8664IosSimulator => "",
        }
    }

    /// Return the `platform_release` value for the target.
    fn platform_release(self) -> &'static str {
        match self {
            Self::Windows | Self::X8664PcWindowsMsvc => "",
            Self::Aarch64PcWindowsMsvc => "",
            Self::Linux | Self::X8664UnknownLinuxGnu => "",
            Self::Macos | Self::Aarch64AppleDarwin => "",
            Self::I686PcWindowsMsvc => "",
            Self::X8664AppleDarwin => "",
            Self::Aarch64UnknownLinuxGnu => "",
            Self::Aarch64UnknownLinuxMusl => "",
            Self::X8664UnknownLinuxMusl => "",
            Self::Riscv64UnknownLinuxGnu => "",
            Self::S390XUnknownLinuxGnu => "",
            Self::Powerpc64LeUnknownLinuxGnu => "",
            Self::LoongArch64UnknownLinuxGnu => "",
            Self::X8664Manylinux2014 => "",
            Self::X8664Manylinux217 => "",
            Self::X8664Manylinux228 => "",
            Self::X8664Manylinux231 => "",
            Self::X8664Manylinux232 => "",
            Self::X8664Manylinux233 => "",
            Self::X8664Manylinux234 => "",
            Self::X8664Manylinux235 => "",
            Self::X8664Manylinux236 => "",
            Self::X8664Manylinux237 => "",
            Self::X8664Manylinux238 => "",
            Self::X8664Manylinux239 => "",
            Self::X8664Manylinux240 => "",
            Self::Aarch64Manylinux2014 => "",
            Self::Aarch64Manylinux217 => "",
            Self::Aarch64Manylinux228 => "",
            Self::Aarch64Manylinux231 => "",
            Self::Aarch64Manylinux232 => "",
            Self::Aarch64Manylinux233 => "",
            Self::Aarch64Manylinux234 => "",
            Self::Aarch64Manylinux235 => "",
            Self::Aarch64Manylinux236 => "",
            Self::Aarch64Manylinux237 => "",
            Self::Aarch64Manylinux238 => "",
            Self::Aarch64Manylinux239 => "",
            Self::Aarch64Manylinux240 => "",
            Self::S390XManylinux2014 => "",
            Self::S390XManylinux217 => "",
            Self::S390XManylinux228 => "",
            Self::S390XManylinux231 => "",
            Self::S390XManylinux232 => "",
            Self::S390XManylinux233 => "",
            Self::S390XManylinux234 => "",
            Self::S390XManylinux235 => "",
            Self::S390XManylinux236 => "",
            Self::S390XManylinux237 => "",
            Self::S390XManylinux238 => "",
            Self::S390XManylinux239 => "",
            Self::S390XManylinux240 => "",
            Self::Powerpc64LeManylinux2014 => "",
            Self::Powerpc64LeManylinux217 => "",
            Self::Powerpc64LeManylinux228 => "",
            Self::Powerpc64LeManylinux231 => "",
            Self::Powerpc64LeManylinux232 => "",
            Self::Powerpc64LeManylinux233 => "",
            Self::Powerpc64LeManylinux234 => "",
            Self::Powerpc64LeManylinux235 => "",
            Self::Powerpc64LeManylinux236 => "",
            Self::Powerpc64LeManylinux237 => "",
            Self::Powerpc64LeManylinux238 => "",
            Self::Powerpc64LeManylinux239 => "",
            Self::Powerpc64LeManylinux240 => "",
            Self::LoongArch64Manylinux236 => "",
            Self::LoongArch64Manylinux237 => "",
            Self::LoongArch64Manylinux238 => "",
            Self::LoongArch64Manylinux239 => "",
            Self::LoongArch64Manylinux240 => "",
            Self::Aarch64LinuxAndroid => "",
            Self::X8664LinuxAndroid => "",
            // This is the Emscripten compiler version for Pyodide 2024.
            // See https://pyodide.org/en/stable/development/abi/312.html
            Self::Wasm32Pyodide2024 => "3.1.58",
            // See https://pyodide.org/en/stable/development/abi/313.html
            Self::Wasm32Pyodide2025 => "4.0.9",
            Self::Arm64Ios => "",
            Self::Arm64IosSimulator => "",
            Self::X8664IosSimulator => "",
        }
    }

    /// Return the `os_name` value for the target.
    fn os_name(self) -> &'static str {
        match self {
            Self::Windows | Self::X8664PcWindowsMsvc => "nt",
            Self::Aarch64PcWindowsMsvc => "nt",
            Self::Linux | Self::X8664UnknownLinuxGnu => "posix",
            Self::Macos | Self::Aarch64AppleDarwin => "posix",
            Self::I686PcWindowsMsvc => "nt",
            Self::X8664AppleDarwin => "posix",
            Self::Aarch64UnknownLinuxGnu => "posix",
            Self::Aarch64UnknownLinuxMusl => "posix",
            Self::X8664UnknownLinuxMusl => "posix",
            Self::Riscv64UnknownLinuxGnu => "posix",
            Self::S390XUnknownLinuxGnu => "posix",
            Self::Powerpc64LeUnknownLinuxGnu => "posix",
            Self::LoongArch64UnknownLinuxGnu => "posix",
            Self::X8664Manylinux2014 => "posix",
            Self::X8664Manylinux217 => "posix",
            Self::X8664Manylinux228 => "posix",
            Self::X8664Manylinux231 => "posix",
            Self::X8664Manylinux232 => "posix",
            Self::X8664Manylinux233 => "posix",
            Self::X8664Manylinux234 => "posix",
            Self::X8664Manylinux235 => "posix",
            Self::X8664Manylinux236 => "posix",
            Self::X8664Manylinux237 => "posix",
            Self::X8664Manylinux238 => "posix",
            Self::X8664Manylinux239 => "posix",
            Self::X8664Manylinux240 => "posix",
            Self::Aarch64Manylinux2014 => "posix",
            Self::Aarch64Manylinux217 => "posix",
            Self::Aarch64Manylinux228 => "posix",
            Self::Aarch64Manylinux231 => "posix",
            Self::Aarch64Manylinux232 => "posix",
            Self::Aarch64Manylinux233 => "posix",
            Self::Aarch64Manylinux234 => "posix",
            Self::Aarch64Manylinux235 => "posix",
            Self::Aarch64Manylinux236 => "posix",
            Self::Aarch64Manylinux237 => "posix",
            Self::Aarch64Manylinux238 => "posix",
            Self::Aarch64Manylinux239 => "posix",
            Self::Aarch64Manylinux240 => "posix",
            Self::S390XManylinux2014 => "posix",
            Self::S390XManylinux217 => "posix",
            Self::S390XManylinux228 => "posix",
            Self::S390XManylinux231 => "posix",
            Self::S390XManylinux232 => "posix",
            Self::S390XManylinux233 => "posix",
            Self::S390XManylinux234 => "posix",
            Self::S390XManylinux235 => "posix",
            Self::S390XManylinux236 => "posix",
            Self::S390XManylinux237 => "posix",
            Self::S390XManylinux238 => "posix",
            Self::S390XManylinux239 => "posix",
            Self::S390XManylinux240 => "posix",
            Self::Powerpc64LeManylinux2014 => "posix",
            Self::Powerpc64LeManylinux217 => "posix",
            Self::Powerpc64LeManylinux228 => "posix",
            Self::Powerpc64LeManylinux231 => "posix",
            Self::Powerpc64LeManylinux232 => "posix",
            Self::Powerpc64LeManylinux233 => "posix",
            Self::Powerpc64LeManylinux234 => "posix",
            Self::Powerpc64LeManylinux235 => "posix",
            Self::Powerpc64LeManylinux236 => "posix",
            Self::Powerpc64LeManylinux237 => "posix",
            Self::Powerpc64LeManylinux238 => "posix",
            Self::Powerpc64LeManylinux239 => "posix",
            Self::Powerpc64LeManylinux240 => "posix",
            Self::LoongArch64Manylinux236 => "posix",
            Self::LoongArch64Manylinux237 => "posix",
            Self::LoongArch64Manylinux238 => "posix",
            Self::LoongArch64Manylinux239 => "posix",
            Self::LoongArch64Manylinux240 => "posix",
            Self::Aarch64LinuxAndroid => "posix",
            Self::X8664LinuxAndroid => "posix",
            Self::Wasm32Pyodide2024 => "posix",
            Self::Wasm32Pyodide2025 => "posix",
            Self::Arm64Ios => "posix",
            Self::Arm64IosSimulator => "posix",
            Self::X8664IosSimulator => "posix",
        }
    }

    /// Return the `sys_platform` value for the target.
    fn sys_platform(self) -> &'static str {
        match self {
            Self::Windows | Self::X8664PcWindowsMsvc => "win32",
            Self::Aarch64PcWindowsMsvc => "win32",
            Self::Linux | Self::X8664UnknownLinuxGnu => "linux",
            Self::Macos | Self::Aarch64AppleDarwin => "darwin",
            Self::I686PcWindowsMsvc => "win32",
            Self::X8664AppleDarwin => "darwin",
            Self::Aarch64UnknownLinuxGnu => "linux",
            Self::Aarch64UnknownLinuxMusl => "linux",
            Self::X8664UnknownLinuxMusl => "linux",
            Self::Riscv64UnknownLinuxGnu => "linux",
            Self::S390XUnknownLinuxGnu => "linux",
            Self::Powerpc64LeUnknownLinuxGnu => "linux",
            Self::LoongArch64UnknownLinuxGnu => "linux",
            Self::X8664Manylinux2014 => "linux",
            Self::X8664Manylinux217 => "linux",
            Self::X8664Manylinux228 => "linux",
            Self::X8664Manylinux231 => "linux",
            Self::X8664Manylinux232 => "linux",
            Self::X8664Manylinux233 => "linux",
            Self::X8664Manylinux234 => "linux",
            Self::X8664Manylinux235 => "linux",
            Self::X8664Manylinux236 => "linux",
            Self::X8664Manylinux237 => "linux",
            Self::X8664Manylinux238 => "linux",
            Self::X8664Manylinux239 => "linux",
            Self::X8664Manylinux240 => "linux",
            Self::Aarch64Manylinux2014 => "linux",
            Self::Aarch64Manylinux217 => "linux",
            Self::Aarch64Manylinux228 => "linux",
            Self::Aarch64Manylinux231 => "linux",
            Self::Aarch64Manylinux232 => "linux",
            Self::Aarch64Manylinux233 => "linux",
            Self::Aarch64Manylinux234 => "linux",
            Self::Aarch64Manylinux235 => "linux",
            Self::Aarch64Manylinux236 => "linux",
            Self::Aarch64Manylinux237 => "linux",
            Self::Aarch64Manylinux238 => "linux",
            Self::Aarch64Manylinux239 => "linux",
            Self::Aarch64Manylinux240 => "linux",
            Self::S390XManylinux2014 => "linux",
            Self::S390XManylinux217 => "linux",
            Self::S390XManylinux228 => "linux",
            Self::S390XManylinux231 => "linux",
            Self::S390XManylinux232 => "linux",
            Self::S390XManylinux233 => "linux",
            Self::S390XManylinux234 => "linux",
            Self::S390XManylinux235 => "linux",
            Self::S390XManylinux236 => "linux",
            Self::S390XManylinux237 => "linux",
            Self::S390XManylinux238 => "linux",
            Self::S390XManylinux239 => "linux",
            Self::S390XManylinux240 => "linux",
            Self::Powerpc64LeManylinux2014 => "linux",
            Self::Powerpc64LeManylinux217 => "linux",
            Self::Powerpc64LeManylinux228 => "linux",
            Self::Powerpc64LeManylinux231 => "linux",
            Self::Powerpc64LeManylinux232 => "linux",
            Self::Powerpc64LeManylinux233 => "linux",
            Self::Powerpc64LeManylinux234 => "linux",
            Self::Powerpc64LeManylinux235 => "linux",
            Self::Powerpc64LeManylinux236 => "linux",
            Self::Powerpc64LeManylinux237 => "linux",
            Self::Powerpc64LeManylinux238 => "linux",
            Self::Powerpc64LeManylinux239 => "linux",
            Self::Powerpc64LeManylinux240 => "linux",
            Self::LoongArch64Manylinux236 => "linux",
            Self::LoongArch64Manylinux237 => "linux",
            Self::LoongArch64Manylinux238 => "linux",
            Self::LoongArch64Manylinux239 => "linux",
            Self::LoongArch64Manylinux240 => "linux",
            Self::Aarch64LinuxAndroid => "android",
            Self::X8664LinuxAndroid => "android",
            Self::Wasm32Pyodide2024 => "emscripten",
            Self::Wasm32Pyodide2025 => "emscripten",
            Self::Arm64Ios => "ios",
            Self::Arm64IosSimulator => "ios",
            Self::X8664IosSimulator => "ios",
        }
    }

    /// Return `true` if the platform is compatible with manylinux.
    pub fn manylinux_compatible(self) -> bool {
        match self {
            Self::Windows | Self::X8664PcWindowsMsvc => false,
            Self::Aarch64PcWindowsMsvc => false,
            Self::Linux | Self::X8664UnknownLinuxGnu => true,
            Self::Macos | Self::Aarch64AppleDarwin => false,
            Self::I686PcWindowsMsvc => false,
            Self::X8664AppleDarwin => false,
            Self::Aarch64UnknownLinuxGnu => true,
            Self::Aarch64UnknownLinuxMusl => true,
            Self::X8664UnknownLinuxMusl => true,
            Self::Riscv64UnknownLinuxGnu => true,
            Self::S390XUnknownLinuxGnu => true,
            Self::Powerpc64LeUnknownLinuxGnu => true,
            Self::LoongArch64UnknownLinuxGnu => true,
            Self::X8664Manylinux2014 => true,
            Self::X8664Manylinux217 => true,
            Self::X8664Manylinux228 => true,
            Self::X8664Manylinux231 => true,
            Self::X8664Manylinux232 => true,
            Self::X8664Manylinux233 => true,
            Self::X8664Manylinux234 => true,
            Self::X8664Manylinux235 => true,
            Self::X8664Manylinux236 => true,
            Self::X8664Manylinux237 => true,
            Self::X8664Manylinux238 => true,
            Self::X8664Manylinux239 => true,
            Self::X8664Manylinux240 => true,
            Self::Aarch64Manylinux2014 => true,
            Self::Aarch64Manylinux217 => true,
            Self::Aarch64Manylinux228 => true,
            Self::Aarch64Manylinux231 => true,
            Self::Aarch64Manylinux232 => true,
            Self::Aarch64Manylinux233 => true,
            Self::Aarch64Manylinux234 => true,
            Self::Aarch64Manylinux235 => true,
            Self::Aarch64Manylinux236 => true,
            Self::Aarch64Manylinux237 => true,
            Self::Aarch64Manylinux238 => true,
            Self::Aarch64Manylinux239 => true,
            Self::Aarch64Manylinux240 => true,
            Self::S390XManylinux2014 => true,
            Self::S390XManylinux217 => true,
            Self::S390XManylinux228 => true,
            Self::S390XManylinux231 => true,
            Self::S390XManylinux232 => true,
            Self::S390XManylinux233 => true,
            Self::S390XManylinux234 => true,
            Self::S390XManylinux235 => true,
            Self::S390XManylinux236 => true,
            Self::S390XManylinux237 => true,
            Self::S390XManylinux238 => true,
            Self::S390XManylinux239 => true,
            Self::S390XManylinux240 => true,
            Self::Powerpc64LeManylinux2014 => true,
            Self::Powerpc64LeManylinux217 => true,
            Self::Powerpc64LeManylinux228 => true,
            Self::Powerpc64LeManylinux231 => true,
            Self::Powerpc64LeManylinux232 => true,
            Self::Powerpc64LeManylinux233 => true,
            Self::Powerpc64LeManylinux234 => true,
            Self::Powerpc64LeManylinux235 => true,
            Self::Powerpc64LeManylinux236 => true,
            Self::Powerpc64LeManylinux237 => true,
            Self::Powerpc64LeManylinux238 => true,
            Self::Powerpc64LeManylinux239 => true,
            Self::Powerpc64LeManylinux240 => true,
            Self::LoongArch64Manylinux236 => true,
            Self::LoongArch64Manylinux237 => true,
            Self::LoongArch64Manylinux238 => true,
            Self::LoongArch64Manylinux239 => true,
            Self::LoongArch64Manylinux240 => true,
            Self::Aarch64LinuxAndroid => false,
            Self::X8664LinuxAndroid => false,
            Self::Wasm32Pyodide2024 => false,
            Self::Wasm32Pyodide2025 => false,
            Self::Arm64Ios => false,
            Self::Arm64IosSimulator => false,
            Self::X8664IosSimulator => false,
        }
    }

    /// Return a [`MarkerEnvironment`] compatible with the given [`TargetTriple`], based on
    /// a base [`MarkerEnvironment`].
    ///
    /// The returned [`MarkerEnvironment`] will preserve the base environment's Python version
    /// markers, but override its platform markers.
    pub fn markers(self, base: MarkerEnvironment) -> MarkerEnvironment {
        base.with_os_name(self.os_name())
            .with_platform_machine(self.platform_machine())
            .with_platform_system(self.platform_system())
            .with_sys_platform(self.sys_platform())
            .with_platform_release(self.platform_release())
            .with_platform_version(self.platform_version())
    }
}

/// Return the macOS deployment target as parsed from the environment.
fn macos_deployment_target() -> Option<(u16, u16)> {
    let version = std::env::var(EnvVars::MACOSX_DEPLOYMENT_TARGET).ok()?;
    let mut parts = version.split('.');

    // Parse the major version (e.g., `12` in `12.0`).
    let major = parts.next()?.parse::<u16>().ok()?;

    // Parse the minor version (e.g., `0` in `12.0`), with a default of `0`.
    let minor = parts.next().unwrap_or("0").parse::<u16>().ok()?;

    Some((major, minor))
}

/// Return the iOS deployment target as parsed from the environment.
fn ios_deployment_target() -> Option<(u16, u16)> {
    let version = std::env::var(EnvVars::IPHONEOS_DEPLOYMENT_TARGET).ok()?;
    let mut parts = version.split('.');

    // Parse the major version (e.g., `12` in `12.0`).
    let major = parts.next()?.parse::<u16>().ok()?;

    // Parse the minor version (e.g., `0` in `12.0`), with a default of `0`.
    let minor = parts.next().unwrap_or("0").parse::<u16>().ok()?;

    Some((major, minor))
}

/// Return the Android API level as parsed from the environment.
fn android_api_level() -> Option<u16> {
    let api_level_str = std::env::var(EnvVars::ANDROID_API_LEVEL).ok()?;

    // Parse the api level.
    let api_level = api_level_str.parse::<u16>().ok()?;

    Some(api_level)
}

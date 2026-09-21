//! This embeds a "manifest" - a special XML document - into the uv binary on Windows builds.
//!
//! It includes reasonable defaults for Windows binaries:
//! - `System` codepage to retain backwards compatibility with previous uv releases.
//!   We can set to the utf-8 codepage in a future breaking release which lets us use the
//!   *A versions of Windows API functions without utf-16 conversion.
//! - Long path awareness allows paths longer than 260 characters in Windows operations.
//!   This still requires `LongPathsEnabled` to be set in the Windows registry.
//! - Declared Windows 7-10+ compatibility to avoid legacy compat layers from being potentially
//!   applied by not specifying any. This does not imply actual Windows 7, 8.0, 8.1 support. In
//!   cases where the application is run on Windows 7, the app is treated as Windows 7 aware
//!   rather than an unspecified legacy application (e.g. Windows XP).
//! - Standard invoker execution levels for CLI applications to disable UAC virtualization.
//!
//! See <https://learn.microsoft.com/en-us/windows/win32/sbscs/application-manifests>
use embed_manifest::manifest::{ActiveCodePage, ExecutionLevel, Setting, SupportedOS};
use embed_manifest::{embed_manifest, empty_manifest};

use uv_static::EnvVars;

fn main() {
    if std::env::var_os(EnvVars::CARGO_CFG_WINDOWS).is_some() {
        let [major, minor, patch] = uv_version::version()
            .splitn(3, '.')
            .map(str::parse)
            .collect::<Result<Vec<u16>, _>>()
            .ok()
            .and_then(|v| v.try_into().ok())
            .expect("uv version must be in x.y.z format");
        let manifest = empty_manifest()
            .name("uv")
            .version(major, minor, patch, 0)
            .active_code_page(ActiveCodePage::System)
            // "Windows10" includes Windows 10 and 11, and Windows Server 2016, 2019 and 2022
            .supported_os(SupportedOS::Windows7..=SupportedOS::Windows10)
            .requested_execution_level(ExecutionLevel::AsInvoker)
            .long_path_aware(Setting::Enabled);
        embed_manifest(manifest).expect("unable to embed manifest");
    }
    println!("cargo:rerun-if-changed=build.rs");
}

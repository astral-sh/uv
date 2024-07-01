use anyhow::Context;
use owo_colors::OwoColorize;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_toolchain::managed::InstalledToolchains;
use uv_warnings::warn_user_once;

/// Show the toolchain directory.
pub(crate) fn dir(preview: PreviewMode) -> anyhow::Result<()> {
    if preview.is_disabled() {
        warn_user_once!("`uv toolchain dir` is experimental and may change without warning.");
    }
    let installed_toolchains =
        InstalledToolchains::from_settings().context("Failed to initialize toolchain settings")?;
    anstream::println!(
        "{}",
        installed_toolchains.root().simplified_display().cyan()
    );
    Ok(())
}

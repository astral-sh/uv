use anyhow::Context;
use owo_colors::OwoColorize;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_tool::InstalledTools;
use uv_warnings::warn_user_once;

/// Show the tool directory.
pub(crate) fn dir(preview: PreviewMode) -> anyhow::Result<()> {
    if preview.is_disabled() {
        warn_user_once!("`uv tool dir` is experimental and may change without warning.");
    }
    let installed_tools =
        InstalledTools::from_settings().context("Failed to initialize tools settings")?;
    anstream::println!("{}", installed_tools.root().simplified_display().cyan());
    Ok(())
}

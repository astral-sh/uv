use anstream::println;
use anyhow::Context;
use owo_colors::OwoColorize;

use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_tool::{find_executable_directory, InstalledTools};
use uv_warnings::warn_user_once;

/// Show the tool directory.
pub(crate) fn dir(bin: bool, preview: PreviewMode) -> anyhow::Result<()> {
    if preview.is_disabled() {
        warn_user_once!("`uv tool dir` is experimental and may change without warning");
    }

    if bin {
        let executable_directory = find_executable_directory()?;
        println!("{}", executable_directory.simplified_display().cyan());
    } else {
        let installed_tools =
            InstalledTools::from_settings().context("Failed to initialize tools settings")?;
        println!("{}", installed_tools.root().simplified_display().cyan());
    }

    Ok(())
}

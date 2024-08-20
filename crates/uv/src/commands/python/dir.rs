use anstream::println;
use anyhow::Context;
use owo_colors::OwoColorize;

use uv_fs::Simplified;
use uv_python::managed::ManagedPythonInstallations;

/// Show the toolchain directory.
pub(crate) fn dir() -> anyhow::Result<()> {
    let installed_toolchains = ManagedPythonInstallations::from_settings()
        .context("Failed to initialize toolchain settings")?;
    println!(
        "{}",
        installed_toolchains.root().simplified_display().cyan()
    );
    Ok(())
}

use anstream::println;
use anyhow::Context;
use owo_colors::OwoColorize;

use uv_fs::Simplified;
use uv_python::managed::{python_executable_dir, ManagedPythonInstallations};

/// Show the Python installation directory.
pub(crate) fn dir(bin: bool) -> anyhow::Result<()> {
    if bin {
        let bin = python_executable_dir()?;
        println!("{}", bin.simplified_display().cyan());
    } else {
        let installed_toolchains = ManagedPythonInstallations::from_settings(None)
            .context("Failed to initialize toolchain settings")?;
        println!(
            "{}",
            installed_toolchains.root().simplified_display().cyan()
        );
    }

    Ok(())
}

use std::fmt::{self, Write};
use std::path::PathBuf;

use owo_colors::OwoColorize;
use tracing::debug;
use uv_cache::Cache;
use uv_command_support::Printer;
use uv_fs::Simplified;
use uv_python_interpreter::PythonEnvironment;
use uv_python_managed::{ManagedPythonInstallation, PythonMinorVersionLink};
use uv_tool::InstalledTools;

/// Display a message about the target environment for the operation.
pub(crate) fn report_target_environment(
    env: &PythonEnvironment,
    cache: &Cache,
    printer: Printer,
) -> fmt::Result {
    // Resolve minor-version link directories (e.g., `cpython-3.12` → `cpython-3.12.12`).
    // On Windows, junction points aren't resolved by the interpreter's `sys.prefix`, so we
    // use the target directory from the minor-version link to display the actual installation.
    // This only applies to managed installations, not virtual environments.
    let root = if env.interpreter().is_virtualenv() {
        env.root().to_path_buf()
    } else {
        ManagedPythonInstallation::try_from_interpreter(env.interpreter())
            .and_then(|installation| PythonMinorVersionLink::from_installation(&installation))
            .map(|link| link.target_directory)
            .unwrap_or_else(|| env.root().to_path_buf())
    };

    let message = format!(
        "Using Python {} environment at: {}",
        env.interpreter().python_version(),
        root.user_display()
    );

    let Ok(target) = std::path::absolute(&root) else {
        debug!("{}", message);
        return Ok(());
    };

    // Do not report environments in the cache
    if target.starts_with(cache.root()) {
        debug!("{}", message);
        return Ok(());
    }

    // Do not report tool environments
    if let Ok(tools) = InstalledTools::from_settings() {
        if target.starts_with(tools.root()) {
            debug!("{}", message);
            return Ok(());
        }
    }

    // Do not report a default environment path
    if let Ok(default) = std::path::absolute(PathBuf::from(".venv")) {
        if target == default {
            debug!("{}", message);
            return Ok(());
        }
    }

    writeln!(printer.stderr(), "{}", message.dimmed())
}

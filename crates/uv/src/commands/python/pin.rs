use std::fmt::Write;
use std::path::PathBuf;

use anyhow::{bail, Result};

use tracing::debug;
use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_python::{
    requests_from_version_file, EnvironmentPreference, PythonInstallation, PythonPreference,
    PythonRequest, PYTHON_VERSION_FILENAME,
};
use uv_warnings::warn_user_once;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Pin to a specific Python version.
pub(crate) async fn pin(
    request: Option<String>,
    resolved: bool,
    python_preference: PythonPreference,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv python pin` is experimental and may change without warning.");
    }

    let Some(request) = request else {
        // Display the current pinned Python version
        if let Some(pins) = requests_from_version_file().await? {
            for pin in pins {
                writeln!(printer.stdout(), "{}", pin.to_canonical_string())?;
            }
            return Ok(ExitStatus::Success);
        }
        bail!("No pinned Python version found.")
    };
    let request = PythonRequest::parse(&request);

    let python = match PythonInstallation::find(
        &request,
        EnvironmentPreference::OnlySystem,
        python_preference,
        cache,
    ) {
        Ok(python) => Some(python),
        // If no matching Python version is found, don't fail unless `resolved` was requested
        Err(uv_python::Error::MissingPython(err)) if !resolved => {
            warn_user_once!("{}", err);
            None
        }
        Err(err) => return Err(err.into()),
    };

    let output = if resolved {
        // SAFETY: We exit early if Python is not found and resolved is `true`
        python
            .unwrap()
            .interpreter()
            .sys_executable()
            .user_display()
            .to_string()
    } else {
        request.to_canonical_string()
    };

    debug!("Using pin `{}`", output);
    let version_file = PathBuf::from(PYTHON_VERSION_FILENAME);
    let exists = version_file.exists();

    debug!("Writing pin to {}", version_file.user_display());
    fs_err::write(&version_file, format!("{output}\n"))?;
    if exists {
        writeln!(printer.stdout(), "Replaced existing pin with `{output}`")?;
    } else {
        writeln!(printer.stdout(), "Pinned to `{output}`")?;
    }

    Ok(ExitStatus::Success)
}

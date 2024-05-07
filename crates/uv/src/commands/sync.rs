use anyhow::Result;
use fs_err;

use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_interpreter::PythonEnvironment;
use uv_normalize::PackageName;
use uv_resolver::Lock;
use uv_warnings::warn_user;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Sync the project environment.
#[allow(clippy::unnecessary_wraps, clippy::too_many_arguments)]
pub(crate) async fn sync(
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv run` is experimental and may change without warning.");
    }

    // TODO(charlie): If the environment doesn't exist, create it.
    let venv = PythonEnvironment::from_virtualenv(&cache)?;
    let markers = venv.interpreter().markers();
    let tags = venv.interpreter().tags()?;

    // Read the lockfile.
    let resolution = {
        let root = PackageName::new(root.to_string())?;
        let encoded = fs_err::tokio::read_to_string("uv.lock").await?;
        let lock: Lock = toml::from_str(&encoded)?;
        lock.to_resolution(&markers, &tags, &root)
    };

    // Sync the environment.

    Ok(ExitStatus::Success)
}

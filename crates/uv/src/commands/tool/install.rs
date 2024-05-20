
use anyhow::Result;
use itertools::Itertools;
use tracing::debug;

use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_interpreter::PythonEnvironment;
use uv_requirements::RequirementsSource;
use uv_warnings::warn_user;
use uv_workspace::Workspace;

use crate::commands::update_environment;
use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Run a command.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn install(
    target: String,
    python: Option<String>,
    _isolated: bool,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user!("`uv tool run` is experimental and may change without warning.");
    }

    // TODO(zanieb): Allow users to pass an explicit package name different than the target
    // as well as additional requirements
    let requirements = [RequirementsSource::from_package(target.clone())];

    // TODO(zanieb): When implementing project-level tools, discover the project and check if it has the tool
    // TOOD(zanieb): Determine if we sould layer on top of the project environment if it is present

    // If necessary, create an environment for the ephemeral requirements.
    debug!("Syncing ephemeral environment.");

    // Discover an interpreter.
    let interpreter = if let Some(python) = python.as_ref() {
        PythonEnvironment::from_requested_python(python, cache)?.into_interpreter()
    } else {
        PythonEnvironment::from_default_python(cache)?.into_interpreter()
    };

    // Create a virtual environment
    let env_path = cache.bucket(uv_cache::CacheBucket::Environments).join(target);

    let venv = if !env_path.try_exists()? {
        fs_err::create_dir_all(&env_path)?;

        uv_virtualenv::create_venv(
            &env_path,
            interpreter,
            uv_virtualenv::Prompt::None,
            false,
            false,
        )?
    } else {
        PythonEnvironment::from_root(&env_path, cache)?
    };

    // Install the requirements.
    update_environment(venv, &requirements, preview, cache, printer).await?;

    Ok(ExitStatus::Success)
}

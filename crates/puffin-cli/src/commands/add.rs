use anyhow::Result;
use tracing::info;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Add a dependency to the workspace.
pub(crate) fn add(name: &str, _printer: Printer) -> Result<ExitStatus> {
    // Locate the workspace.
    let Some(workspace_root) = puffin_workspace::find_pyproject_toml(std::env::current_dir()?)
    else {
        return Err(anyhow::anyhow!(
            "Could not find a `pyproject.toml` file in the current directory or any of its parents"
        ));
    };

    info!("Found workspace at: {}", workspace_root.display());

    // Parse the manifest.
    let mut manifest = puffin_workspace::Workspace::try_from(workspace_root.as_path())?;

    // Add the dependency.
    manifest.add_dependency(name)?;

    // Write the manifest back to disk.
    manifest.save(&workspace_root)?;

    Ok(ExitStatus::Success)
}

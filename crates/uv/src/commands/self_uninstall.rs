use anyhow::Result;

use crate::commands::cache_clean;
use crate::commands::ExitStatus;
use crate::printer::Printer;
use std::env;
use std::fs;
use std::path::Path;
use uv_cache::Cache;
use uv_python::managed::ManagedPythonInstallations;
use uv_tool::InstalledTools;

pub(crate) fn self_uninstall(
    cache: &Cache,
    printer: Printer,
    clean_stored_data: bool,
) -> Result<ExitStatus> {
    if clean_stored_data {
        // uv cache clean
        cache_clean(&[], &cache, printer)?;

        // rm -r "$(uv python dir)"
        let installed_toolchains = ManagedPythonInstallations::from_settings(None)?;
        let python_directory = installed_toolchains.root();
        fs::remove_dir_all(python_directory)?;

        // rm -r "$(uv tool dir)"
        let installed_tools = InstalledTools::from_settings()?;
        let tools_path = installed_tools.root();
        fs::remove_dir_all(tools_path)?;
    }

    // Remove uv and uvx binaries
    // rm ~/.local/bin/uv ~/.local/bin/uvx
    let home_dir = env::var("HOME").unwrap();
    let home_path = Path::new(&home_dir);

    let uv_path = home_path.join(".local").join("bin").join("uv.exe");
    let uvx_path = home_path.join(".local").join("bin").join("uvx.exe");
    fs::remove_file(uv_path)?;
    fs::remove_file(uvx_path)?;

    Ok(ExitStatus::Success)
}

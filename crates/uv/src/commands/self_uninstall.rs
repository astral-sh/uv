use anyhow::Result;

use crate::commands::cache_clean;
use crate::commands::ExitStatus;
use crate::printer::Printer;
use std::env;
use std::path::Path;
use uv_cache::rm_rf;
use uv_cache::Cache;
use uv_python::managed::ManagedPythonInstallations;
use uv_tool::InstalledTools;

pub(crate) fn self_uninstall(
    cache: &Cache,
    printer: Printer,
    remove_data: bool,
) -> Result<ExitStatus> {
    if remove_data {
        // uv cache clean
        cache_clean(&[], &cache, printer)?;

        // rm -r "$(uv python dir)"
        let installed_toolchains = ManagedPythonInstallations::from_settings(None)?;
        let python_directory = installed_toolchains.root();
        rm_rf(python_directory)?;

        // rm -r "$(uv tool dir)"
        let installed_tools = InstalledTools::from_settings()?;
        let tools_path = installed_tools.root();
        rm_rf(tools_path)?;
    }

    // Remove uv and uvx binaries
    // rm ~/.local/bin/uv ~/.local/bin/uvx
    let home_dir = env::var("HOME").unwrap();
    let home_path = Path::new(&home_dir);

    let uv_executable = format!("uv{}", std::env::consts::EXE_SUFFIX);
    let uvx_executable = format!("uvx{}", std::env::consts::EXE_SUFFIX);

    let uv_path = home_path.join(".local").join("bin").join(uv_executable);
    let uvx_path = home_path.join(".local").join("bin").join(uvx_executable);
    rm_rf(uv_path)?;
    rm_rf(uvx_path)?;

    Ok(ExitStatus::Success)
}

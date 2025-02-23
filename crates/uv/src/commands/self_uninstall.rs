use anyhow::Result;

use crate::commands::cache_clean;
use crate::commands::ExitStatus;
use crate::printer::Printer;
use std::env;
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
        cache_clean(&[], cache, printer)?;

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

    let uv_executable = env::current_exe().unwrap();
    let uv_path = uv_executable.as_path();
    // We assume uvx executable is in same directory as uv executable
    let uvx_executable = uv_path.with_file_name(format!("uvx{}", std::env::consts::EXE_SUFFIX));
    let uvx_path = uvx_executable.as_path();

    rm_rf(uv_path)?;
    rm_rf(uvx_path)?;

    Ok(ExitStatus::Success)
}

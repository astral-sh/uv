use std::fmt::Write;
use std::str::FromStr;

use anyhow::Result;

use pep508_rs::PackageName;
use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_installer::SitePackages;
use uv_tool::InstalledTools;
use uv_toolchain::PythonEnvironment;
use uv_warnings::warn_user_once;

use crate::commands::ExitStatus;
use crate::printer::Printer;

fn get_tool_version(
    name: &str,
    installed_tools: &InstalledTools,
) -> Result<String, Box<dyn std::error::Error>> {
    let tool_path = installed_tools.root().join(name);
    let cache = Cache::from_path(installed_tools.root());
    let env = PythonEnvironment::from_root(tool_path, &cache)
        .map_err(|_| "Failed to get python environment")?;
    let name_ = PackageName::from_str(name).map_err(|_| "Failed to convert name to PackageName")?;
    let packages = SitePackages::from_environment(&env)
        .map_err(|_| "Failed to get site packages from environment")?;
    let package = packages.get_packages(&name_);
    let dist_version = package.first().ok_or("No packages found")?.version();
    let version = dist_version.to_string();
    Ok(version)
}
/// List installed tools.
pub(crate) async fn list(preview: PreviewMode, printer: Printer) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv tool list` is experimental and may change without warning.");
    }

    let installed_tools = InstalledTools::from_settings()?;

    let mut tools = installed_tools.tools()?.into_iter().collect::<Vec<_>>();
    tools.sort_by_key(|(name, _)| name.clone());

    if tools.is_empty() {
        writeln!(printer.stderr(), "No tools installed")?;
        return Ok(ExitStatus::Success);
    }

    // TODO(zanieb): Track and display additional metadata, like entry points
    for (name, _tool) in tools {
        match get_tool_version(&name, &installed_tools) {
            Ok(version) => {
                writeln!(printer.stdout(), "{name} v{version}")?;
            },
            Err(e) => {
                writeln!(printer.stderr(), "Failed to get version for {name}: {e}")?;
            },
        }
    }

    Ok(ExitStatus::Success)
}

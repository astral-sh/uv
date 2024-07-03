use std::fmt::Write;

use anyhow::Result;

use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_tool::InstalledTools;
use uv_warnings::warn_user_once;

use crate::commands::ExitStatus;
use crate::printer::Printer;

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

    for (name, tool) in tools {
        // Output tool name and version
        let version =
            match installed_tools.version(&name, &Cache::from_path(installed_tools.root())) {
                Ok(version) => version,
                Err(e) => {
                    writeln!(printer.stderr(), "{e}")?;
                    continue;
                }
            };

        writeln!(printer.stdout(), "{name} v{version}")?;

        // Output tool entrypoints
        for entrypoint in tool.entrypoints() {
            writeln!(printer.stdout(), "    {}", &entrypoint.name)?;
        }
    }

    Ok(ExitStatus::Success)
}

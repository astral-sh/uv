use std::fmt::Write;

use anyhow::Result;
use itertools::Itertools;
use owo_colors::OwoColorize;

use uv_cache::Cache;
use uv_fs::Simplified;
use uv_tool::InstalledTools;
use uv_warnings::warn_user;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// List installed tools.
pub(crate) async fn list(
    show_paths: bool,
    show_version_specifiers: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let installed_tools = InstalledTools::from_settings()?;
    let _lock = match installed_tools.lock().await {
        Ok(lock) => lock,
        Err(uv_tool::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
            writeln!(printer.stderr(), "No tools installed")?;
            return Ok(ExitStatus::Success);
        }
        Err(err) => return Err(err.into()),
    };

    let mut tools = installed_tools.tools()?.into_iter().collect::<Vec<_>>();
    tools.sort_by_key(|(name, _)| name.clone());

    if tools.is_empty() {
        writeln!(printer.stderr(), "No tools installed")?;
        return Ok(ExitStatus::Success);
    }

    for (name, tool) in tools {
        // Skip invalid tools
        let Ok(tool) = tool else {
            warn_user!(
                "Ignoring malformed tool `{name}` (run `{}` to remove)",
                format!("uv tool uninstall {name}").green()
            );
            continue;
        };

        // Output tool name and version
        let version = match installed_tools.version(&name, cache) {
            Ok(version) => version,
            Err(e) => {
                writeln!(printer.stderr(), "{e}")?;
                continue;
            }
        };

        let version_specifier = if show_version_specifiers {
            let specifiers = tool
                .requirements()
                .iter()
                .filter(|req| req.name == name)
                .map(|req| req.source.to_string())
                .filter(|s| !s.is_empty())
                .join(", ");
            format!(" [required: {specifiers}]")
        } else {
            String::new()
        };

        if show_paths {
            writeln!(
                printer.stdout(),
                "{} ({})",
                format!("{name} v{version}{version_specifier}").bold(),
                installed_tools.tool_dir(&name).simplified_display().cyan(),
            )?;
        } else {
            writeln!(
                printer.stdout(),
                "{}",
                format!("{name} v{version}{version_specifier}").bold()
            )?;
        }

        // Output tool entrypoints
        for entrypoint in tool.entrypoints() {
            if show_paths {
                writeln!(
                    printer.stdout(),
                    "- {} ({})",
                    entrypoint.name,
                    entrypoint.install_path.simplified_display().cyan()
                )?;
            } else {
                writeln!(printer.stdout(), "- {}", entrypoint.name)?;
            }
        }
    }

    Ok(ExitStatus::Success)
}

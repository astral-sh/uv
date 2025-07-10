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
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn list(
    show_paths: bool,
    show_version_specifiers: bool,
    show_with: bool,
    show_extras: bool,
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
                if let uv_tool::Error::EnvironmentError(e) = e {
                    warn_user!(
                        "{e} (run `{}` to reinstall)",
                        format!("uv tool install {name} --reinstall").green()
                    );
                } else {
                    writeln!(printer.stderr(), "{e}")?;
                }
                continue;
            }
        };

        let version_specifier = show_version_specifiers
            .then(|| {
                tool.requirements()
                    .iter()
                    .filter(|req| req.name == name)
                    .map(|req| req.source.to_string())
                    .filter(|s| !s.is_empty())
                    .peekable()
            })
            .take_if(|specifiers| specifiers.peek().is_some())
            .map(|mut specifiers| {
                let specifiers = specifiers.join(", ");
                format!(" [required: {specifiers}]")
            })
            .unwrap_or_default();

        let extra_requirements = show_extras
            .then(|| {
                tool.requirements()
                    .iter()
                    .filter(|req| req.name == name)
                    .flat_map(|req| req.extras.iter()) // Flatten the extras from all matching requirements
                    .peekable()
            })
            .take_if(|extras| extras.peek().is_some())
            .map(|extras| {
                let extras_str = extras.map(ToString::to_string).join(", ");
                format!(" [extras: {extras_str}]")
            })
            .unwrap_or_default();

        let with_requirements = show_with
            .then(|| {
                tool.requirements()
                    .iter()
                    .filter(|req| req.name != name)
                    .peekable()
            })
            .take_if(|requirements| requirements.peek().is_some())
            .map(|requirements| {
                let requirements = requirements
                    .map(|req| format!("{}{}", req.name, req.source))
                    .join(", ");
                format!(" [with: {requirements}]")
            })
            .unwrap_or_default();

        if show_paths {
            writeln!(
                printer.stdout(),
                "{} ({})",
                format!(
                    "{name} v{version}{version_specifier}{extra_requirements}{with_requirements}"
                )
                .bold(),
                installed_tools.tool_dir(&name).simplified_display().cyan(),
            )?;
        } else {
            writeln!(
                printer.stdout(),
                "{}",
                format!(
                    "{name} v{version}{version_specifier}{extra_requirements}{with_requirements}"
                )
                .bold()
            )?;
        }

        // Output tool entrypoints
        for entrypoint in tool.entrypoints() {
            if show_paths {
                writeln!(printer.stdout(), "- {}", entrypoint.to_string().cyan())?;
            } else {
                writeln!(printer.stdout(), "- {}", entrypoint.name)?;
            }
        }
    }

    Ok(ExitStatus::Success)
}

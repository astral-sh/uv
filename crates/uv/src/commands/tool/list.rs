use std::fmt::Write;

use anyhow::Result;
use itertools::Itertools;
use owo_colors::OwoColorize;

use serde::Serialize;
use uv_cache::Cache;
use uv_cli::ToolListFormat;
use uv_fs::Simplified;
use uv_pep440::Version;
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
    output_format: ToolListFormat,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    match output_format {
        ToolListFormat::Text => {
            list_text(
                show_paths,
                show_version_specifiers,
                show_with,
                show_extras,
                cache,
                printer,
            )
            .await
        }
        ToolListFormat::Json => list_json(cache, printer).await,
    }
}

#[allow(clippy::fn_params_excessive_bools)]
async fn list_text(
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

#[derive(Serialize)]
#[serde(untagged)]
enum ToolListEntry {
    Tool {
        name: String,
        version: Version,
        version_specifiers: Vec<String>,
        extra_requirements: Vec<String>,
        with_requirements: Vec<String>,
        directory: String,
        environment: EnvironmentInfo,
        entrypoints: Vec<Entrypoint>,
    },
    MalformedTool {
        name: String,
    },
    Error {
        name: String,
        error: String,
    },
}

#[derive(Serialize)]
#[serde(untagged)]
enum EnvironmentInfo {
    Environment { python: String, version: Version },
    NoEnvironment,
    Error { error: String },
}

#[derive(Serialize)]
struct Entrypoint {
    name: String,
    path: String,
}

async fn list_json(cache: &Cache, printer: Printer) -> Result<ExitStatus> {
    let installed_tools = InstalledTools::from_settings()?;

    match installed_tools.lock().await {
        Ok(_lock) => (),
        Err(uv_tool::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
            writeln!(printer.stdout(), "[]")?;
            return Ok(ExitStatus::Success);
        }
        Err(err) => return Err(err.into()),
    }

    let tools = installed_tools.tools()?;

    if tools.is_empty() {
        writeln!(printer.stdout(), "[]")?;
        return Ok(ExitStatus::Success);
    }

    let tool_list = tools
        .into_iter()
        .sorted_by_cached_key(|(name, _)| name.clone())
        .map(|(name, tool)| match tool {
            Err(_) => ToolListEntry::MalformedTool {
                name: name.to_string(),
            },
            Ok(tool) => {
                let version = match installed_tools.version(&name, cache) {
                    Ok(version) => version,
                    Err(error) => {
                        return ToolListEntry::Error {
                            name: name.to_string(),
                            error: error.to_string(),
                        };
                    }
                };

                let mut version_specifiers = vec![];
                let mut extra_requirements = vec![];
                let mut with_requirements = vec![];

                tool.requirements().iter().for_each(|req| {
                    if req.name == name {
                        let specifier = req.source.to_string();

                        if !specifier.is_empty() {
                            version_specifiers.push(specifier);
                        }

                        for extra in &req.extras {
                            extra_requirements.push(extra.to_string());
                        }
                    } else {
                        with_requirements.push(format!("{}{}", req.name, req.source));
                    }
                });

                let directory = installed_tools.tool_dir(&name).display().to_string();
                let environment = match installed_tools.get_environment(&name, cache) {
                    Ok(None) => EnvironmentInfo::NoEnvironment,
                    Err(error) => EnvironmentInfo::Error {
                        error: error.to_string(),
                    },
                    Ok(Some(environment)) => {
                        let python_executable = environment.python_executable();
                        let interpreter = environment.interpreter();

                        EnvironmentInfo::Environment {
                            python: python_executable.display().to_string(),
                            version: interpreter.python_version().clone(),
                        }
                    }
                };

                let entrypoints = tool
                    .entrypoints()
                    .iter()
                    .map(|entrypoint| Entrypoint {
                        name: entrypoint.name.to_string(),
                        path: entrypoint.install_path.display().to_string(),
                    })
                    .collect::<Vec<_>>();

                ToolListEntry::Tool {
                    name: name.to_string(),
                    version,
                    version_specifiers,
                    extra_requirements,
                    with_requirements,
                    directory,
                    environment,
                    entrypoints,
                }
            }
        })
        .collect::<Vec<_>>();

    writeln!(printer.stdout(), "{}", serde_json::to_string(&tool_list)?)?;
    Ok(ExitStatus::Success)
}

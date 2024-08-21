use std::fmt::Write;

use anyhow::{bail, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_tool::{InstalledTools, Tool, ToolEntrypoint};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Uninstall a tool.
pub(crate) async fn uninstall(name: Option<PackageName>, printer: Printer) -> Result<ExitStatus> {
    let installed_tools = InstalledTools::from_settings()?.init()?;
    let _lock = match installed_tools.acquire_lock() {
        Ok(lock) => lock,
        Err(uv_tool::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
            if let Some(name) = name {
                bail!("`{name}` is not installed");
            }
            writeln!(printer.stderr(), "Nothing to uninstall")?;
            return Ok(ExitStatus::Success);
        }
        Err(err) => return Err(err.into()),
    };

    // Perform the uninstallation.
    do_uninstall(&installed_tools, name, printer).await?;

    // Clean up any empty directories.
    if uv_fs::directories(installed_tools.root()).all(|path| uv_fs::is_temporary(&path)) {
        fs_err::tokio::remove_dir_all(&installed_tools.root()).await?;
        if let Some(top_level) = installed_tools.root().parent() {
            if uv_fs::directories(top_level).all(|path| uv_fs::is_temporary(&path)) {
                fs_err::tokio::remove_dir_all(top_level).await?;
            }
        }
    }

    Ok(ExitStatus::Success)
}

/// Perform the uninstallation.
async fn do_uninstall(
    installed_tools: &InstalledTools,
    name: Option<PackageName>,
    printer: Printer,
) -> Result<()> {
    let mut dangling = false;
    let mut entrypoints = if let Some(name) = name {
        let Some(receipt) = installed_tools.get_tool_receipt(&name)? else {
            // If the tool is not installed, attempt to remove the environment anyway.
            match installed_tools.remove_environment(&name) {
                Ok(()) => {
                    writeln!(
                        printer.stderr(),
                        "Removed dangling environment for `{name}`"
                    )?;
                    return Ok(());
                }
                Err(uv_tool::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
                    bail!("`{name}` is not installed");
                }
                Err(err) => {
                    return Err(err.into());
                }
            }
        };

        uninstall_tool(&name, &receipt, installed_tools).await?
    } else {
        let mut entrypoints = vec![];
        for (name, receipt) in installed_tools.tools()? {
            let Ok(receipt) = receipt else {
                // If the tool is not installed properly, attempt to remove the environment anyway.
                match installed_tools.remove_environment(&name) {
                    Ok(()) => {
                        dangling = true;
                        writeln!(
                            printer.stderr(),
                            "Removed dangling environment for `{name}`"
                        )?;
                        continue;
                    }
                    Err(uv_tool::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
                        bail!("`{name}` is not installed");
                    }
                    Err(err) => {
                        return Err(err.into());
                    }
                }
            };

            entrypoints.extend(uninstall_tool(&name, &receipt, installed_tools).await?);
        }
        entrypoints
    };
    entrypoints.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    if entrypoints.is_empty() {
        // If we removed at least one dangling environment, there's no need to summarize.
        if !dangling {
            writeln!(printer.stderr(), "Nothing to uninstall")?;
        }
        return Ok(());
    }

    let s = if entrypoints.len() == 1 { "" } else { "s" };
    writeln!(
        printer.stderr(),
        "Uninstalled {} executable{s}: {}",
        entrypoints.len(),
        entrypoints
            .iter()
            .map(|entrypoint| entrypoint.name.bold())
            .join(", ")
    )?;

    Ok(())
}

/// Uninstall a tool.
async fn uninstall_tool(
    name: &PackageName,
    receipt: &Tool,
    tools: &InstalledTools,
) -> Result<Vec<ToolEntrypoint>> {
    // Remove the tool itself.
    tools.remove_environment(name)?;

    // Remove the tool's entrypoints.
    let entrypoints = receipt.entrypoints();
    for entrypoint in entrypoints {
        debug!(
            "Removing executable: {}",
            entrypoint.install_path.user_display()
        );
        match fs_err::tokio::remove_file(&entrypoint.install_path).await {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                debug!(
                    "Executable not found: {}",
                    entrypoint.install_path.user_display()
                );
            }
            Err(err) => {
                return Err(err.into());
            }
        }
    }

    Ok(entrypoints.to_vec())
}

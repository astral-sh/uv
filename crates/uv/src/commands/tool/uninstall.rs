use std::fmt::Write;
use std::path::Path;

use anyhow::{bail, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_tool::{InstalledTools, Tool, ToolEntrypoint, ToolManpage};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Uninstall a tool.
pub(crate) async fn uninstall(name: Vec<PackageName>, printer: Printer) -> Result<ExitStatus> {
    let installed_tools = InstalledTools::from_settings()?.init()?;
    let _lock = match installed_tools.lock().await {
        Ok(lock) => lock,
        Err(uv_tool::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
            if !name.is_empty() {
                for name in name {
                    writeln!(printer.stderr(), "`{name}` is not installed")?;
                }
                return Ok(ExitStatus::Success);
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
        fs_err::tokio::remove_dir_all(&installed_tools.root())
            .await
            .ignore_currently_being_deleted()?;
        if let Some(top_level) = installed_tools.root().parent() {
            if uv_fs::directories(top_level).all(|path| uv_fs::is_temporary(&path)) {
                fs_err::tokio::remove_dir_all(top_level)
                    .await
                    .ignore_currently_being_deleted()?;
            }
        }
    }

    Ok(ExitStatus::Success)
}

trait IoErrorExt: std::error::Error + 'static {
    #[inline]
    fn is_in_process_of_being_deleted(&self) -> bool {
        if cfg!(target_os = "windows") {
            use std::error::Error;
            let mut e: &dyn Error = &self;
            loop {
                if e.to_string().contains("The file cannot be opened because it is in the process of being deleted. (os error 303)") {
                    return true;
                }
                e = match e.source() {
                    Some(e) => e,
                    None => break,
                }
            }
        }

        false
    }
}

impl IoErrorExt for std::io::Error {}

/// An extension trait to suppress "cannot open file because it's currently being deleted"
trait IgnoreCurrentlyBeingDeleted {
    fn ignore_currently_being_deleted(self) -> Self;
}

impl IgnoreCurrentlyBeingDeleted for Result<(), std::io::Error> {
    fn ignore_currently_being_deleted(self) -> Self {
        match self {
            Ok(()) => Ok(()),
            Err(err) if err.is_in_process_of_being_deleted() => Ok(()),
            Err(err) => Err(err),
        }
    }
}

/// Perform the uninstallation.
async fn do_uninstall(
    installed_tools: &InstalledTools,
    names: Vec<PackageName>,
    printer: Printer,
) -> Result<()> {
    let mut dangling = false;
    let (mut entrypoints, mut manpages) = if names.is_empty() {
        let mut entrypoints = vec![];
        let mut manpages = vec![];
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

            let (removed_entrypoitns, removed_manpages) =
                uninstall_tool(&name, &receipt, installed_tools).await?;
            entrypoints.extend(removed_entrypoitns);
            manpages.extend(removed_manpages);
        }
        (entrypoints, manpages)
    } else {
        let mut entrypoints = vec![];
        let mut manpages = vec![];
        for name in names {
            let Some(receipt) = installed_tools.get_tool_receipt(&name)? else {
                // If the tool is not installed properly, attempt to remove the environment anyway.
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

            let (removed_entrypoints, removed_manpages) =
                uninstall_tool(&name, &receipt, installed_tools).await?;
            entrypoints.extend(removed_entrypoints);
            manpages.extend(removed_manpages);
        }
        (entrypoints, manpages)
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

    if !manpages.is_empty() {
        manpages.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        let s = if manpages.len() == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "Uninstalled {} manpage{s}: {}",
            manpages.len(),
            manpages
                .iter()
                .map(|manpage| manpage.name.bold())
                .join(", ")
        )?;
    }

    Ok(())
}

/// Uninstall a tool.
async fn uninstall_tool(
    name: &PackageName,
    receipt: &Tool,
    tools: &InstalledTools,
) -> Result<(Vec<ToolEntrypoint>, Vec<ToolManpage>)> {
    // Remove the tool itself.
    tools.remove_environment(name)?;

    // Remove the tool's entrypoints.
    let entrypoints = receipt.entrypoints();
    for entrypoint in entrypoints {
        remove_resource(&entrypoint.install_path, "executable").await?;
    }

    // Remove the tool's manpages.
    let manpages = receipt.manpages();
    for manpage in manpages {
        remove_resource(&manpage.install_path, "manpage").await?;
    }

    Ok((entrypoints.to_vec(), manpages.to_vec()))
}

async fn remove_resource(target: &Path, resource_type: &str) -> Result<()> {
    debug!("Removing {resource_type}: {}", target.user_display());
    match fs_err::tokio::remove_file(target).await {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            debug!(
                "{}{} not found: {}",
                &resource_type[0..1].to_uppercase(),
                &resource_type[1..],
                target.user_display()
            );
        }
        Err(err) => {
            return Err(err.into());
        }
    };
    Ok(())
}

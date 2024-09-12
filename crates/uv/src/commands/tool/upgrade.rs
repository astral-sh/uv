use std::{collections::BTreeSet, fmt::Write};

use anyhow::Result;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::Concurrency;
use uv_normalize::PackageName;
use uv_requirements::RequirementsSpecification;
use uv_settings::{Combine, ResolverInstallerOptions, ToolOptions};
use uv_tool::InstalledTools;

use crate::commands::pip::loggers::{SummaryResolveLogger, UpgradeInstallLogger};
use crate::commands::pip::operations::Changelog;
use crate::commands::project::{update_environment, EnvironmentUpdate};
use crate::commands::tool::common::remove_entrypoints;
use crate::commands::{tool::common::install_executables, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Upgrade a tool.
pub(crate) async fn upgrade(
    name: Vec<PackageName>,
    connectivity: Connectivity,
    args: ResolverInstallerOptions,
    filesystem: ResolverInstallerOptions,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let installed_tools = InstalledTools::from_settings()?.init()?;
    let _lock = installed_tools.lock().await?;

    let names: BTreeSet<PackageName> = {
        if name.is_empty() {
            installed_tools
                .tools()
                .unwrap_or_default()
                .into_iter()
                .map(|(name, _)| name)
                .collect()
        } else {
            name.into_iter().collect()
        }
    };

    if names.is_empty() {
        writeln!(printer.stderr(), "Nothing to upgrade")?;
        return Ok(ExitStatus::Success);
    }

    // Determine whether we applied any upgrades.
    let mut did_upgrade = false;

    // Determine whether any tool upgrade failed.
    let mut failed_upgrade = false;

    for name in &names {
        debug!("Upgrading tool: `{name}`");
        let changelog = upgrade_tool(
            name,
            printer,
            &installed_tools,
            &args,
            cache,
            &filesystem,
            connectivity,
            concurrency,
            native_tls,
        )
        .await;

        match changelog {
            Ok(changelog) => {
                did_upgrade |= !changelog.is_empty();
            }
            Err(err) => {
                // If we have a single tool, return the error directly.
                if names.len() > 1 {
                    writeln!(
                        printer.stderr(),
                        "Failed to upgrade `{}`: {err}",
                        name.cyan(),
                    )?;
                } else {
                    writeln!(printer.stderr(), "{err}")?;
                }
                failed_upgrade = true;
            }
        }
    }

    if failed_upgrade {
        return Ok(ExitStatus::Failure);
    }

    if !did_upgrade {
        writeln!(printer.stderr(), "Nothing to upgrade")?;
    }

    Ok(ExitStatus::Success)
}

async fn upgrade_tool(
    name: &PackageName,
    printer: Printer,
    installed_tools: &InstalledTools,
    args: &ResolverInstallerOptions,
    cache: &Cache,
    filesystem: &ResolverInstallerOptions,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
) -> Result<Changelog> {
    // Ensure the tool is installed.
    let existing_tool_receipt = match installed_tools.get_tool_receipt(name) {
        Ok(Some(receipt)) => receipt,
        Ok(None) => {
            let install_command = format!("uv tool install {name}");
            return Err(anyhow::anyhow!(
                "`{}` is not installed; run `{}` to install",
                name.cyan(),
                install_command.green()
            ));
        }
        Err(_) => {
            let install_command = format!("uv tool install --force {name}");
            return Err(anyhow::anyhow!(
                "`{}` is missing a valid receipt; run `{}` to reinstall",
                name.cyan(),
                install_command.green()
            ));
        }
    };

    let existing_environment = match installed_tools.get_environment(name, cache) {
        Ok(Some(environment)) => environment,
        Ok(None) => {
            let install_command = format!("uv tool install {name}");
            return Err(anyhow::anyhow!(
                "`{}` is not installed; run `{}` to install",
                name.cyan(),
                install_command.green()
            ));
        }
        Err(_) => {
            let install_command = format!("uv tool install --force {name}");
            return Err(anyhow::anyhow!(
                "`{}` is missing a valid environment; run `{}` to reinstall",
                name.cyan(),
                install_command.green()
            ));
        }
    };

    // Resolve the appropriate settings, preferring: CLI > receipt > user.
    let options = args.clone().combine(
        ResolverInstallerOptions::from(existing_tool_receipt.options().clone())
            .combine(filesystem.clone()),
    );
    let settings = ResolverInstallerSettings::from(options.clone());

    // Resolve the requirements.
    let requirements = existing_tool_receipt.requirements();
    let spec = RequirementsSpecification::from_requirements(requirements.to_vec());

    // Initialize any shared state.
    let state = SharedState::default();

    // TODO(zanieb): Build the environment in the cache directory then copy into the tool
    // directory.
    let EnvironmentUpdate {
        environment,
        changelog,
    } = update_environment(
        existing_environment,
        spec,
        &settings,
        &state,
        Box::new(SummaryResolveLogger),
        Box::new(UpgradeInstallLogger::new(name.clone())),
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await?;

    // If we modified the target tool, reinstall the entrypoints.
    if changelog.includes(name) {
        // At this point, we updated the existing environment, so we should remove any of its
        // existing executables.
        remove_entrypoints(&existing_tool_receipt);

        install_executables(
            &environment,
            name,
            installed_tools,
            ToolOptions::from(options),
            true,
            existing_tool_receipt.python().to_owned(),
            requirements.to_vec(),
            printer,
        )?;
    }

    Ok(changelog)
}

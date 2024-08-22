use std::{collections::BTreeSet, fmt::Write};

use anyhow::Result;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::Concurrency;
use uv_normalize::PackageName;
use uv_python::{
    EnvironmentPreference, PythonDownloads, PythonInstallation, PythonPreference, PythonRequest,
};
use uv_requirements::RequirementsSpecification;
use uv_settings::{Combine, ResolverInstallerOptions, ToolOptions};
use uv_tool::InstalledTools;

use crate::commands::pip::loggers::{SummaryResolveLogger, UpgradeInstallLogger};
use crate::commands::project::{update_environment, EnvironmentUpdate};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::tool::common::remove_entrypoints;
use crate::commands::{tool::common::install_executables, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Upgrade a tool.
pub(crate) async fn upgrade(
    name: Option<PackageName>,
    args: ResolverInstallerOptions,
    python: Option<String>,
    filesystem: ResolverInstallerOptions,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Initialize any shared state.
    let state = SharedState::default();

    let python_request = python.as_deref().map(PythonRequest::parse);

    let installed_tools = InstalledTools::from_settings()?.init()?;
    let _lock = installed_tools.acquire_lock()?;

    let names: BTreeSet<PackageName> =
        name.map(|name| BTreeSet::from_iter([name]))
            .unwrap_or_else(|| {
                installed_tools
                    .tools()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(name, _)| name)
                    .collect()
            });

    if names.is_empty() {
        writeln!(printer.stderr(), "Nothing to upgrade")?;
        return Ok(ExitStatus::Success);
    }

    // Determine whether we applied any upgrades.
    let mut did_upgrade = false;

    for name in names {
        debug!("Upgrading tool: `{name}`");

        // Ensure the tool is installed.
        let existing_tool_receipt = match installed_tools.get_tool_receipt(&name) {
            Ok(Some(receipt)) => receipt,
            Ok(None) => {
                let install_command = format!("uv tool install {name}");
                writeln!(
                    printer.stderr(),
                    "`{}` is not installed; run `{}` to install",
                    name.cyan(),
                    install_command.green()
                )?;
                return Ok(ExitStatus::Failure);
            }
            Err(_) => {
                let install_command = format!("uv tool install --force {name}");
                writeln!(
                    printer.stderr(),
                    "`{}` is missing a valid receipt; run `{}` to reinstall",
                    name.cyan(),
                    install_command.green()
                )?;
                return Ok(ExitStatus::Failure);
            }
        };

        let mut existing_environment = match installed_tools.get_environment(&name, cache) {
            Ok(Some(environment)) => environment,
            Ok(None) => {
                let install_command = format!("uv tool install {name}");
                writeln!(
                    printer.stderr(),
                    "`{}` is not installed; run `{}` to install",
                    name.cyan(),
                    install_command.green()
                )?;
                return Ok(ExitStatus::Failure);
            }
            Err(_) => {
                let install_command = format!("uv tool install --force {name}");
                writeln!(
                    printer.stderr(),
                    "`{}` is missing a valid environment; run `{}` to reinstall",
                    name.cyan(),
                    install_command.green()
                )?;
                return Ok(ExitStatus::Failure);
            }
        };

        let mut recreated_venv = false;
        if let Some(python_request) = &python_request {
            if !python_request.satisfied(existing_environment.interpreter(), cache) {
                debug!("Requested `{python_request}`, not satisfied; reinstalling");

                let client_builder = BaseClientBuilder::new()
                    .connectivity(connectivity)
                    .native_tls(native_tls);

                let reporter = PythonDownloadReporter::single(printer);

                let interpreter = PythonInstallation::find_or_download(
                    Some(python_request.clone()),
                    EnvironmentPreference::OnlySystem,
                    python_preference,
                    python_downloads,
                    &client_builder,
                    cache,
                    Some(&reporter),
                )
                .await?
                .into_interpreter();

                existing_environment = installed_tools.create_environment(&name, interpreter)?;
                recreated_venv = true;
            }
        }

        // Resolve the appropriate settings, preferring: CLI > receipt > user.
        let options = args.clone().combine(
            ResolverInstallerOptions::from(existing_tool_receipt.options().clone())
                .combine(filesystem.clone()),
        );
        let settings = ResolverInstallerSettings::from(options.clone());

        // Resolve the requirements.
        let requirements = existing_tool_receipt.requirements();
        let spec = RequirementsSpecification::from_requirements(requirements.to_vec());

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

        did_upgrade |= !changelog.is_empty() || recreated_venv;

        // If we modified the target tool, reinstall the entrypoints.
        if changelog.includes(&name) || recreated_venv {
            // At this point, we updated the existing environment, so we should remove any of its
            // existing executables.
            remove_entrypoints(&existing_tool_receipt);

            install_executables(
                &environment,
                &name,
                &installed_tools,
                ToolOptions::from(options),
                true,
                existing_tool_receipt.python().to_owned(),
                requirements.to_vec(),
                printer,
            )?;
        }
    }

    if !did_upgrade {
        writeln!(printer.stderr(), "Nothing to upgrade")?;
    }

    Ok(ExitStatus::Success)
}

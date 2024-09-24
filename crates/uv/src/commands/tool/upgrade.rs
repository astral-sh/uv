use std::{collections::BTreeSet, fmt::Write};

use anyhow::Result;
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::Concurrency;
use uv_normalize::PackageName;
use uv_python::{
    EnvironmentPreference, Interpreter, PythonDownloads, PythonInstallation, PythonPreference,
    PythonRequest,
};
use uv_requirements::RequirementsSpecification;
use uv_settings::{Combine, ResolverInstallerOptions, ToolOptions};
use uv_tool::InstalledTools;

use crate::commands::pip::loggers::{
    DefaultInstallLogger, SummaryResolveLogger, UpgradeInstallLogger,
};
use crate::commands::project::{
    resolve_environment, sync_environment, update_environment, EnvironmentUpdate,
};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::tool::common::remove_entrypoints;
use crate::commands::{tool::common::install_executables, ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

enum ToolUpgradeResult {
    ToolOrDependency,
    Environment,
    NoOp,
}

/// Upgrade a tool.
pub(crate) async fn upgrade(
    name: Vec<PackageName>,
    python: Option<String>,
    connectivity: Connectivity,
    args: ResolverInstallerOptions,
    filesystem: ResolverInstallerOptions,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
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

    let reporter = PythonDownloadReporter::single(printer);
    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls);

    let python_request = python.as_deref().map(PythonRequest::parse);

    let interpreter = if python_request.is_some() {
        Some(
            PythonInstallation::find_or_download(
                python_request.as_ref(),
                EnvironmentPreference::OnlySystem,
                python_preference,
                python_downloads,
                &client_builder,
                cache,
                Some(&reporter),
            )
            .await?
            .into_interpreter(),
        )
    } else {
        None
    };

    // Determine whether we applied any upgrades.
    let mut did_upgrade = false;

    // Determine whether any tool upgrade failed.
    let mut failed_upgrade = false;

    for name in &names {
        debug!("Upgrading tool: `{name}`");
        let upgrade_tool_result = upgrade_tool(
            name,
            interpreter.as_ref(),
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

        match upgrade_tool_result {
            Ok(ToolUpgradeResult::ToolOrDependency | ToolUpgradeResult::Environment) => {
                did_upgrade |= true;
            }
            Ok(ToolUpgradeResult::NoOp) => {
                debug!("Request for upgrade of {name} was a no-op");
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

    if let (true, Some(python)) = (did_upgrade, python) {
        writeln!(
            printer.stderr(),
            "Upgraded build environment for {} to Python {}",
            if let [name] = names.into_iter().collect_vec().as_slice() {
                name.to_string()
            } else {
                "all tools".bold().to_string()
            },
            python
        )?;
    }

    Ok(ExitStatus::Success)
}

async fn upgrade_tool(
    name: &PackageName,
    interpreter: Option<&Interpreter>,
    printer: Printer,
    installed_tools: &InstalledTools,
    args: &ResolverInstallerOptions,
    cache: &Cache,
    filesystem: &ResolverInstallerOptions,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
) -> Result<ToolUpgradeResult> {
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

    let mut environment = match installed_tools.get_environment(name, cache) {
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

    let mut tool_or_env_upgraded = false;
    let mut changelog = None;

    // Check if we need to create a new environment — if so, resolve it first, then
    // install the requested tool
    if let Some(interpreter) = interpreter {
        if !interpreter.is_interpreter_used_by(&environment) {
            let resolution = resolve_environment(
                RequirementsSpecification::from_requirements(requirements.to_vec()).into(),
                interpreter,
                settings.as_ref().into(),
                &state,
                Box::new(SummaryResolveLogger),
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer,
            )
            .await?;

            environment = installed_tools.create_environment(name, interpreter.clone())?;

            environment = sync_environment(
                environment,
                &resolution.into(),
                settings.as_ref().into(),
                &state,
                Box::new(DefaultInstallLogger),
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer,
            )
            .await?;

            tool_or_env_upgraded = true;
        }
    }

    // If we didn't need to upgrade the environment, ensure the tool is updated
    if !tool_or_env_upgraded {
        // TODO(zanieb): Build the environment in the cache directory then copy into the tool
        // directory.
        let EnvironmentUpdate {
            environment: env_update,
            changelog: changelog_update,
        } = update_environment(
            environment,
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

        environment = env_update;
        if changelog_update.includes(name) {
            tool_or_env_upgraded = true;
        }
        changelog = if changelog_update.is_empty() {
            None
        } else {
            Some(changelog_update)
        }
    }

    if tool_or_env_upgraded {
        // At this point, we updated the existing environment, so we should remove any of its
        // existing executables.
        remove_entrypoints(&existing_tool_receipt);

        // If we modified the target tool, reinstall the entrypoints.
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

    if changelog.is_some() {
        Ok(ToolUpgradeResult::ToolOrDependency)
    } else if tool_or_env_upgraded {
        Ok(ToolUpgradeResult::Environment)
    } else {
        Ok(ToolUpgradeResult::NoOp)
    }
}

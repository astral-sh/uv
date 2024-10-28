use std::{collections::BTreeSet, fmt::Write};

use anyhow::Result;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::{Concurrency, TrustedHost};
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
    allow_insecure_host: &[TrustedHost],
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    let installed_tools = InstalledTools::from_settings()?.init()?;
    let _lock = installed_tools.lock().await?;

    // Collect the tools to upgrade.
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
        .native_tls(native_tls)
        .allow_insecure_host(allow_insecure_host.to_vec());

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
    let mut did_upgrade_tool = vec![];

    // Determine whether we applied any upgrades.
    let mut did_upgrade_environment = vec![];

    // Determine whether any tool upgrade failed.
    let mut failed_upgrade = false;

    for name in &names {
        debug!("Upgrading tool: `{name}`");
        let result = upgrade_tool(
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
            allow_insecure_host,
        )
        .await;

        match result {
            Ok(UpgradeOutcome::UpgradeEnvironment) => {
                did_upgrade_environment.push(name);
            }
            Ok(UpgradeOutcome::UpgradeDependencies | UpgradeOutcome::UpgradeTool) => {
                did_upgrade_tool.push(name);
            }
            Ok(UpgradeOutcome::NoOp) => {
                debug!("Upgrading `{name}` was a no-op");
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

    if did_upgrade_tool.is_empty() && did_upgrade_environment.is_empty() {
        writeln!(printer.stderr(), "Nothing to upgrade")?;
    }

    if let Some(python_request) = python_request {
        let tools = did_upgrade_environment
            .iter()
            .map(|name| format!("`{}`", name.cyan()))
            .collect::<Vec<_>>();
        let s = if tools.len() > 1 { "s" } else { "" };
        writeln!(
            printer.stderr(),
            "Upgraded tool environment{s} for {} to {}",
            conjunction(tools),
            python_request.cyan(),
        )?;
    }

    Ok(ExitStatus::Success)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpgradeOutcome {
    /// The tool itself was upgraded.
    UpgradeTool,
    /// The tool's dependencies were upgraded, but the tool itself was unchanged.
    UpgradeDependencies,
    /// The tool's environment was upgraded.
    UpgradeEnvironment,
    /// The tool was already up-to-date.
    NoOp,
}

/// Upgrade a specific tool.
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
    allow_insecure_host: &[TrustedHost],
) -> Result<UpgradeOutcome> {
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

    let environment = match installed_tools.get_environment(name, cache) {
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

    // Check if we need to create a new environment — if so, resolve it first, then
    // install the requested tool
    let (environment, outcome) = if let Some(interpreter) =
        interpreter.filter(|interpreter| !environment.uses(interpreter))
    {
        // If we're using a new interpreter, re-create the environment for each tool.
        let resolution = resolve_environment(
            RequirementsSpecification::from_requirements(requirements.to_vec()).into(),
            interpreter,
            settings.as_ref().into(),
            &state,
            Box::new(SummaryResolveLogger),
            connectivity,
            concurrency,
            native_tls,
            allow_insecure_host,
            cache,
            printer,
        )
        .await?;

        let environment = installed_tools.create_environment(name, interpreter.clone())?;

        let environment = sync_environment(
            environment,
            &resolution.into(),
            settings.as_ref().into(),
            &state,
            Box::new(DefaultInstallLogger),
            connectivity,
            concurrency,
            native_tls,
            allow_insecure_host,
            cache,
            printer,
        )
        .await?;

        (environment, UpgradeOutcome::UpgradeEnvironment)
    } else {
        // Otherwise, upgrade the existing environment.
        // TODO(zanieb): Build the environment in the cache directory then copy into the tool
        // directory.
        let EnvironmentUpdate {
            environment,
            changelog,
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
            allow_insecure_host,
            cache,
            printer,
        )
        .await?;

        let outcome = if changelog.includes(name) {
            UpgradeOutcome::UpgradeTool
        } else if changelog.is_empty() {
            UpgradeOutcome::NoOp
        } else {
            UpgradeOutcome::UpgradeDependencies
        };

        (environment, outcome)
    };

    if matches!(
        outcome,
        UpgradeOutcome::UpgradeEnvironment | UpgradeOutcome::UpgradeTool
    ) {
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

    Ok(outcome)
}

/// Given a list of names, return a conjunction of the names (e.g., "Alice, Bob and Charlie").
fn conjunction(names: Vec<String>) -> String {
    let mut names = names.into_iter();
    let first = names.next();
    let last = names.next_back();
    match (first, last) {
        (Some(first), Some(last)) => {
            let mut result = first;
            let mut comma = false;
            for name in names {
                result.push_str(", ");
                result.push_str(&name);
                comma = true;
            }
            if comma {
                result.push_str(", and ");
            } else {
                result.push_str(" and ");
            }
            result.push_str(&last);
            result
        }
        (Some(first), None) => first,
        _ => String::new(),
    }
}

use anyhow::Result;
use itertools::Itertools;
use owo_colors::OwoColorize;
use std::collections::BTreeMap;
use std::fmt::Write;
use tracing::debug;

use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::{Concurrency, DryRun, PreviewMode};
use uv_fs::CWD;
use uv_normalize::PackageName;
use uv_pypi_types::Requirement;
use uv_python::{
    EnvironmentPreference, Interpreter, PythonDownloads, PythonInstallation, PythonPreference,
    PythonRequest,
};
use uv_requirements::RequirementsSpecification;
use uv_settings::{Combine, PythonInstallMirrors, ResolverInstallerOptions, ToolOptions};
use uv_tool::InstalledTools;
use uv_workspace::WorkspaceCache;

use crate::commands::pip::loggers::{
    DefaultInstallLogger, SummaryResolveLogger, UpgradeInstallLogger,
};
use crate::commands::pip::operations::Modifications;
use crate::commands::project::{
    resolve_environment, sync_environment, update_environment, EnvironmentUpdate, PlatformState,
};
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::tool::common::remove_entrypoints;
use crate::commands::{conjunction, tool::common::install_executables, ExitStatus};
use crate::printer::Printer;
use crate::settings::{NetworkSettings, ResolverInstallerSettings};

/// Upgrade a tool.
pub(crate) async fn upgrade(
    names: Vec<String>,
    python: Option<String>,
    install_mirrors: PythonInstallMirrors,
    args: ResolverInstallerOptions,
    filesystem: ResolverInstallerOptions,
    network_settings: NetworkSettings,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    installer_metadata: bool,
    concurrency: Concurrency,
    cache: &Cache,
    printer: Printer,
    preview: PreviewMode,
) -> Result<ExitStatus> {
    let installed_tools = InstalledTools::from_settings()?.init()?;
    let _lock = installed_tools.lock().await?;

    // Collect the tools to upgrade, along with any constraints.
    let names: BTreeMap<PackageName, Vec<Requirement>> = {
        if names.is_empty() {
            installed_tools
                .tools()
                .unwrap_or_default()
                .into_iter()
                .map(|(name, _)| (name, Vec::new()))
                .collect()
        } else {
            let mut map = BTreeMap::new();
            for name in names {
                let requirement = Requirement::from(uv_pep508::Requirement::parse(&name, &*CWD)?);
                map.entry(requirement.name.clone())
                    .or_insert_with(Vec::new)
                    .push(requirement);
            }
            map
        }
    };

    if names.is_empty() {
        writeln!(printer.stderr(), "Nothing to upgrade")?;
        return Ok(ExitStatus::Success);
    }

    let reporter = PythonDownloadReporter::single(printer);
    let client_builder = BaseClientBuilder::new()
        .connectivity(network_settings.connectivity)
        .native_tls(network_settings.native_tls)
        .allow_insecure_host(network_settings.allow_insecure_host.clone());

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
                install_mirrors.python_install_mirror.as_deref(),
                install_mirrors.pypy_install_mirror.as_deref(),
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

    let mut errors = Vec::new();
    for (name, constraints) in &names {
        debug!("Upgrading tool: `{name}`");
        let result = upgrade_tool(
            name,
            constraints,
            interpreter.as_ref(),
            printer,
            &installed_tools,
            &args,
            &network_settings,
            cache,
            &filesystem,
            installer_metadata,
            concurrency,
            preview,
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
                errors.push((name, err));
            }
        }
    }

    if !errors.is_empty() {
        for (name, err) in errors
            .into_iter()
            .sorted_unstable_by(|(name_a, _), (name_b, _)| name_a.cmp(name_b))
        {
            writeln!(
                printer.stderr(),
                "{}: Failed to upgrade {}",
                "error".red().bold(),
                name.green()
            )?;
            for err in err.chain() {
                writeln!(
                    printer.stderr(),
                    "  {}: {}",
                    "Caused by".red().bold(),
                    err.to_string().trim()
                )?;
            }
        }
        return Ok(ExitStatus::Failure);
    }

    if did_upgrade_tool.is_empty() && did_upgrade_environment.is_empty() {
        writeln!(printer.stderr(), "Nothing to upgrade")?;
    }

    if let Some(python_request) = python_request {
        if !did_upgrade_environment.is_empty() {
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
    constraints: &[Requirement],
    interpreter: Option<&Interpreter>,
    printer: Printer,
    installed_tools: &InstalledTools,
    args: &ResolverInstallerOptions,
    network_settings: &NetworkSettings,
    cache: &Cache,
    filesystem: &ResolverInstallerOptions,
    installer_metadata: bool,
    concurrency: Concurrency,
    preview: PreviewMode,
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
    let spec = RequirementsSpecification::from_overrides(
        existing_tool_receipt.requirements().to_vec(),
        existing_tool_receipt
            .constraints()
            .iter()
            .chain(constraints)
            .cloned()
            .collect(),
        existing_tool_receipt.overrides().to_vec(),
    );

    // Initialize any shared state.
    let state = PlatformState::default();
    let workspace_cache = WorkspaceCache::default();

    // Check if we need to create a new environment — if so, resolve it first, then
    // install the requested tool
    let (environment, outcome) = if let Some(interpreter) =
        interpreter.filter(|interpreter| !environment.uses(interpreter))
    {
        // If we're using a new interpreter, re-create the environment for each tool.
        let resolution = resolve_environment(
            spec.into(),
            interpreter,
            &settings.resolver,
            network_settings,
            &state,
            Box::new(SummaryResolveLogger),
            concurrency,
            cache,
            printer,
            preview,
        )
        .await?;

        let environment = installed_tools.create_environment(name, interpreter.clone())?;

        let environment = sync_environment(
            environment,
            &resolution.into(),
            Modifications::Exact,
            (&settings).into(),
            network_settings,
            &state,
            Box::new(DefaultInstallLogger),
            installer_metadata,
            concurrency,
            cache,
            printer,
            preview,
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
            Modifications::Exact,
            &settings,
            network_settings,
            &state,
            Box::new(SummaryResolveLogger),
            Box::new(UpgradeInstallLogger::new(name.clone())),
            installer_metadata,
            concurrency,
            cache,
            workspace_cache,
            DryRun::Disabled,
            printer,
            preview,
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
            existing_tool_receipt.requirements().to_vec(),
            existing_tool_receipt.constraints().to_vec(),
            existing_tool_receipt.overrides().to_vec(),
            printer,
        )?;
    }

    Ok(outcome)
}

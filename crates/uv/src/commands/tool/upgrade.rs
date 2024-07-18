use std::{collections::BTreeSet, fmt::Write};

use crate::{
    commands::{
        project::update_environment,
        tool::common::{resolve_requirements, InstallAction},
    },
    settings::ResolverInstallerSettings,
};
use anyhow::Result;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, PreviewMode};
use uv_normalize::PackageName;
use uv_requirements::RequirementsSpecification;
use uv_tool::InstalledTools;
use uv_warnings::warn_user_once;

use crate::commands::{tool::common::install_executables, ExitStatus, SharedState};
use crate::printer::Printer;

/// Upgrade a tool.
pub(crate) async fn upgrade(
    name: Option<PackageName>,
    connectivity: Connectivity,
    settings: ResolverInstallerSettings,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    preview: PreviewMode,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv tool upgrade` is experimental and may change without warning");
    }
    // Initialize any shared state.
    let state = SharedState::default();

    let installed_tools = InstalledTools::from_settings()?.init()?;
    let _lock = installed_tools.acquire_lock()?;

    let names: BTreeSet<PackageName> = name
        .map(|name| {
            let mut set = BTreeSet::new();
            set.insert(name);
            set
        })
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

    for name in names {
        debug!("Upgrading tool {name}");
        let existing_tool_receipt = installed_tools.get_tool_receipt(&name)?;
        let existing_environment = installed_tools.get_environment(&name, cache)?;
        if existing_tool_receipt.is_none() {
            debug!("Unable to find tool receipt for {name}");
        }
        if existing_environment.is_none() {
            debug!("Unable to find environment for {name}");
        }

        if let (Some(tool_receipt), Some(environment)) =
            (existing_tool_receipt, existing_environment)
        {
            let requirement = resolve_requirements(
                std::iter::once(name.as_str()),
                environment.interpreter(),
                &settings,
                &state,
                preview,
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer,
            )
            .await?
            .pop()
            .unwrap();
            let requirements = vec![requirement];

            // Resolve the requirements.
            let spec = RequirementsSpecification::from_requirements(requirements.clone());

            // TODO(zanieb): Build the environment in the cache directory then copy into the tool directory.
            // This lets us confirm the environment is valid before removing an existing install. However,
            // entrypoints always contain an absolute path to the relevant Python interpreter, which would
            // be invalidated by moving the environment.
            let environment = update_environment(
                environment,
                spec,
                &settings,
                &state,
                preview,
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer,
            )
            .await?;
            install_executables(
                &environment,
                &name,
                &installed_tools,
                printer,
                true,
                true,
                tool_receipt.python().to_owned(),
                requirements,
                &InstallAction::Update,
            )?;
        } else {
            let install_command = format!("uv tool install {name}");
            writeln!(
                printer.stderr(),
                "`{name}` is not installed; install `{name}` via `{}`",
                install_command.cyan()
            )?;
            return Ok(ExitStatus::Failure);
        }
    }
    Ok(ExitStatus::Success)
}

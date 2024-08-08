use std::{collections::BTreeSet, fmt::Write};

use anyhow::Result;
use tracing::debug;

use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, PreviewMode, Upgrade};
use uv_normalize::PackageName;
use uv_requirements::RequirementsSpecification;
use uv_tool::InstalledTools;
use uv_warnings::{warn_user, warn_user_once};

use crate::commands::{ExitStatus, SharedState, tool::common::install_executables};
use crate::commands::pip::loggers::{DefaultInstallLogger, DefaultResolveLogger};
use crate::commands::project::update_environment;
use crate::commands::tool::common::{InstallAction, remove_entrypoints};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

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

    // Force upgrades.
    let settings = ResolverInstallerSettings {
        upgrade: Upgrade::All,
        ..settings
    };

    // Initialize any shared state.
    let state = SharedState::default();

    let installed_tools = InstalledTools::from_settings()?.init()?;
    let _lock = installed_tools.acquire_lock()?;

    let names: BTreeSet<PackageName> = name
        .map(|name| {
            BTreeSet::from_iter([name])
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
        debug!("Upgrading tool: `{name}`");

        let Some(existing_tool_receipt) = installed_tools.get_tool_receipt(&name)? else {
            warn_user!(
                "Ignoring malformed tool `{name}` (run `{}` to remove)",
                format!("uv tool uninstall {name}").green()
            );
            continue;
        };

        let Some(existing_environment) = installed_tools.get_environment(&name, cache)? else {
            warn_user!(
                "Ignoring malformed tool `{name}` (run `{}` to remove)",
                format!("uv tool uninstall {name}").green()
            );
            continue;
        };

        // Resolve the requirements.
        let requirements = existing_tool_receipt.requirements();
        let spec = RequirementsSpecification::from_requirements(requirements.to_vec());

        // TODO(zanieb): Build the environment in the cache directory then copy into the tool directory.
        // This lets us confirm the environment is valid before removing an existing install. However,
        // entrypoints always contain an absolute path to the relevant Python interpreter, which would
        // be invalidated by moving the environment.
        let environment = update_environment(
            existing_environment,
            spec,
            &settings,
            &state,
            Box::new(DefaultResolveLogger),
            Box::new(DefaultInstallLogger),
            preview,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
        )
        .await?;

        // At this point, we updated the existing environment, so we should remove any of its
        // existing executables.
        remove_entrypoints(&existing_tool_receipt);

        install_executables(
            &environment,
            &name,
            &installed_tools,
            printer,
            true,
            existing_tool_receipt.python().to_owned(),
            requirements.to_vec(),
            InstallAction::Update,
        )?;
    }

    Ok(ExitStatus::Success)
}

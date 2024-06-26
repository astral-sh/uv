use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt::Write;
use std::str::FromStr;

use anyhow::{bail, Context, Result};
use distribution_types::Name;
use itertools::Itertools;

use pep508_rs::Requirement;
use pypi_types::VerbatimParsedUrl;
use tracing::debug;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, PreviewMode, Reinstall};
#[cfg(unix)]
use uv_fs::replace_symlink;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_requirements::RequirementsSource;
use uv_tool::{entrypoint_paths, find_executable_directory, InstalledTools, Tool};
use uv_toolchain::{EnvironmentPreference, Toolchain, ToolchainPreference, ToolchainRequest};
use uv_warnings::warn_user_once;

use crate::commands::project::update_environment;
use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Install a tool.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn install(
    name: String,
    python: Option<String>,
    from: Option<String>,
    with: Vec<String>,
    force: bool,
    settings: ResolverInstallerSettings,
    preview: PreviewMode,
    toolchain_preference: ToolchainPreference,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv tool install` is experimental and may change without warning.");
    }
    let from = from.unwrap_or(name.clone());

    let installed_tools = InstalledTools::from_settings()?;

    // TODO(zanieb): Figure out the interface here, do we infer the name or do we match the `run --from` interface?
    let from = Requirement::<VerbatimParsedUrl>::from_str(&from)?;
    let existing_tool_receipt = installed_tools.get_tool_receipt(&name)?;
    // TODO(zanieb): Automatically replace an existing tool if the request differs
    let reinstall_entry_points = if existing_tool_receipt.is_some() {
        if force {
            debug!("Replacing existing tool due to `--force` flag.");
            false
        } else {
            match settings.reinstall {
                Reinstall::All => {
                    debug!("Replacing existing tool due to `--reinstall` flag.");
                    true
                }
                // Do not replace the entry points unless the tool is explicitly requested
                Reinstall::Packages(ref packages) => packages.contains(&from.name),
                // If not reinstalling... then we're done
                Reinstall::None => {
                    writeln!(printer.stderr(), "Tool `{name}` is already installed")?;
                    return Ok(ExitStatus::Failure);
                }
            }
        }
    } else {
        false
    };

    let requirements = [Ok(from.clone())]
        .into_iter()
        .chain(with.iter().map(|name| Requirement::from_str(name)))
        .collect::<Result<Vec<Requirement<VerbatimParsedUrl>>, _>>()?;

    // TODO(zanieb): Duplicative with the above parsing but needed for `update_environment`
    let requirements_sources = [RequirementsSource::from_package(from.to_string())]
        .into_iter()
        .chain(with.into_iter().map(RequirementsSource::from_package))
        .collect::<Vec<_>>();

    let Some(from) = requirements.first().cloned() else {
        bail!("Expected at least one requirement")
    };
    let tool = Tool::new(requirements, python.clone());

    let interpreter = Toolchain::find(
        &python
            .as_deref()
            .map(ToolchainRequest::parse)
            .unwrap_or_default(),
        EnvironmentPreference::OnlySystem,
        toolchain_preference,
        cache,
    )?
    .into_interpreter();

    // TODO(zanieb): Build the environment in the cache directory then copy into the tool directory
    // This lets us confirm the environment is valid before removing an existing install
    let environment = installed_tools.environment(
        &name,
        // Do not remove the existing environment if we're reinstalling a subset of packages
        !matches!(settings.reinstall, Reinstall::Packages(_)),
        interpreter,
        cache,
    )?;

    // Install the ephemeral requirements.
    let environment = update_environment(
        environment,
        &requirements_sources,
        &settings,
        preview,
        connectivity,
        concurrency,
        native_tls,
        cache,
        printer,
    )
    .await?;

    let site_packages = SitePackages::from_environment(&environment)?;
    let installed = site_packages.get_packages(&from.name);
    let Some(installed_dist) = installed.first().copied() else {
        bail!("Expected at least one requirement")
    };

    // Exit early if we're not supposed to be reinstalling entry points
    // e.g. `--reinstall-package` was used for some dependency
    if existing_tool_receipt.is_some() && !reinstall_entry_points {
        writeln!(printer.stderr(), "Updated environment for tool `{name}`")?;
        return Ok(ExitStatus::Success);
    }

    // Find a suitable path to install into
    // TODO(zanieb): Warn if this directory is not on the PATH
    let executable_directory = find_executable_directory()?;
    fs_err::create_dir_all(&executable_directory)
        .context("Failed to create executable directory")?;

    debug!(
        "Installing tool entry points into {}",
        executable_directory.user_display()
    );

    let entrypoints = entrypoint_paths(
        &environment,
        installed_dist.name(),
        installed_dist.version(),
    )?;

    // Determine the entry points targets
    // Use a sorted collection for deterministic output
    let targets = entrypoints
        .into_iter()
        .map(|(name, path)| {
            let target = executable_directory.join(
                path.file_name()
                    .map(std::borrow::ToOwned::to_owned)
                    .unwrap_or_else(|| OsString::from(name.clone())),
            );
            (name, path, target)
        })
        .collect::<BTreeSet<_>>();

    // Check if they exist, before installing
    let mut existing_targets = targets
        .iter()
        .filter(|(_, _, target)| target.exists())
        .peekable();

    // Note we use `reinstall_entry_points` here instead of `reinstall`; requesting reinstall
    // will _not_ remove existing entry points when they are not managed by uv.
    if force || reinstall_entry_points {
        for (name, _, target) in existing_targets {
            debug!("Removing existing entry point `{name}`");
            fs_err::remove_file(target)?;
        }
    } else if existing_targets.peek().is_some() {
        // Clean up the environment we just created
        installed_tools.remove_environment(&name)?;

        let existing_targets = existing_targets
            // SAFETY: We know the target has a filename because we just constructed it above
            .map(|(_, _, target)| target.file_name().unwrap().to_string_lossy())
            .collect::<Vec<_>>();
        let (s, exists) = if existing_targets.len() == 1 {
            ("", "exists")
        } else {
            ("s", "exist")
        };
        bail!(
            "Entry point{s} for tool already {exists}: {} (use `--force` to overwrite)",
            existing_targets.iter().join(", ")
        )
    }

    // TODO(zanieb): Handle the case where there are no entrypoints
    for (name, path, target) in &targets {
        debug!("Installing `{name}`");
        #[cfg(unix)]
        replace_symlink(path, target).context("Failed to install entrypoint")?;
        #[cfg(windows)]
        fs_err::copy(path, target).context("Failed to install entrypoint")?;
    }

    debug!("Adding receipt for tool `{name}`",);
    let installed_tools = installed_tools.init()?;
    installed_tools.add_tool_receipt(&name, tool)?;

    writeln!(
        printer.stdout(),
        "Installed: {}",
        targets.iter().map(|(name, _, _)| name).join(", ")
    )?;

    Ok(ExitStatus::Success)
}

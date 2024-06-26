use std::fmt::Write;
use std::str::FromStr;

use anyhow::{bail, Context, Result};
use distribution_types::Name;
use pep508_rs::Requirement;

use pypi_types::VerbatimParsedUrl;
use tracing::debug;
use uv_cache::Cache;
use uv_client::Connectivity;
use uv_configuration::{Concurrency, PreviewMode};
use uv_fs::{replace_symlink, Simplified};
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

    let installed_tools = InstalledTools::from_settings()?;

    // TODO(zanieb): Allow replacing an existing tool
    if installed_tools.find_tool_entry(&name)?.is_some() {
        writeln!(printer.stderr(), "Tool `{name}` is already installed.")?;
        return Ok(ExitStatus::Failure);
    }

    // TODO(zanieb): Figure out the interface here, do we infer the name or do we match the `run --from` interface?
    let from = from.unwrap_or(name.clone());

    let requirements = [Requirement::from_str(&from)]
        .into_iter()
        .chain(with.iter().map(|name| Requirement::from_str(name)))
        .collect::<Result<Vec<Requirement<VerbatimParsedUrl>>, _>>()?;

    // TODO(zanieb): Duplicative with the above parsing but needed for `update_environment`
    let requirements_sources = [RequirementsSource::from_package(from.clone())]
        .into_iter()
        .chain(with.into_iter().map(RequirementsSource::from_package))
        .collect::<Vec<_>>();

    let Some(from) = requirements.first().cloned() else {
        bail!("Expected at least one requirement")
    };
    let tool = Tool::new(requirements, python.clone());
    let path = installed_tools.tools_toml_path();

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
    let environment = installed_tools.create_environment(&name, interpreter)?;

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

    // Find a suitable path to install into
    // TODO(zanieb): Warn if this directory is not on the PATH
    let executable_directory = find_executable_directory()?;
    fs_err::create_dir_all(&executable_directory)
        .context("Failed to create executable directory")?;

    let entrypoints = entrypoint_paths(
        &environment,
        installed_dist.name(),
        installed_dist.version(),
    )?;

    // TODO(zanieb): Handle the case where there are no entrypoints
    // TODO(zanieb): Better error when an entry point exists, check if they all are don't exist first
    for (name, path) in entrypoints {
        let target = executable_directory.join(path.file_name().unwrap());
        debug!("Installing {name} to {}", target.user_display());
        replace_symlink(&path, &target).context("Failed to install entrypoint")?;
    }

    debug!("Adding `{name}` to {}", path.user_display());
    let installed_tools = installed_tools.init()?;
    installed_tools.add_tool_entry(&name, &tool)?;

    Ok(ExitStatus::Success)
}

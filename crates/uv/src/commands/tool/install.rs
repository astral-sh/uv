use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt::Write;
use std::str::FromStr;

use anyhow::{bail, Context, Result};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::{debug, warn};

use distribution_types::{Name, UnresolvedRequirementSpecification};
use pypi_types::Requirement;
use uv_cache::Cache;
use uv_client::{BaseClientBuilder, Connectivity};
use uv_configuration::{Concurrency, PreviewMode};
#[cfg(unix)]
use uv_fs::replace_symlink;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_python::{
    EnvironmentPreference, PythonEnvironment, PythonFetch, PythonInstallation, PythonPreference,
    PythonRequest,
};
use uv_requirements::{RequirementsSource, RequirementsSpecification};
use uv_shell::Shell;
use uv_tool::{entrypoint_paths, find_executable_directory, InstalledTools, Tool, ToolEntrypoint};
use uv_warnings::{warn_user, warn_user_once};

use crate::commands::reporters::PythonDownloadReporter;

use crate::commands::{
    project::{resolve_environment, resolve_names, sync_environment, update_environment},
    tool::common::matching_packages,
};
use crate::commands::{ExitStatus, SharedState};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

/// Install a tool.
pub(crate) async fn install(
    package: String,
    editable: bool,
    from: Option<String>,
    with: &[RequirementsSource],
    python: Option<String>,
    force: bool,
    settings: ResolverInstallerSettings,
    preview: PreviewMode,
    python_preference: PythonPreference,
    python_fetch: PythonFetch,
    connectivity: Connectivity,
    concurrency: Concurrency,
    native_tls: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv tool install` is experimental and may change without warning");
    }

    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls);

    let reporter = PythonDownloadReporter::single(printer);

    let python_request = python.as_deref().map(PythonRequest::parse);

    // Pre-emptively identify a Python interpreter. We need an interpreter to resolve any unnamed
    // requirements, even if we end up using a different interpreter for the tool install itself.
    let interpreter = PythonInstallation::find_or_fetch(
        python_request.clone(),
        EnvironmentPreference::OnlySystem,
        python_preference,
        python_fetch,
        &client_builder,
        cache,
        Some(&reporter),
    )
    .await?
    .into_interpreter();

    // Initialize any shared state.
    let state = SharedState::default();
    let client_builder = BaseClientBuilder::new()
        .connectivity(connectivity)
        .native_tls(native_tls);

    // Resolve the `from` requirement.
    let from = if let Some(from) = from {
        // Parse the positional name. If the user provided more than a package name, it's an error
        // (e.g., `uv install foo==1.0 --from foo`).
        let Ok(package) = PackageName::from_str(&package) else {
            bail!("Package requirement (`{from}`) provided with `--from` conflicts with install request (`{package}`)", from = from.cyan(), package = package.cyan())
        };

        let source = if editable {
            RequirementsSource::Editable(from)
        } else {
            RequirementsSource::Package(from)
        };
        let requirements = RequirementsSpecification::from_source(&source, &client_builder)
            .await?
            .requirements;

        let from_requirement = {
            resolve_names(
                requirements,
                &interpreter,
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
            .unwrap()
        };

        // Check if the positional name conflicts with `--from`.
        if from_requirement.name != package {
            // Determine if it's an entirely different package (e.g., `uv install foo --from bar`).
            bail!(
                "Package name (`{}`) provided with `--from` does not match install request (`{}`)",
                from_requirement.name.cyan(),
                package.cyan()
            );
        }

        from_requirement
    } else {
        let source = if editable {
            RequirementsSource::Editable(package.clone())
        } else {
            RequirementsSource::Package(package.clone())
        };
        let requirements = RequirementsSpecification::from_source(&source, &client_builder)
            .await?
            .requirements;

        resolve_names(
            requirements,
            &interpreter,
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
        .unwrap()
    };

    // Read the `--with` requirements.
    let spec = RequirementsSpecification::from_simple_sources(with, &client_builder).await?;

    // Resolve the `--from` and `--with` requirements.
    let requirements = {
        let mut requirements = Vec::with_capacity(1 + with.len());
        requirements.push(from.clone());
        requirements.extend(
            resolve_names(
                spec.requirements.clone(),
                &interpreter,
                &settings,
                &state,
                preview,
                connectivity,
                concurrency,
                native_tls,
                cache,
                printer,
            )
            .await?,
        );
        requirements
    };

    let installed_tools = InstalledTools::from_settings()?.init()?;
    let _lock = installed_tools.acquire_lock()?;

    // Find the existing receipt, if it exists. If the receipt is present but malformed, we'll
    // remove the environment and continue with the install.
    //
    // Later on, we want to replace entrypoints if the tool already exists, regardless of whether
    // the receipt was valid.
    //
    // (If we find existing entrypoints later on, and the tool _doesn't_ exist, we'll avoid removing
    // the external tool's entrypoints (without `--force`).)
    let (existing_tool_receipt, reinstall_entry_points) =
        match installed_tools.get_tool_receipt(&from.name) {
            Ok(None) => (None, false),
            Ok(Some(receipt)) => (Some(receipt), true),
            Err(_) => {
                // If the tool is not installed properly, remove the environment and continue.
                match installed_tools.remove_environment(&from.name) {
                    Ok(()) => {
                        warn_user!(
                            "Removed existing `{from}` with invalid receipt",
                            from = from.name.cyan()
                        );
                    }
                    Err(uv_tool::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(err) => {
                        return Err(err.into());
                    }
                }
                (None, true)
            }
        };

    let existing_environment =
        installed_tools
            .get_environment(&from.name, cache)?
            .filter(|environment| {
                python_request.as_ref().map_or(true, |python_request| {
                    if python_request.satisfied(environment.interpreter(), cache) {
                        debug!("Found existing environment for `{from}`", from = from.name.cyan());
                        true
                    } else {
                        let _ = writeln!(
                            printer.stderr(),
                            "Existing environment for `{from}` does not satisfy the requested Python interpreter",
                            from = from.name.cyan(),
                        );
                        false
                    }
                })
            });

    // If the requested and receipt requirements are the same...
    if existing_environment.is_some() {
        if let Some(tool_receipt) = existing_tool_receipt.as_ref() {
            let receipt = tool_receipt
                .requirements()
                .iter()
                .cloned()
                .map(Requirement::from)
                .collect::<Vec<_>>();
            if requirements == receipt {
                // And the user didn't request a reinstall or upgrade...
                if !force && settings.reinstall.is_none() && settings.upgrade.is_none() {
                    // We're done.
                    writeln!(
                        printer.stderr(),
                        "`{from}` is already installed",
                        from = from.cyan()
                    )?;
                    return Ok(ExitStatus::Success);
                }
            }
        }
    }

    // Create a `RequirementsSpecification` from the resolved requirements, to avoid re-resolving.
    let spec = RequirementsSpecification {
        requirements: requirements
            .iter()
            .cloned()
            .map(UnresolvedRequirementSpecification::from)
            .collect(),
        ..spec
    };

    // TODO(zanieb): Build the environment in the cache directory then copy into the tool directory.
    // This lets us confirm the environment is valid before removing an existing install. However,
    // entrypoints always contain an absolute path to the relevant Python interpreter, which would
    // be invalidated by moving the environment.
    let environment = if let Some(environment) = existing_environment {
        update_environment(
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
        .await?
    } else {
        // If we're creating a new environment, ensure that we can resolve the requirements prior
        // to removing any existing tools.
        let resolution = resolve_environment(
            &interpreter,
            spec,
            settings.as_ref().into(),
            &state,
            preview,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
        )
        .await?;

        let environment = installed_tools.create_environment(&from.name, interpreter)?;

        // Sync the environment with the resolved requirements.
        sync_environment(
            environment,
            &resolution.into(),
            settings.as_ref().into(),
            &state,
            preview,
            connectivity,
            concurrency,
            native_tls,
            cache,
            printer,
        )
        .await?
    };

    let site_packages = SitePackages::from_environment(&environment)?;
    let installed = site_packages.get_packages(&from.name);
    let Some(installed_dist) = installed.first().copied() else {
        bail!("Expected at least one requirement")
    };

    // Find a suitable path to install into
    let executable_directory = find_executable_directory()?;
    fs_err::create_dir_all(&executable_directory)
        .context("Failed to create executable directory")?;

    debug!(
        "Installing tool executables into: {}",
        executable_directory.user_display()
    );

    let entry_points = entrypoint_paths(
        &environment,
        installed_dist.name(),
        installed_dist.version(),
    )?;

    // Determine the entry points targets
    // Use a sorted collection for deterministic output
    let target_entry_points = entry_points
        .into_iter()
        .map(|(name, source_path)| {
            let target_path = executable_directory.join(
                source_path
                    .file_name()
                    .map(std::borrow::ToOwned::to_owned)
                    .unwrap_or_else(|| OsString::from(name.clone())),
            );
            (name, source_path, target_path)
        })
        .collect::<BTreeSet<_>>();

    if target_entry_points.is_empty() {
        writeln!(
            printer.stdout(),
            "No executables are provided by `{from}`",
            from = from.name.cyan()
        )?;

        hint_executable_from_dependency(&from, &environment, printer)?;

        // Clean up the environment we just created
        installed_tools.remove_environment(&from.name)?;
        return Ok(ExitStatus::Failure);
    }

    // Check if they exist, before installing
    let mut existing_entry_points = target_entry_points
        .iter()
        .filter(|(_, _, target_path)| target_path.exists())
        .peekable();

    // Note we use `reinstall_entry_points` here instead of `reinstall`; requesting reinstall
    // will _not_ remove existing entry points when they are not managed by uv.
    if force || reinstall_entry_points {
        for (name, _, target) in existing_entry_points {
            debug!("Removing existing executable: `{name}`");
            fs_err::remove_file(target)?;
        }
    } else if existing_entry_points.peek().is_some() {
        // Clean up the environment we just created
        installed_tools.remove_environment(&from.name)?;

        let existing_entry_points = existing_entry_points
            // SAFETY: We know the target has a filename because we just constructed it above
            .map(|(_, _, target)| target.file_name().unwrap().to_string_lossy())
            .collect::<Vec<_>>();
        let (s, exists) = if existing_entry_points.len() == 1 {
            ("", "exists")
        } else {
            ("s", "exist")
        };
        bail!(
            "Executable{s} already {exists}: {} (use `--force` to overwrite)",
            existing_entry_points
                .iter()
                .map(|name| name.bold())
                .join(", ")
        )
    }

    for (name, source_path, target_path) in &target_entry_points {
        debug!("Installing executable: `{name}`");
        #[cfg(unix)]
        replace_symlink(source_path, target_path).context("Failed to install executable")?;
        #[cfg(windows)]
        fs_err::copy(source_path, target_path).context("Failed to install entrypoint")?;
    }

    let s = if target_entry_points.len() == 1 {
        ""
    } else {
        "s"
    };
    writeln!(
        printer.stderr(),
        "Installed {} executable{s}: {}",
        target_entry_points.len(),
        target_entry_points
            .iter()
            .map(|(name, _, _)| name.bold())
            .join(", ")
    )?;

    debug!("Adding receipt for tool `{}`", from.name);
    let tool = Tool::new(
        requirements
            .into_iter()
            .map(pep508_rs::Requirement::from)
            .collect(),
        python,
        target_entry_points
            .into_iter()
            .map(|(name, _, target_path)| ToolEntrypoint::new(name, target_path)),
    );
    installed_tools.add_tool_receipt(&from.name, tool)?;

    // If the executable directory isn't on the user's PATH, warn.
    if !Shell::contains_path(&executable_directory) {
        if let Some(shell) = Shell::from_env() {
            if let Some(command) = shell.prepend_path(&executable_directory) {
                if shell.configuration_files().is_empty() {
                    warn_user!(
                        "`{}` is not on your PATH. To use installed tools, run `{}`.",
                        executable_directory.simplified_display().cyan(),
                        command.green()
                    );
                } else {
                    warn_user!(
                        "`{}` is not on your PATH. To use installed tools, run `{}` or `{}`.",
                        executable_directory.simplified_display().cyan(),
                        command.green(),
                        "uv tool update-shell".green()
                    );
                }
            } else {
                warn_user!(
                    "`{}` is not on your PATH. To use installed tools, add the directory to your PATH.",
                    executable_directory.simplified_display().cyan(),
                );
            }
        } else {
            warn_user!(
                "`{}` is not on your PATH. To use installed tools, add the directory to your PATH.",
                executable_directory.simplified_display().cyan(),
            );
        }
    }

    Ok(ExitStatus::Success)
}

/// Displays a hint if an executable matching the package name can be found in a dependency of the package.
fn hint_executable_from_dependency(
    from: &Requirement,
    environment: &PythonEnvironment,
    printer: Printer,
) -> Result<()> {
    match matching_packages(from.name.as_ref(), environment) {
        Ok(packages) => match packages.as_slice() {
            [] => {}
            [package] => {
                let command = format!("uv tool install {}", package.name());
                writeln!(
                        printer.stdout(),
                        "However, an executable with the name `{}` is available via dependency `{}`.\nDid you mean `{}`?",
                        from.name.cyan(),
                        package.name().cyan(),
                        command.bold(),
                    )?;
            }
            packages => {
                writeln!(
                    printer.stdout(),
                    "However, an executable with the name `{}` is available via the following dependencies::",
                    from.name.cyan(),
                )?;

                for package in packages {
                    writeln!(printer.stdout(), "- {}", package.name().cyan())?;
                }
                writeln!(
                    printer.stdout(),
                    "Did you mean to install one of them instead?"
                )?;
            }
        },
        Err(err) => {
            warn!("Failed to determine executables for packages: {err}");
        }
    }

    Ok(())
}

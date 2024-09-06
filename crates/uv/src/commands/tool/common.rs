use std::fmt::Write;
use std::path::PathBuf;
use std::{collections::BTreeSet, ffi::OsString};

use anyhow::{bail, Context};
use itertools::{Either, Itertools};
use owo_colors::OwoColorize;
use tracing::{debug, warn};

use distribution_types::{InstalledDist, Name};
use pep508_rs::PackageName;
use pypi_types::Requirement;
#[cfg(unix)]
use uv_fs::replace_symlink;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_python::PythonEnvironment;
use uv_settings::ToolOptions;
use uv_shell::Shell;
use uv_tool::{
    entrypoint_paths, find_executable_directory, find_manpage_directory, find_resources,
    InstalledTools, Resource, Tool, ToolEntrypoint, ToolManpage,
};
use uv_warnings::warn_user;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Return all packages which contain an executable with the given name.
pub(super) fn matching_packages(name: &str, site_packages: &SitePackages) -> Vec<InstalledDist> {
    site_packages
        .iter()
        .filter_map(|package| {
            entrypoint_paths(site_packages, package.name(), package.version())
                .ok()
                .and_then(|entrypoints| {
                    entrypoints
                        .iter()
                        .any(|entrypoint| {
                            entrypoint
                                .0
                                .strip_suffix(std::env::consts::EXE_SUFFIX)
                                .is_some_and(|stripped| stripped == name)
                        })
                        .then(|| package.clone())
                })
        })
        .collect()
}

/// Remove any resources attached to the [`Tool`].
pub(crate) fn remove_resources(tool: &Tool) {
    for executable in tool
        .entrypoints()
        .iter()
        .map(|entrypoint| &entrypoint.install_path)
    {
        debug!("Removing executable: `{}`", executable.simplified_display());
        if let Err(err) = fs_err::remove_file(executable) {
            warn!(
                "Failed to remove executable: `{}`: {err}",
                executable.simplified_display()
            );
        }
    }

    for manpage in tool.manpages().iter().map(|manpage| &manpage.install_path) {
        debug!("Removing manpage: `{}`", manpage.simplified_display());
        if let Err(err) = fs_err::remove_file(manpage) {
            warn!(
                "Failed to remove manpage: `{}`: {err}",
                manpage.simplified_display()
            );
        }
    }
}

/// Installs tool executables for a given package and handles any conflicts.
pub(crate) fn install_executables(
    environment: &PythonEnvironment,
    name: &PackageName,
    installed_tools: &InstalledTools,
    options: ToolOptions,
    force: bool,
    python: Option<String>,
    requirements: Vec<Requirement>,
    printer: Printer,
) -> anyhow::Result<ExitStatus> {
    let site_packages = SitePackages::from_environment(environment)?;
    let installed = site_packages.get_packages(name);
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
        &site_packages,
        installed_dist.name(),
        installed_dist.version(),
    )?;

    // Determine the entry points targets. Use a sorted collection for deterministic output.
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
            from = name.cyan()
        )?;

        hint_executable_from_dependency(name, &site_packages, printer)?;

        // Clean up the environment we just created.
        installed_tools.remove_environment(name)?;

        return Ok(ExitStatus::Failure);
    }

    // Check if they exist, before installing
    let mut existing_entry_points = target_entry_points
        .iter()
        .filter(|(_, _, target_path)| target_path.exists())
        .peekable();

    // Ignore any existing entrypoints if the user passed `--force`, or the existing recept was
    // broken.
    if force {
        for (name, _, target) in existing_entry_points {
            debug!("Removing existing executable: `{name}`");
            fs_err::remove_file(target)?;
        }
    } else if existing_entry_points.peek().is_some() {
        // Clean up the environment we just created
        installed_tools.remove_environment(name)?;

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

    debug!("Adding receipt for tool `{name}`");
    let tool = Tool::new(
        requirements.into_iter().collect(),
        python,
        target_entry_points
            .into_iter()
            .map(|(name, _, target_path)| ToolEntrypoint::new(name, target_path)),
        [].into_iter(),
        options,
    );
    installed_tools.add_tool_receipt(name, tool)?;

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
    name: &PackageName,
    site_packages: &SitePackages,
    printer: Printer,
) -> anyhow::Result<()> {
    let packages = matching_packages(name.as_ref(), site_packages);
    match packages.as_slice() {
        [] => {}
        [package] => {
            let command = format!("uv tool install {}", package.name());
            writeln!(
                printer.stdout(),
                "However, an executable with the name `{}` is available via dependency `{}`.\nDid you mean `{}`?",
                name.cyan(),
                package.name().cyan(),
                command.bold(),
            )?;
        }
        packages => {
            writeln!(
                printer.stdout(),
                "However, an executable with the name `{}` is available via the following dependencies::",
                name.cyan(),
            )?;

            for package in packages {
                writeln!(printer.stdout(), "- {}", package.name().cyan())?;
            }
            writeln!(
                printer.stdout(),
                "Did you mean to install one of them instead?"
            )?;
        }
    }

    Ok(())
}

pub(crate) fn install_resources(
    environment: &PythonEnvironment,
    name: &PackageName,
    installed_tools: &InstalledTools,
    options: ToolOptions,
    force: bool,
    python: Option<String>,
    requirements: Vec<Requirement>,
    printer: Printer,
) -> anyhow::Result<ExitStatus> {
    let site_packages = SitePackages::from_environment(environment)?;
    let installed = site_packages.get_packages(name);
    let Some(installed_dist) = installed.first().copied() else {
        bail!("Expected at least one requirement")
    };

    // Find a suitable path to install into
    let executable_directory = find_executable_directory()?;
    let manpage_directory = find_manpage_directory()?;
    fs_err::create_dir_all(&executable_directory)
        .context("Failed to create executable directory")?;

    debug!(
        "Installing tool executables into: {}",
        executable_directory.user_display()
    );

    // Determine the resource targets. Use a sorted collection for deterministic output.
    let (target_entry_points, target_man_pages): (BTreeSet<_>, BTreeSet<_>) = find_resources(
        &site_packages,
        installed_dist.name(),
        installed_dist.version(),
    )?
    .partition_map(|item| match item {
        Resource::Executable((name, source_path)) => {
            let target_path = executable_directory.join(
                source_path
                    .file_name()
                    .map(std::borrow::ToOwned::to_owned)
                    .unwrap_or_else(|| OsString::from(name.clone())),
            );
            Either::Left((name, source_path, target_path))
        }
        Resource::Manpage((name, source_path)) => {
            // SAFETY: We know the path has section and filename because we have checked it already
            let filename = source_path.file_name().expect("filename");
            let section = source_path.components().nth_back(1).expect("section");
            let target_path = manpage_directory.join(section).join(filename);
            Either::Right((name, source_path, target_path))
        }
    });

    if target_entry_points.is_empty() {
        writeln!(
            printer.stdout(),
            "No executables are provided by `{from}`",
            from = name.cyan()
        )?;

        hint_executable_from_dependency(name, &site_packages, printer)?;

        // Clean up the environment we just created.
        installed_tools.remove_environment(name)?;

        return Ok(ExitStatus::Failure);
    }

    // Check if executables exist, before installing
    check_existing_resource_targets(
        name,
        installed_tools,
        &target_entry_points,
        force,
        "executable",
    )?;

    // Check if manpages exist, before installing
    check_existing_resource_targets(name, installed_tools, &target_man_pages, force, "manpage")?;

    // Install executable targets
    install_resource_targets(&target_entry_points, "executable", printer)?;

    // Create dir for manpages
    for (dir, target_path) in target_man_pages
        .iter()
        .filter_map(|(_, _, target_path)| target_path.parent().map(|dir| (dir, target_path)))
    {
        fs_err::create_dir_all(dir).context(format!(
            "Failed to create a directory for the manpage target {}",
            target_path.simplified_display()
        ))?;
    }
    // Install manpage targets
    install_resource_targets(&target_man_pages, "manpage", printer)?;

    debug!("Adding receipt for tool `{name}`");
    let tool = Tool::new(
        requirements.into_iter().collect(),
        python,
        target_entry_points
            .into_iter()
            .map(|(name, _, target_path)| ToolEntrypoint::new(name, target_path)),
        target_man_pages
            .into_iter()
            .map(|(name, _, target_path)| ToolManpage::new(name, target_path)),
        options,
    );
    installed_tools.add_tool_receipt(name, tool)?;

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

fn check_existing_resource_targets(
    name: &PackageName,
    installed_tools: &InstalledTools,
    targets: &BTreeSet<(String, PathBuf, PathBuf)>,
    force: bool,
    resource_type: &str,
) -> anyhow::Result<()> {
    let mut existing_targets = targets
        .iter()
        .filter(|(_, _, target_path)| target_path.exists())
        .peekable();
    // Ignore any existing entrypoints if the user passed `--force`, or the existing recept was
    // broken.
    if force {
        for (name, _, target) in existing_targets {
            debug!("Removing existing {resource_type}: `{name}`");
            fs_err::remove_file(target)?;
        }
    } else if existing_targets.peek().is_some() {
        // Clean up the environment we just created
        installed_tools.remove_environment(name)?;

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
            "{}{}{s} already {exists}: {} (use `--force` to overwrite)",
            &resource_type[..1].to_ascii_uppercase(),
            &resource_type[1..],
            existing_targets.iter().map(|name| name.bold()).join(", ")
        )
    }
    Ok(())
}

fn install_resource_targets(
    targets: &BTreeSet<(String, PathBuf, PathBuf)>,
    resource_type: &str,
    printer: Printer,
) -> anyhow::Result<()> {
    for (name, source_path, target_path) in targets {
        debug!("Installing {resource_type}: `{name}`");
        #[cfg(unix)]
        replace_symlink(source_path, target_path).context("Failed to install {resource_type}")?;
        #[cfg(windows)]
        fs_err::copy(source_path, target_path).context("Failed to install entrypoint")?;
    }

    if !targets.is_empty() {
        let s = if targets.len() == 1 { "" } else { "s" };
        writeln!(
            printer.stderr(),
            "Installed {} {resource_type}{s}: {}",
            targets.len(),
            targets.iter().map(|(name, _, _)| name.bold()).join(", ")
        )?;
    }
    Ok(())
}

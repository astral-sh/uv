use anyhow::{bail, Context};
use itertools::Itertools;
use owo_colors::OwoColorize;
use std::collections::Bound;
use std::fmt::Write;
use std::{collections::BTreeSet, ffi::OsString};
use tracing::{debug, warn};
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_distribution_types::{InstalledDist, Name};
#[cfg(unix)]
use uv_fs::replace_symlink;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_pep440::{Version, VersionSpecifier, VersionSpecifiers};
use uv_pep508::PackageName;
use uv_pypi_types::Requirement;
use uv_python::{
    EnvironmentPreference, Interpreter, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest, PythonVariant, VersionRequest,
};
use uv_settings::{PythonInstallMirrors, ToolOptions};
use uv_shell::Shell;
use uv_tool::{entrypoint_paths, tool_executable_dir, InstalledTools, Tool, ToolEntrypoint};
use uv_warnings::warn_user;

use crate::commands::project::ProjectError;
use crate::commands::reporters::PythonDownloadReporter;
use crate::commands::{pip, ExitStatus};
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

/// Remove any entrypoints attached to the [`Tool`].
pub(crate) fn remove_entrypoints(tool: &Tool) {
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
}

/// Given a no-solution error and the [`Interpreter`] that was used during the solve, attempt to
/// discover an alternate [`Interpreter`] that satisfies the `requires-python` constraint.
pub(crate) async fn refine_interpreter(
    interpreter: &Interpreter,
    python_request: Option<&PythonRequest>,
    err: &pip::operations::Error,
    client_builder: &BaseClientBuilder<'_>,
    reporter: &PythonDownloadReporter,
    install_mirrors: &PythonInstallMirrors,
    python_preference: PythonPreference,
    python_downloads: PythonDownloads,
    cache: &Cache,
) -> anyhow::Result<Option<Interpreter>, ProjectError> {
    let pip::operations::Error::Resolve(uv_resolver::ResolveError::NoSolution(ref no_solution_err)) =
        err
    else {
        return Ok(None);
    };

    // Infer the `requires-python` constraint from the error.
    let requires_python = no_solution_err.find_requires_python();

    // If the existing interpreter already satisfies the `requires-python` constraint, we don't need
    // to refine it. We'd expect to fail again anyway.
    if requires_python.contains(interpreter.python_version()) {
        return Ok(None);
    }

    // If the user passed a `--python` request, and the refined interpreter is incompatible, we
    // can't use it.
    if let Some(python_request) = python_request {
        if !python_request.satisfied(interpreter, cache) {
            return Ok(None);
        }
    }

    // We want an interpreter that's as close to the required version as possible. If we choose the
    // "latest" Python, we risk choosing a version that lacks wheels for the tool's requirements
    // (assuming those requirements don't publish source distributions).
    //
    // TODO(charlie): Solve for the Python version iteratively (or even, within the resolver
    // itself). The current strategy can also fail if the tool's requirements have greater
    // `requires-python` constraints, and we didn't see them in the initial solve. It can also fail
    // if the tool's requirements don't publish wheels for this interpreter version, though that's
    // rarer.
    let lower_bound = match requires_python.as_ref() {
        Bound::Included(version) => VersionSpecifier::greater_than_equal_version(version.clone()),
        Bound::Excluded(version) => VersionSpecifier::greater_than_version(version.clone()),
        Bound::Unbounded => unreachable!("`requires-python` should never be unbounded"),
    };

    let upper_bound = match requires_python.as_ref() {
        Bound::Included(version) => {
            let major = version.release().first().copied().unwrap_or(0);
            let minor = version.release().get(1).copied().unwrap_or(0);
            VersionSpecifier::less_than_version(Version::new([major, minor + 1]))
        }
        Bound::Excluded(version) => {
            let major = version.release().first().copied().unwrap_or(0);
            let minor = version.release().get(1).copied().unwrap_or(0);
            VersionSpecifier::less_than_version(Version::new([major, minor + 1]))
        }
        Bound::Unbounded => unreachable!("`requires-python` should never be unbounded"),
    };

    let python_request = PythonRequest::Version(VersionRequest::Range(
        VersionSpecifiers::from_iter([lower_bound, upper_bound]),
        PythonVariant::default(),
    ));

    debug!("Refining interpreter with: {python_request}");

    let interpreter = PythonInstallation::find_or_download(
        Some(&python_request),
        EnvironmentPreference::OnlySystem,
        python_preference,
        python_downloads,
        client_builder,
        cache,
        Some(reporter),
        install_mirrors.python_install_mirror.as_deref(),
        install_mirrors.pypy_install_mirror.as_deref(),
    )
    .await?
    .into_interpreter();

    Ok(Some(interpreter))
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
    constraints: Vec<Requirement>,
    overrides: Vec<Requirement>,
    printer: Printer,
) -> anyhow::Result<ExitStatus> {
    let site_packages = SitePackages::from_environment(environment)?;
    let installed = site_packages.get_packages(name);
    let Some(installed_dist) = installed.first().copied() else {
        bail!("Expected at least one requirement")
    };

    // Find a suitable path to install into
    let executable_directory = tool_executable_dir()?;
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

    // Error if we're overwriting an existing entrypoint, unless the user passed `--force`.
    if !force {
        let mut existing_entry_points = target_entry_points
            .iter()
            .filter(|(_, _, target_path)| target_path.exists())
            .peekable();
        if existing_entry_points.peek().is_some() {
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
    }

    #[cfg(windows)]
    let itself = std::env::current_exe().ok();

    for (name, source_path, target_path) in &target_entry_points {
        debug!("Installing executable: `{name}`");

        #[cfg(unix)]
        replace_symlink(source_path, target_path).context("Failed to install executable")?;

        #[cfg(windows)]
        if itself.as_ref().is_some_and(|itself| {
            std::path::absolute(target_path).is_ok_and(|target| *itself == target)
        }) {
            self_replace::self_replace(source_path).context("Failed to install entrypoint")?;
        } else {
            fs_err::copy(source_path, target_path).context("Failed to install entrypoint")?;
        }
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
        requirements,
        constraints,
        overrides,
        python,
        target_entry_points
            .into_iter()
            .map(|(name, _, target_path)| ToolEntrypoint::new(name, target_path)),
        options,
    );
    installed_tools.add_tool_receipt(name, tool)?;

    // If the executable directory isn't on the user's PATH, warn.
    if !Shell::contains_path(&executable_directory) {
        if let Some(shell) = Shell::from_env() {
            if let Some(command) = shell.prepend_path(&executable_directory) {
                if shell.supports_update() {
                    warn_user!(
                        "`{}` is not on your PATH. To use installed tools, run `{}` or `{}`.",
                        executable_directory.simplified_display().cyan(),
                        command.green(),
                        "uv tool update-shell".green()
                    );
                } else {
                    warn_user!(
                        "`{}` is not on your PATH. To use installed tools, run `{}`.",
                        executable_directory.simplified_display().cyan(),
                        command.green()
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

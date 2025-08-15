use anyhow::{Context, bail};
use itertools::Itertools;
use owo_colors::OwoColorize;
use std::{
    collections::{BTreeSet, Bound},
    ffi::OsString,
    fmt::Write,
    path::Path,
};
use tracing::{debug, warn};
use uv_cache::Cache;
use uv_client::BaseClientBuilder;
use uv_configuration::Preview;
use uv_distribution_types::Requirement;
use uv_distribution_types::{InstalledDist, Name};
use uv_fs::Simplified;
#[cfg(unix)]
use uv_fs::replace_symlink;
use uv_installer::SitePackages;
use uv_pep440::{Version, VersionSpecifier, VersionSpecifiers};
use uv_pep508::PackageName;
use uv_python::{
    EnvironmentPreference, Interpreter, PythonDownloads, PythonEnvironment, PythonInstallation,
    PythonPreference, PythonRequest, PythonVariant, VersionRequest,
};
use uv_settings::{PythonInstallMirrors, ToolOptions};
use uv_shell::Shell;
use uv_tool::{InstalledTools, Tool, ToolEntrypoint, entrypoint_paths};
use uv_warnings::warn_user_once;

use crate::commands::pip;
use crate::commands::project::ProjectError;
use crate::commands::reporters::PythonDownloadReporter;
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
    preview: Preview,
) -> anyhow::Result<Option<Interpreter>, ProjectError> {
    let pip::operations::Error::Resolve(uv_resolver::ResolveError::NoSolution(no_solution_err)) =
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

    let requires_python_request = PythonRequest::Version(VersionRequest::Range(
        VersionSpecifiers::from_iter([lower_bound, upper_bound]),
        PythonVariant::default(),
    ));

    debug!("Refining interpreter with: {requires_python_request}");

    let interpreter = PythonInstallation::find_or_download(
        Some(&requires_python_request),
        EnvironmentPreference::OnlySystem,
        python_preference,
        python_downloads,
        client_builder,
        cache,
        Some(reporter),
        install_mirrors.python_install_mirror.as_deref(),
        install_mirrors.pypy_install_mirror.as_deref(),
        install_mirrors.python_downloads_json_url.as_deref(),
        preview,
    )
    .await?
    .into_interpreter();

    // If the user passed a `--python` request, and the refined interpreter is incompatible, we
    // can't use it.
    if let Some(python_request) = python_request {
        if !python_request.satisfied(&interpreter, cache) {
            return Ok(None);
        }
    }

    Ok(Some(interpreter))
}

/// Finalizes a tool installation, after creation of an environment.
///
/// Installs tool executables for a given package, handling any conflicts.
///
/// Adds a receipt for the tool.
pub(crate) fn finalize_tool_install(
    environment: &PythonEnvironment,
    name: &PackageName,
    entrypoints: &[PackageName],
    installed_tools: &InstalledTools,
    options: &ToolOptions,
    force: bool,
    python: Option<PythonRequest>,
    requirements: Vec<Requirement>,
    constraints: Vec<Requirement>,
    overrides: Vec<Requirement>,
    build_constraints: Vec<Requirement>,
    printer: Printer,
) -> anyhow::Result<()> {
    let executable_directory = uv_tool::tool_executable_dir()?;
    fs_err::create_dir_all(&executable_directory)
        .context("Failed to create executable directory")?;
    debug!(
        "Installing tool executables into: {}",
        executable_directory.user_display()
    );

    let mut installed_entrypoints = Vec::new();
    let site_packages = SitePackages::from_environment(environment)?;
    let ordered_packages = entrypoints
        // Install dependencies first
        .iter()
        .filter(|pkg| *pkg != name)
        .collect::<BTreeSet<_>>()
        // Then install the root package last
        .into_iter()
        .chain(std::iter::once(name));

    for package in ordered_packages {
        if package == name {
            debug!("Installing entrypoints for tool `{package}`");
        } else {
            debug!("Installing entrypoints for `{package}` as part of tool `{name}`");
        }

        let installed = site_packages.get_packages(package);
        let dist = installed
            .first()
            .context("Expected at least one requirement")?;
        let dist_entrypoints = entrypoint_paths(&site_packages, dist.name(), dist.version())?;

        // Determine the entry points targets. Use a sorted collection for deterministic output.
        let target_entrypoints = dist_entrypoints
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

        if target_entrypoints.is_empty() {
            // If package is not the root package, suggest to install it as a dependency.
            if package != name {
                writeln!(
                    printer.stdout(),
                    "No executables are provided by package `{}`\n{}{} Use `--with {}` to include `{}` as a dependency without installing its executables.",
                    package.cyan(),
                    "hint".bold().cyan(),
                    ":".bold(),
                    package.cyan(),
                    package.cyan(),
                )?;
                continue;
            }

            // For the root package, this is a fatal error
            writeln!(
                printer.stdout(),
                "No executables are provided by package `{}`; removing tool",
                package.cyan()
            )?;

            hint_executable_from_dependency(package, &site_packages, printer)?;

            // Clean up the environment we just created.
            installed_tools.remove_environment(name)?;

            return Err(anyhow::anyhow!(
                "Failed to install entrypoints for `{}`",
                package.cyan()
            ));
        }

        // Error if we're overwriting an existing entrypoint, unless the user passed `--force`.
        if !force {
            let mut existing_entrypoints = target_entrypoints
                .iter()
                .filter(|(_, _, target_path)| target_path.exists())
                .peekable();
            if existing_entrypoints.peek().is_some() {
                // Clean up the environment we just created
                installed_tools.remove_environment(name)?;

                let existing_entrypoints = existing_entrypoints
                    // SAFETY: We know the target has a filename because we just constructed it above
                    .map(|(_, _, target)| target.file_name().unwrap().to_string_lossy())
                    .collect::<Vec<_>>();
                let (s, exists) = if existing_entrypoints.len() == 1 {
                    ("", "exists")
                } else {
                    ("s", "exist")
                };
                bail!(
                    "Executable{s} already {exists}: {} (use `--force` to overwrite)",
                    existing_entrypoints
                        .iter()
                        .map(|name| name.bold())
                        .join(", ")
                )
            }
        }

        #[cfg(windows)]
        let itself = std::env::current_exe().ok();

        let mut names = BTreeSet::new();
        for (name, src, target) in target_entrypoints {
            debug!("Installing executable: `{name}`");

            #[cfg(unix)]
            replace_symlink(src, &target).context("Failed to install executable")?;

            #[cfg(windows)]
            if itself.as_ref().is_some_and(|itself| {
                std::path::absolute(&target).is_ok_and(|target| *itself == target)
            }) {
                self_replace::self_replace(src).context("Failed to install entrypoint")?;
            } else {
                fs_err::copy(src, &target).context("Failed to install entrypoint")?;
            }

            let tool_entry = ToolEntrypoint::new(&name, target, package.to_string());
            names.insert(tool_entry.name.clone());
            installed_entrypoints.push(tool_entry);
        }

        let s = if names.len() == 1 { "" } else { "s" };
        let from_pkg = if name == package {
            String::new()
        } else {
            format!(" from `{package}`")
        };
        writeln!(
            printer.stderr(),
            "Installed {} executable{s}{from_pkg}: {}",
            names.len(),
            names.iter().map(|name| name.bold()).join(", ")
        )?;
    }

    debug!("Adding receipt for tool `{name}`");
    let tool = Tool::new(
        requirements,
        constraints,
        overrides,
        build_constraints,
        python,
        installed_entrypoints,
        options.clone(),
    );
    installed_tools.add_tool_receipt(name, tool)?;

    warn_out_of_path(&executable_directory);

    Ok(())
}

fn warn_out_of_path(executable_directory: &Path) {
    // If the executable directory isn't on the user's PATH, warn.
    if !Shell::contains_path(executable_directory) {
        if let Some(shell) = Shell::from_env() {
            if let Some(command) = shell.prepend_path(executable_directory) {
                if shell.supports_update() {
                    warn_user_once!(
                        "`{}` is not on your PATH. To use installed tools, run `{}` or `{}`.",
                        executable_directory.simplified_display().cyan(),
                        command.green(),
                        "uv tool update-shell".green()
                    );
                } else {
                    warn_user_once!(
                        "`{}` is not on your PATH. To use installed tools, run `{}`.",
                        executable_directory.simplified_display().cyan(),
                        command.green()
                    );
                }
            } else {
                warn_user_once!(
                    "`{}` is not on your PATH. To use installed tools, add the directory to your PATH.",
                    executable_directory.simplified_display().cyan(),
                );
            }
        } else {
            warn_user_once!(
                "`{}` is not on your PATH. To use installed tools, add the directory to your PATH.",
                executable_directory.simplified_display().cyan(),
            );
        }
    }
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
                "{}{} An executable with the name `{}` is available via dependency `{}`.\n      Did you mean `{}`?",
                "hint".bold().cyan(),
                ":".bold(),
                name.cyan(),
                package.name().cyan(),
                command.bold(),
            )?;
        }
        packages => {
            writeln!(
                printer.stdout(),
                "{}{} An executable with the name `{}` is available via the following dependencies::",
                "hint".bold().cyan(),
                ":".bold(),
                name.cyan(),
            )?;

            for package in packages {
                writeln!(printer.stdout(), "- {}", package.name().cyan())?;
            }
            writeln!(
                printer.stdout(),
                "      Did you mean to install one of them instead?"
            )?;
        }
    }

    Ok(())
}

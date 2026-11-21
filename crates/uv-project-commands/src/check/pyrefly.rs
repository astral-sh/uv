use std::ffi::{OsStr, OsString};
use std::fmt::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::process::Command;

use uv_command_support::{ExitStatus, Printer, child::run_to_completion};
use uv_configuration::ColorChoice;
use uv_pep440::Version;
use uv_settings::{FrozenSource, LockCheck};
use uv_shell::shlex_posix;
use uv_static::EnvVars;

/// Run a type check powered by Pyrefly.
///
/// Pyrefly checks each directory that it is given as a project, with the includes and excludes of
/// the config that governs the directory. A directory that is given explicitly is checked even if
/// a config excludes it, unless uv selects the members itself and passes `--force-exclude`, as it
/// does for ty.
///
/// Pyrefly resolves imports in the environment that `VIRTUAL_ENV` selects. For each PEP 723
/// script that it checks, Pyrefly runs `uv workspace metadata --script` to find the script's
/// environment.
pub(super) async fn run(
    pyrefly_path: &Path,
    target_dir: &Path,
    lock_check: LockCheck,
    frozen: Option<FrozenSource>,
    check_targets: &[PathBuf],
    excluded_targets: &[PathBuf],
    explicit_targets: bool,
    venv_path: Option<&Path>,
    python_version: Option<&Version>,
    show_version: bool,
    show_command: bool,
    color: ColorChoice,
    printer: Printer,
) -> Result<ExitStatus> {
    if show_version {
        let output = Command::new(pyrefly_path)
            .arg("--version")
            .output()
            .await
            .context("Failed to query Pyrefly version")?;
        if !output.status.success() {
            anyhow::bail!("Failed to query Pyrefly version");
        }
        writeln!(
            printer.stderr(),
            "Using {}",
            String::from_utf8_lossy(&output.stdout).trim()
        )?;
    }

    let mut command = Command::new(pyrefly_path);
    command.current_dir(target_dir);
    command.arg("check");
    command.arg("--color").arg(color.as_str());
    if let Some(python_version) = python_version {
        command
            .arg("--python-version")
            .arg(python_version.to_string());
    }
    // Pyrefly runs this command, followed by the script's path, to find the environment of each
    // PEP 723 script that it checks.
    let uv = std::env::current_exe().context("Failed to find the uv executable")?;
    for argument in [
        uv.as_os_str(),
        OsStr::new("workspace"),
        OsStr::new("metadata"),
        OsStr::new("--preview-features"),
        OsStr::new("workspace-metadata"),
        OsStr::new("--script"),
    ] {
        let mut flag = OsString::from("--script-command=");
        flag.push(argument);
        command.arg(flag);
    }
    // PEP 723 scripts have independent environments and must be checked explicitly with
    // `uv check --script`. This still allows explicitly selected script paths to be checked.
    command.arg("--exclude-scripts");
    // Unlike `--project-excludes`, `--extend-project-excludes` keeps the excludes of the config.
    for excluded_target in excluded_targets {
        command.arg("--extend-project-excludes");
        command.arg(
            excluded_target
                .strip_prefix(target_dir)
                .unwrap_or(excluded_target),
        );
    }
    if !check_targets.is_empty() {
        // Respect configured exclusions when automatically selecting members of a virtual workspace.
        if !explicit_targets {
            command.arg("--force-exclude");
        }
        // Keep paths relative to the working directory for stable diagnostics, and use `--` so
        // option-like filenames are treated as paths.
        command.arg("--");
        for check_target in check_targets {
            let relative_target = check_target
                .strip_prefix(target_dir)
                .unwrap_or(check_target);
            // Pyrefly rejects an empty path, which is what the working directory strips to.
            if relative_target.as_os_str().is_empty() {
                command.arg(".");
            } else {
                command.arg(relative_target);
            }
        }
    }

    // Forward enabled settings and remove disabled ones so that CLI overrides of inherited
    // settings also apply when Pyrefly invokes `uv workspace metadata`.
    if frozen.is_some() {
        command.env(EnvVars::UV_FROZEN, "1");
    } else {
        command.env_remove(EnvVars::UV_FROZEN);
    }
    match lock_check {
        LockCheck::Enabled(_) => command.env(EnvVars::UV_LOCKED, "1"),
        LockCheck::Disabled => command.env_remove(EnvVars::UV_LOCKED),
    };

    if let Some(venv_path) = venv_path {
        command.env(EnvVars::VIRTUAL_ENV, venv_path);
    }

    if show_command {
        let mut stderr = printer.stderr_important();
        write!(stderr, "Running `pyrefly")?;
        for argument in command.as_std().get_args() {
            write!(stderr, " {}", shlex_posix(argument))?;
        }
        writeln!(stderr, "`")?;
    }

    let handle = command.spawn().context("Failed to spawn `pyrefly check`")?;
    run_to_completion(handle).await
}

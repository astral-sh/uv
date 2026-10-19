#![cfg_attr(windows, allow(unreachable_code))]

use std::fmt::Write;
use std::path::Path;

use anyhow::Result;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_fs::Simplified;
use uv_shell::{ConfigurationUpdate, Shell, update_configuration_file};

use crate::ExitStatus;
use crate::Printer;

/// Ensure that an executable directory is in PATH.
pub async fn update_shell(executable_directory: &Path, printer: Printer) -> Result<ExitStatus> {
    debug!(
        "Ensuring that the executable directory is in PATH: {}",
        executable_directory.simplified_display()
    );

    #[cfg(windows)]
    {
        if uv_shell::prepend_path(executable_directory)? {
            writeln!(
                printer.stderr(),
                "Updated PATH to include executable directory {}",
                executable_directory.simplified_display().cyan()
            )?;
            writeln!(printer.stderr(), "Restart your shell to apply changes")?;
        } else {
            writeln!(
                printer.stderr(),
                "Executable directory {} is already in PATH",
                executable_directory.simplified_display().cyan()
            )?;
        }

        return Ok(ExitStatus::Success);
    }

    if Shell::contains_path(executable_directory) {
        writeln!(
            printer.stderr(),
            "Executable directory {} is already in PATH",
            executable_directory.simplified_display().cyan()
        )?;
        return Ok(ExitStatus::Success);
    }

    // Determine the current shell.
    let Some(shell) = Shell::from_env() else {
        return Err(anyhow::anyhow!(
            "The executable directory `{}` is not in PATH, but the current shell could not be determined",
            executable_directory.simplified_display().cyan()
        ));
    };

    // Look up the configuration files (e.g., `.bashrc`, `.zshrc`) for the shell.
    let files = shell.configuration_files();
    if files.is_empty() {
        return Err(anyhow::anyhow!(
            "The executable directory `{}` is not in PATH, but updating {shell} is currently unsupported",
            executable_directory.simplified_display().cyan()
        ));
    }

    // Prepare the command (e.g., `export PATH="$HOME/.cargo/bin:$PATH"`).
    let Some(command) = shell.prepend_path(executable_directory) else {
        return Err(anyhow::anyhow!(
            "The executable directory `{}` is not in PATH, but the necessary command to update {shell} could not be determined",
            executable_directory.simplified_display().cyan()
        ));
    };

    // Update each file, as necessary.
    let mut updated = false;
    for file in files {
        match update_configuration_file(&file, &command).await? {
            ConfigurationUpdate::Updated => {
                writeln!(
                    printer.stderr(),
                    "Updated configuration file: {}",
                    file.simplified_display().cyan()
                )?;
                updated = true;
            }
            ConfigurationUpdate::Created => {
                writeln!(
                    printer.stderr(),
                    "Created configuration file: {}",
                    file.simplified_display().cyan()
                )?;
                updated = true;
            }
            ConfigurationUpdate::Unchanged => {}
        }
    }

    if updated {
        writeln!(printer.stderr(), "Restart your shell to apply changes")?;
        Ok(ExitStatus::Success)
    } else {
        Err(anyhow::anyhow!(
            "The executable directory `{}` is not in PATH, but the {shell} configuration files are already up-to-date",
            executable_directory.simplified_display().cyan()
        ))
    }
}

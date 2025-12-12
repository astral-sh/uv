#![cfg_attr(windows, allow(unreachable_code))]

use std::fmt::Write;

use anyhow::Result;
use owo_colors::OwoColorize;
use tokio::io::AsyncWriteExt;
use tracing::debug;

use uv_cli::PythonUpdateShellArgs;
use uv_fs::Simplified;
use uv_python::managed::python_executable_dir;
use uv_shell::Shell;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Ensure that the executable directory is in PATH.
pub(crate) async fn update_shell(
    args: PythonUpdateShellArgs,
    printer: Printer,
) -> Result<ExitStatus> {
    let executable_directory = python_executable_dir()?;
    debug!(
        "Ensuring that the executable directory is in PATH: {}",
        executable_directory.simplified_display()
    );

    #[cfg(windows)]
    {
        // Check if path is in PATH (read-only, doesn't mutate)
        let is_in_path = uv_shell::windows::contains_path(&executable_directory)?;

        if is_in_path && !args.force {
            // Already in PATH and not forcing - exit early
            writeln!(
                printer.stderr(),
                "Executable directory {} is already in PATH",
                executable_directory.simplified_display().cyan()
            )?;
            return Ok(ExitStatus::Success);
        }

        if is_in_path && args.force {
            // Already in PATH but forcing - remove it first, then prepend
            uv_shell::windows::force_prepend_path(&executable_directory)?;

            writeln!(
                printer.stderr(),
                "Force updated PATH to prioritize executable directory {}",
                executable_directory.simplified_display().cyan()
            )?;
            writeln!(printer.stderr(), "Restart your shell to apply changes")?;
        } else if !is_in_path {
            // Not in PATH - normal prepend
            uv_shell::windows::prepend_path(&executable_directory)?;
            writeln!(
                printer.stderr(),
                "Updated PATH to include executable directory {}",
                executable_directory.simplified_display().cyan()
            )?;
            writeln!(printer.stderr(), "Restart your shell to apply changes")?;
        }

        return Ok(ExitStatus::Success);
    }

    if Shell::contains_path(&executable_directory) && !args.force {
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
            "The executable directory {} is not in PATH, but the current shell could not be determined",
            executable_directory.simplified_display().cyan()
        ));
    };

    // Look up the configuration files (e.g., `.bashrc`, `.zshrc`) for the shell.
    let files = shell.configuration_files();
    if files.is_empty() {
        return Err(anyhow::anyhow!(
            "The executable directory {} is not in PATH, but updating {shell} is currently unsupported",
            executable_directory.simplified_display().cyan()
        ));
    }

    // Prepare the command (e.g., `export PATH="$HOME/.cargo/bin:$PATH"`).
    let Some(command) = shell.prepend_path(&executable_directory) else {
        return Err(anyhow::anyhow!(
            "The executable directory {} is not in PATH, but the necessary command to update {shell} could not be determined",
            executable_directory.simplified_display().cyan()
        ));
    };

    // Update each file, as necessary.
    let mut updated = false;
    for file in files {
        // Search for the command in the file, to avoid redundant updates.
        match fs_err::tokio::read_to_string(&file).await {
            Ok(contents) => {
                // Check if command already exists
                let command_exists = contents
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.starts_with('#'))
                    .any(|line| line.contains(&command));

                if command_exists {
                    if args.force {
                        // With --force: Remove old entry and add it again at the end (ensures first priority)
                        let new_contents: String = contents
                            .lines()
                            .filter(|line| {
                                let trimmed = line.trim();
                                // Remove old # uv comments and lines containing the command
                                !(trimmed == "# uv"
                                    || (!trimmed.starts_with('#') && trimmed.contains(&command)))
                            })
                            .collect::<Vec<_>>()
                            .join("\n");

                        // Add the command at the end to ensure it's executed last (first in PATH)
                        let final_contents = format!("{new_contents}\n# uv\n{command}\n");

                        fs_err::tokio::OpenOptions::new()
                            .create(true)
                            .truncate(true)
                            .write(true)
                            .open(&file)
                            .await?
                            .write_all(final_contents.as_bytes())
                            .await?;

                        writeln!(
                            printer.stderr(),
                            "Force updated configuration file: {}",
                            file.simplified_display().cyan()
                        )?;
                        updated = true;
                    } else {
                        // Without --force: Skip if already exists
                        debug!(
                            "Skipping already-updated configuration file: {}",
                            file.simplified_display()
                        );
                    }
                } else {
                    // Command doesn't exist: Add it normally
                    fs_err::tokio::OpenOptions::new()
                        .create(true)
                        .truncate(true)
                        .write(true)
                        .open(&file)
                        .await?
                        .write_all(format!("{contents}\n# uv\n{command}\n").as_bytes())
                        .await?;

                    writeln!(
                        printer.stderr(),
                        "Updated configuration file: {}",
                        file.simplified_display().cyan()
                    )?;
                    updated = true;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                // Ensure that the directory containing the file exists.
                if let Some(parent) = file.parent() {
                    fs_err::tokio::create_dir_all(&parent).await?;
                }

                // Append the command to the file.
                fs_err::tokio::OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(&file)
                    .await?
                    .write_all(format!("# uv\n{command}\n").as_bytes())
                    .await?;

                writeln!(
                    printer.stderr(),
                    "Created configuration file: {}",
                    file.simplified_display().cyan()
                )?;
                updated = true;
            }
            Err(err) => {
                return Err(err.into());
            }
        }
    }

    if updated {
        writeln!(printer.stderr(), "Restart your shell to apply changes")?;
        Ok(ExitStatus::Success)
    } else {
        Err(anyhow::anyhow!(
            "The executable directory {} is not in PATH, but the {shell} configuration files are already up-to-date",
            executable_directory.simplified_display().cyan()
        ))
    }
}

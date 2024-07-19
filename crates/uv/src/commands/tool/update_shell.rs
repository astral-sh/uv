#![cfg_attr(windows, allow(unreachable_code))]

use std::fmt::Write;

use anyhow::Result;
use owo_colors::OwoColorize;
use tokio::io::AsyncWriteExt;
use tracing::debug;

use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_shell::Shell;
use uv_tool::find_executable_directory;
use uv_warnings::warn_user_once;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Ensure that the executable directory is in PATH.
pub(crate) async fn update_shell(preview: PreviewMode, printer: Printer) -> Result<ExitStatus> {
    if preview.is_disabled() {
        warn_user_once!("`uv tool update-shell` is experimental and may change without warning");
    }

    let executable_directory = find_executable_directory()?;
    debug!(
        "Ensuring that the executable directory is in PATH: {}",
        executable_directory.simplified_display()
    );

    #[cfg(windows)]
    {
        if uv_shell::windows::prepend_path(&executable_directory)? {
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

    if Shell::contains_path(&executable_directory) {
        writeln!(
            printer.stderr(),
            "Executable directory {} is already in PATH",
            executable_directory.simplified_display().cyan()
        )?;
        Ok(ExitStatus::Success)
    } else {
        // Determine the current shell.
        let Some(shell) = Shell::from_env() else {
            return Err(anyhow::anyhow!("The executable directory {} is not in PATH, but the current shell could not be determined", executable_directory.simplified_display().cyan()));
        };

        // Look up the configuration files (e.g., `.bashrc`, `.zshrc`) for the shell.
        let files = shell.configuration_files();
        if files.is_empty() {
            return Err(anyhow::anyhow!("The executable directory {} is not in PATH, but updating {shell} is currently unsupported", executable_directory.simplified_display().cyan()));
        }

        // Prepare the command (e.g., `export PATH="$HOME/.cargo/bin:$PATH"`).
        let Some(command) = shell.prepend_path(&executable_directory) else {
            return Err(anyhow::anyhow!("The executable directory {} is not in PATH, but the necessary command to update {shell} could not be determined", executable_directory.simplified_display().cyan()));
        };

        // Update each file, as necessary.
        let mut updated = false;
        for file in files {
            // Search for the command in the file, to avoid redundant updates.
            match fs_err::tokio::read_to_string(&file).await {
                Ok(contents) => {
                    if contents.contains(&command) {
                        debug!(
                            "Skipping already-updated configuration file: {}",
                            file.simplified_display()
                        );
                        continue;
                    }

                    // Append the command to the file.
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
            Err(anyhow::anyhow!("The executable directory {} is not in PATH, but the {shell} configuration files are already up-to-date", executable_directory.simplified_display().cyan()))
        }
    }
}

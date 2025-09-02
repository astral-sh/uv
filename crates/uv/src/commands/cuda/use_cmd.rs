use std::fmt::Write;
use std::fs;
use std::env;
use std::path::{Path, PathBuf};

use anyhow::Result;
use owo_colors::OwoColorize;

use uv_cuda::{CudaVersion, ManagedCudaInstallations};

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// set the active CUDA version by displaying environment variable setup
#[allow(unsafe_code)]
pub(crate) async fn cuda_use(
    version_str: String,
    printer: Printer,
) -> Result<ExitStatus> {
    let version = version_str.parse::<CudaVersion>()
        .map_err(|e| anyhow::anyhow!("Invalid CUDA version '{}': {}", version_str, e))?;

    let installations = ManagedCudaInstallations::from_settings(None)?;

    match installations.find_version(&version)? {
        Some(installation) => {
            if !installation.is_valid() {
                writeln!(
                    printer.stderr(),
                    "CUDA {} installation at {} appears to be corrupted",
                    version.red(),
                    installation.path().display()
                )?;
                writeln!(printer.stderr(), "Try reinstalling with: {}",
                        format!("uv cuda install {} --force", version).bold())?;
                return Ok(ExitStatus::Failure);
            }

            // generate and save environment file
            let env_file_path = installation.env_file_path();
            let env_content = installation.generate_env_file();

            fs::write(&env_file_path, &env_content)
                .map_err(|e| anyhow::anyhow!("Failed to write environment file: {}", e))?;

            // detect shell and RC file
            let (_shell_name, rc_file_path) = detect_shell_and_rc()?;

            // add source command to RC file if not already present
            let source_line = format!("source {}", env_file_path.display());
            if let Some(ref rc_path) = rc_file_path {
                if !rc_file_contains_line(rc_path, &source_line)? {
                    append_to_rc_file(rc_path, &source_line)?;
                    writeln!(printer.stderr(), "Added CUDA environment to {}", rc_path.display())?;
                }
            }

            writeln!(printer.stderr(), "CUDA {} activated", version.cyan())?;
            writeln!(printer.stderr(), "")?;
            writeln!(printer.stderr(), "To activate in new terminal sessions, either:")?;
            writeln!(printer.stderr(), "  • Restart your terminal")?;
            if let Some(ref rc_path) = rc_file_path {
                writeln!(printer.stderr(), "  • Run: {}", format!("source {}", rc_path.display()).bold())?;
            }
        }
        None => {
            writeln!(
                printer.stderr(),
                "CUDA {} is not installed",
                version.yellow()
            )?;
            writeln!(printer.stderr(), "Install it with: {}",
                    format!("uv cuda install {}", version).bold())?;
            return Ok(ExitStatus::Failure);
        }
    }

    Ok(ExitStatus::Success)
}

/// detect the current shell and its RC file path
fn detect_shell_and_rc() -> Result<(String, Option<PathBuf>)> {
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let home = env::var("HOME").map_err(|_| anyhow::anyhow!("HOME environment variable not set"))?;

    let shell_name = if shell.contains("bash") {
        "bash"
    } else if shell.contains("zsh") {
        "zsh"
    } else if shell.contains("fish") {
        "fish"
    } else if shell.contains("tcsh") {
        "tcsh"
    } else {
        "bash" // fallback
    };

    let rc_file = match shell_name {
        "bash" => Some(PathBuf::from(format!("{}/.bashrc", home))),
        "zsh" => Some(PathBuf::from(format!("{}/.zshrc", home))),
        "fish" => Some(PathBuf::from(format!("{}/.config/fish/config.fish", home))),
        "tcsh" => Some(PathBuf::from(format!("{}/.tcshrc", home))),
        _ => None,
    };

    Ok((shell_name.to_string(), rc_file))
}

/// check if a line already exists in the RC file
fn rc_file_contains_line(rc_path: &Path, line: &str) -> Result<bool> {
    if !rc_path.exists() {
        return Ok(false);
    }

    let content = fs::read_to_string(rc_path)?;
    Ok(content.lines().any(|l| l.trim() == line.trim()))
}

/// append a line to the RC file
fn append_to_rc_file(rc_path: &Path, line: &str) -> Result<()> {
    // create parent directory if it doesn't exist (for fish config)
    if let Some(parent) = rc_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut content = if rc_path.exists() {
        fs::read_to_string(rc_path)?
    } else {
        String::new()
    };

    // add newline if content doesn't end with one
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }

    content.push_str(line);
    content.push('\n');

    fs::write(rc_path, content)?;
    Ok(())
}
//! Utilities for running executables and scripts. Particularly in Windows.

use std::env::consts::EXE_EXTENSION;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

#[derive(Debug)]
pub enum WindowsRunnable {
    /// Windows PE (.exe)
    Executable,
    /// `PowerShell` script (.ps1)
    PowerShell,
    /// Command Prompt NT script (.cmd)
    Command,
    /// Command Prompt script (.bat)
    Batch,
}

impl WindowsRunnable {
    /// Returns a list of all supported Windows runnable types.
    fn all() -> &'static [Self] {
        &[
            Self::Executable,
            Self::PowerShell,
            Self::Command,
            Self::Batch,
        ]
    }

    /// Returns the extension for a given Windows runnable type.
    fn to_extension(&self) -> &'static str {
        match self {
            Self::Executable => EXE_EXTENSION,
            Self::PowerShell => "ps1",
            Self::Command => "cmd",
            Self::Batch => "bat",
        }
    }

    /// Determines the runnable type from a given Windows file extension.
    fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            EXE_EXTENSION => Some(Self::Executable),
            "ps1" => Some(Self::PowerShell),
            "cmd" => Some(Self::Command),
            "bat" => Some(Self::Batch),
            _ => None,
        }
    }

    /// Returns a [`Command`] to run the given type under the appropriate Windows runtime.
    fn as_command(&self, runnable_path: &Path) -> Command {
        match self {
            Self::Executable => Command::new(runnable_path),
            Self::PowerShell => {
                let mut cmd = Command::new("powershell");
                cmd.arg("-NoLogo").arg("-File").arg(runnable_path);
                cmd
            }
            Self::Command | Self::Batch => {
                let mut cmd = Command::new("cmd");
                cmd.arg("/q").arg("/c").arg(runnable_path);
                cmd
            }
        }
    }

    /// Handle console and legacy setuptools scripts for Windows.
    ///
    /// Returns [`Command`] that can be used to invoke a supported runnable on Windows
    /// under the scripts path of an interpreter environment.
    pub fn from_script_path(script_path: &Path, runnable_name: &OsStr) -> Command {
        let script_path = script_path.join(runnable_name);

        // Honor explicit extension if provided and recognized.
        if let Some(script_type) = script_path
            .extension()
            .and_then(OsStr::to_str)
            .and_then(Self::from_extension)
            .filter(|_| script_path.is_file())
        {
            return script_type.as_command(&script_path);
        }

        // Guess the extension when an explicit one is not provided.
        // We also add the extension when missing since for some types (e.g. PowerShell) it must be explicit.
        Self::all()
            .iter()
            .map(|script_type| {
                (
                    script_type,
                    script_path.with_extension(script_type.to_extension()),
                )
            })
            .find(|(_, script_path)| script_path.is_file())
            .map(|(script_type, script_path)| script_type.as_command(&script_path))
            .unwrap_or_else(|| Command::new(runnable_name))
    }
}

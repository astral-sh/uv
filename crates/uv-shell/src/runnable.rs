//! Utilities for running executables and scripts. Particularly in Windows.

use std::env::consts::EXE_EXTENSION;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Helper function to add an extension to a path by appending it instead of replacing.
///
/// This mimics the behavior of the upcoming stable `with_added_extension` method for future compatibility.
/// When `Path::with_added_extension` stabilizes, this function can be replaced with:
/// ```ignore
/// fn add_extension_to_path(path: PathBuf, extension: &str) -> PathBuf {
///     path.with_added_extension(extension)
/// }
/// ```
///
/// # Arguments
/// * `path` - The path to add the extension to
/// * `extension` - The extension to add (without the leading dot)
///
/// # Returns
/// A new `PathBuf` with the extension appended
fn add_extension_to_path(mut path: PathBuf, extension: &str) -> PathBuf {
    match path.file_name() {
        Some(file_name) => {
            let mut new_file_name = file_name.to_os_string();
            new_file_name.push(".");
            new_file_name.push(extension);
            path.set_file_name(new_file_name);
            path
        }
        None => path,
    }
}

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
                    add_extension_to_path(script_path.clone(), script_type.to_extension()),
                )
            })
            .find(|(_, script_path)| script_path.is_file())
            .map(|(script_type, script_path)| script_type.as_command(&script_path))
            .unwrap_or_else(|| Command::new(runnable_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs_err as fs;
    use std::io;
    use std::path::PathBuf;

    #[test]
    fn test_add_extension_to_path() {
        // Test with simple package name (no dots)
        let path = PathBuf::from("python");
        let result = add_extension_to_path(path, "exe");
        assert_eq!(result, PathBuf::from("python.exe"));

        // Test with package name containing single dot
        let path = PathBuf::from("awslabs.cdk-mcp-server");
        let result = add_extension_to_path(path, "exe");
        assert_eq!(result, PathBuf::from("awslabs.cdk-mcp-server.exe"));

        // Test with package name containing multiple dots
        let path = PathBuf::from("org.example.tool");
        let result = add_extension_to_path(path, "exe");
        assert_eq!(result, PathBuf::from("org.example.tool.exe"));

        // Test with different extensions
        let path = PathBuf::from("script");
        let result = add_extension_to_path(path, "ps1");
        assert_eq!(result, PathBuf::from("script.ps1"));

        // Test with path that has directory components
        let path = PathBuf::from("some/path/to/awslabs.cdk-mcp-server");
        let result = add_extension_to_path(path, "exe");
        assert_eq!(
            result,
            PathBuf::from("some/path/to/awslabs.cdk-mcp-server.exe")
        );

        // Test with empty path (edge case)
        let path = PathBuf::new();
        let result = add_extension_to_path(path.clone(), "exe");
        assert_eq!(result, path); // Should return unchanged
    }

    /// Helper function to create a temporary directory with test files
    fn create_test_environment() -> io::Result<tempfile::TempDir> {
        let temp_dir = tempfile::tempdir()?;
        let scripts_dir = temp_dir.path().join("Scripts");
        fs::create_dir_all(&scripts_dir)?;

        // Create test executable files
        fs::write(scripts_dir.join("python.exe"), "")?;
        fs::write(scripts_dir.join("awslabs.cdk-mcp-server.exe"), "")?;
        fs::write(scripts_dir.join("org.example.tool.exe"), "")?;
        fs::write(scripts_dir.join("multi.dot.package.name.exe"), "")?;
        fs::write(scripts_dir.join("script.ps1"), "")?;
        fs::write(scripts_dir.join("batch.bat"), "")?;
        fs::write(scripts_dir.join("command.cmd"), "")?;
        fs::write(scripts_dir.join("explicit.ps1"), "")?;

        Ok(temp_dir)
    }

    #[test]
    fn test_from_script_path_single_dot_package() {
        let temp_dir = create_test_environment().expect("Failed to create test environment");
        let scripts_dir = temp_dir.path().join("Scripts");

        // Test package name with single dot (awslabs.cdk-mcp-server)
        let command =
            WindowsRunnable::from_script_path(&scripts_dir, OsStr::new("awslabs.cdk-mcp-server"));

        // The command should be constructed with the correct executable path
        let expected_path = scripts_dir.join("awslabs.cdk-mcp-server.exe");
        assert_eq!(command.get_program(), expected_path.as_os_str());
    }

    #[test]
    fn test_from_script_path_multiple_dots_package() {
        let temp_dir = create_test_environment().expect("Failed to create test environment");
        let scripts_dir = temp_dir.path().join("Scripts");

        // Test package name with multiple dots (org.example.tool)
        let command =
            WindowsRunnable::from_script_path(&scripts_dir, OsStr::new("org.example.tool"));

        let expected_path = scripts_dir.join("org.example.tool.exe");
        assert_eq!(command.get_program(), expected_path.as_os_str());

        // Test another multi-dot package
        let command =
            WindowsRunnable::from_script_path(&scripts_dir, OsStr::new("multi.dot.package.name"));

        let expected_path = scripts_dir.join("multi.dot.package.name.exe");
        assert_eq!(command.get_program(), expected_path.as_os_str());
    }

    #[test]
    fn test_from_script_path_simple_package_name() {
        let temp_dir = create_test_environment().expect("Failed to create test environment");
        let scripts_dir = temp_dir.path().join("Scripts");

        // Test simple package name without dots
        let command = WindowsRunnable::from_script_path(&scripts_dir, OsStr::new("python"));

        let expected_path = scripts_dir.join("python.exe");
        assert_eq!(command.get_program(), expected_path.as_os_str());
    }

    #[test]
    fn test_from_script_path_explicit_extensions() {
        let temp_dir = create_test_environment().expect("Failed to create test environment");
        let scripts_dir = temp_dir.path().join("Scripts");

        // Test explicit .ps1 extension
        let command = WindowsRunnable::from_script_path(&scripts_dir, OsStr::new("explicit.ps1"));

        let expected_path = scripts_dir.join("explicit.ps1");
        assert_eq!(command.get_program(), "powershell");

        // Verify the arguments contain the script path
        let args: Vec<&OsStr> = command.get_args().collect();
        assert!(args.contains(&OsStr::new("-File")));
        assert!(args.contains(&expected_path.as_os_str()));

        // Test explicit .bat extension
        let command = WindowsRunnable::from_script_path(&scripts_dir, OsStr::new("batch.bat"));
        assert_eq!(command.get_program(), "cmd");

        // Test explicit .cmd extension
        let command = WindowsRunnable::from_script_path(&scripts_dir, OsStr::new("command.cmd"));
        assert_eq!(command.get_program(), "cmd");
    }
}

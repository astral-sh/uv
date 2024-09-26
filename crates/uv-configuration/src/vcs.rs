use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;
use tracing::debug;

#[derive(Debug, thiserror::Error)]
pub enum VersionControlError {
    #[error("Attempted to initialize a Git repository, but `git` was not found in PATH")]
    GitNotInstalled,
    #[error("Failed to initialize Git repository at `{0}`\nstdout: {1}\nstderr: {2}")]
    GitInit(PathBuf, String, String),
    #[error("`git` command failed")]
    GitCommand(#[source] std::io::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The version control system to use.
#[derive(Clone, Copy, Debug, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum VersionControlSystem {
    /// Use Git for version control.
    #[default]
    Git,
    /// Do not use any version control system.
    None,
}

impl VersionControlSystem {
    /// Initializes the VCS system based on the provided path.
    pub fn init(&self, path: &Path) -> Result<(), VersionControlError> {
        match self {
            Self::Git => {
                let Ok(git) = which::which("git") else {
                    return Err(VersionControlError::GitNotInstalled);
                };

                if path.join(".git").try_exists()? {
                    debug!("Git repository already exists at: `{}`", path.display());
                } else {
                    let output = Command::new(git)
                        .arg("init")
                        .current_dir(path)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .output()
                        .map_err(VersionControlError::GitCommand)?;
                    if !output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        return Err(VersionControlError::GitInit(
                            path.to_path_buf(),
                            stdout.to_string(),
                            stderr.to_string(),
                        ));
                    }
                }

                // Create the `.gitignore`, if it doesn't exist.
                match fs_err::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path.join(".gitignore"))
                {
                    Ok(mut file) => file.write_all(GITIGNORE.as_bytes())?,
                    Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => (),
                    Err(err) => return Err(err.into()),
                }

                Ok(())
            }
            Self::None => Ok(()),
        }
    }

    /// Detects the VCS system based on the provided path.
    pub fn detect(path: &Path) -> Option<Self> {
        // Determine whether the path is inside a Git work tree.
        if which::which("git").is_ok_and(|git| {
            Command::new(git)
                .arg("rev-parse")
                .arg("--is-inside-work-tree")
                .current_dir(path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|status| status.success())
                .unwrap_or(false)
        }) {
            return Some(Self::Git);
        }

        None
    }
}

impl std::fmt::Display for VersionControlSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Git => write!(f, "git"),
            Self::None => write!(f, "none"),
        }
    }
}

const GITIGNORE: &str = "# Python-generated files
__pycache__/
*.py[oc]
build/
dist/
wheels/
*.egg-info

# Virtual environments
.venv
";

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use uv_configuration::VersionControlSystem;
use uv_git::GIT;

#[derive(Debug, thiserror::Error)]
pub(super) enum VersionControlError {
    #[error("Attempted to initialize a Git repository, but `git` was not found in PATH")]
    GitNotInstalled,
    #[error("Failed to initialize Git repository at `{0}`\nstdout: {1}\nstderr: {2}")]
    GitInit(PathBuf, String, String),
    #[error("`git` command failed")]
    GitCommand(#[source] std::io::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Initializes the VCS system based on the provided path.
pub(super) fn init_version_control(
    vcs: VersionControlSystem,
    path: &Path,
) -> Result<(), VersionControlError> {
    match vcs {
        VersionControlSystem::Git => {
            let Ok(git) = GIT.as_ref() else {
                return Err(VersionControlError::GitNotInstalled);
            };

            let output = git
                .build_command()
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
        VersionControlSystem::None => Ok(()),
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

use std::env;
use std::path::PathBuf;

use anyhow::{bail, Result};
use tracing::debug;

use crate::platform::Platform;

/// Locate the current virtual environment.
pub(crate) fn detect_virtual_env(target: &Platform) -> Result<PathBuf> {
    match (env::var_os("VIRTUAL_ENV"), env::var_os("CONDA_PREFIX")) {
        (Some(dir), None) => return Ok(PathBuf::from(dir)),
        (None, Some(dir)) => return Ok(PathBuf::from(dir)),
        (Some(_), Some(_)) => {
            bail!("Both VIRTUAL_ENV and CONDA_PREFIX are set. Please unset one of them.")
        }
        (None, None) => {
            // No environment variables set. Try to find a virtualenv in the current directory.
        }
    };

    // Search for a `.venv` directory in the current or any parent directory.
    let current_dir = env::current_dir().expect("Failed to detect current directory");
    for dir in current_dir.ancestors() {
        let dot_venv = dir.join(".venv");
        if dot_venv.is_dir() {
            if !dot_venv.join("pyvenv.cfg").is_file() {
                bail!(
                    "Expected {} to be a virtual environment, but pyvenv.cfg is missing",
                    dot_venv.display()
                );
            }
            let python = target.get_venv_python(&dot_venv);
            if !python.is_file() {
                bail!(
                    "Your virtualenv at {} is broken. It contains a pyvenv.cfg but no python at {}",
                    dot_venv.display(),
                    python.display()
                );
            }
            debug!("Found a virtualenv named .venv at {}", dot_venv.display());
            return Ok(dot_venv);
        }
    }

    bail!("Couldn't find a virtualenv or conda environment.")
}

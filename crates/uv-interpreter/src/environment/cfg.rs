use std::path::Path;

use fs_err as fs;
use thiserror::Error;

/// A parsed `pyvenv.cfg`
#[derive(Debug, Clone)]
pub struct PyVenvConfiguration {
    /// The version of the `virtualenv` package used to create the virtual environment, if any.
    pub(crate) virtualenv: bool,
    /// The version of the `uv` package used to create the virtual environment, if any.
    pub(crate) uv: bool,
}

impl PyVenvConfiguration {
    /// Parse a `pyvenv.cfg` file into a [`PyVenvConfiguration`].
    pub fn parse(cfg: impl AsRef<Path>) -> Result<Self, Error> {
        let mut virtualenv = false;
        let mut uv = false;

        // Per https://snarky.ca/how-virtual-environments-work/, the `pyvenv.cfg` file is not a
        // valid INI file, and is instead expected to be parsed by partitioning each line on the
        // first equals sign.
        let content = fs::read_to_string(&cfg)?;
        for line in content.lines() {
            let Some((key, _value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "virtualenv" => {
                    virtualenv = true;
                }
                "uv" => {
                    uv = true;
                }
                _ => {}
            }
        }

        Ok(Self { virtualenv, uv })
    }

    /// Returns true if the virtual environment was created with the `virtualenv` package.
    pub fn is_virtualenv(&self) -> bool {
        self.virtualenv
    }

    /// Returns true if the virtual environment was created with the `uv` package.
    pub fn is_uv(&self) -> bool {
        self.uv
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

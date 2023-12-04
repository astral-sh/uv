use std::path::Path;

use fs_err as fs;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct Configuration {
    /// The version of the `virtualenv` package used to create the virtual environment, if any.
    pub(crate) virtualenv: bool,
    /// The version of the `gourgeist` package used to create the virtual environment, if any.
    pub(crate) gourgeist: bool,
}

impl Configuration {
    /// Parse a `pyvenv.cfg` file into a [`Configuration`].
    pub fn parse(cfg: impl AsRef<Path>) -> Result<Self, Error> {
        let mut virtualenv = false;
        let mut gourgeist = false;

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
                "gourgeist" => {
                    gourgeist = true;
                }
                _ => {}
            }
        }

        Ok(Self {
            virtualenv,
            gourgeist,
        })
    }

    /// Returns true if the virtual environment was created with the `virtualenv` package.
    pub fn is_virtualenv(&self) -> bool {
        self.virtualenv
    }

    /// Returns true if the virtual environment was created with the `gourgeist` package.
    pub fn is_gourgeist(&self) -> bool {
        self.gourgeist
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

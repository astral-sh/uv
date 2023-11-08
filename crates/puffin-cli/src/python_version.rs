use std::str::FromStr;

use pep508_rs::{MarkerEnvironment, StringVersion};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum PythonVersion {
    Py37,
    Py38,
    Py39,
    Py310,
    Py311,
    Py312,
}

impl PythonVersion {
    /// Return the `python_version` marker for a [`PythonVersion`].
    fn python_version(self) -> &'static str {
        match self {
            Self::Py37 => "3.7",
            Self::Py38 => "3.8",
            Self::Py39 => "3.9",
            Self::Py310 => "3.10",
            Self::Py311 => "3.11",
            Self::Py312 => "3.12",
        }
    }

    /// Return the `python_full_version` marker for a [`PythonVersion`].
    fn python_full_version(self) -> &'static str {
        match self {
            Self::Py37 => "3.7.0",
            Self::Py38 => "3.8.0",
            Self::Py39 => "3.9.0",
            Self::Py310 => "3.10.0",
            Self::Py311 => "3.11.0",
            Self::Py312 => "3.12.0",
        }
    }

    /// Return the `implementation_version` marker for a [`PythonVersion`].
    fn implementation_version(self) -> &'static str {
        match self {
            Self::Py37 => "3.7.0",
            Self::Py38 => "3.8.0",
            Self::Py39 => "3.9.0",
            Self::Py310 => "3.10.0",
            Self::Py311 => "3.11.0",
            Self::Py312 => "3.12.0",
        }
    }

    /// Return a [`MarkerEnvironment`] compatible with the given [`PythonVersion`], based on
    /// a base [`MarkerEnvironment`].
    ///
    /// The returned [`MarkerEnvironment`] will preserve the base environment's platform markers,
    /// but override its Python version markers.
    pub(crate) fn markers(self, base: &MarkerEnvironment) -> MarkerEnvironment {
        let mut markers = base.clone();
        // Ex) `python_version == "3.12"`
        markers.python_version = StringVersion::from_str(self.python_version()).unwrap();
        // Ex) `python_full_version == "3.12.0"`
        markers.python_full_version = StringVersion::from_str(self.python_full_version()).unwrap();
        // Ex) `implementation_version == "3.12.0"`
        if markers.implementation_name == "cpython" {
            markers.implementation_version =
                StringVersion::from_str(self.implementation_version()).unwrap();
        }
        markers
    }
}

use std::{
    fmt::{self, Display},
    str::FromStr,
};
use thiserror::Error;

use crate::Interpreter;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Unknown Python implementation `{0}`")]
    UnknownImplementation(String),
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Default, PartialOrd, Ord, Hash)]
pub enum ImplementationName {
    Pyodide,
    GraalPy,
    PyPy,
    #[default]
    CPython,
}

#[derive(Debug, Eq, PartialEq, Clone, Ord, PartialOrd, Hash)]
pub enum LenientImplementationName {
    Unknown(String),
    Known(ImplementationName),
}

impl ImplementationName {
    /// Return the full implementation name.
    pub const fn long_name(self) -> &'static str {
        match self {
            Self::CPython => "cpython",
            Self::PyPy => "pypy",
            Self::GraalPy => "graalpy",
            Self::Pyodide => "pyodide",
        }
    }

    /// Return the abbreviated implementation name, if one exists.
    pub const fn short_name(self) -> Option<&'static str> {
        match self {
            Self::CPython => Some("cp"),
            Self::PyPy => Some("pp"),
            Self::GraalPy => Some("gp"),
            Self::Pyodide => None,
        }
    }

    pub(crate) fn iter_all() -> impl Iterator<Item = Self> {
        [Self::CPython, Self::PyPy, Self::GraalPy, Self::Pyodide].into_iter()
    }

    pub(crate) fn pretty(self) -> &'static str {
        match self {
            Self::CPython => "CPython",
            Self::PyPy => "PyPy",
            Self::GraalPy => "GraalPy",
            Self::Pyodide => "Pyodide",
        }
    }

    /// The executable name used in distributions of this implementation.
    pub(crate) fn executable_name(self) -> &'static str {
        match self {
            Self::CPython | Self::Pyodide => "python",
            Self::PyPy | Self::GraalPy => self.long_name(),
        }
    }

    /// The name used when installing this implementation as an executable into the bin directory.
    fn executable_install_name(self) -> &'static str {
        match self {
            Self::Pyodide => "pyodide",
            _ => self.executable_name(),
        }
    }

    pub(crate) fn matches_interpreter(self, interpreter: &Interpreter) -> bool {
        match self {
            Self::Pyodide => interpreter.os().is_emscripten(),
            _ => interpreter
                .implementation_name()
                .eq_ignore_ascii_case(self.long_name()),
        }
    }
}

impl LenientImplementationName {
    pub fn pretty(&self) -> &str {
        match self {
            Self::Known(implementation) => implementation.pretty(),
            Self::Unknown(name) => name,
        }
    }

    pub(crate) fn executable_install_name(&self) -> &str {
        match self {
            Self::Known(implementation) => implementation.executable_install_name(),
            Self::Unknown(name) => name,
        }
    }
}

impl<'a> From<&'a LenientImplementationName> for &'a str {
    fn from(value: &'a LenientImplementationName) -> &'a str {
        match value {
            LenientImplementationName::Known(implementation) => implementation.long_name(),
            LenientImplementationName::Unknown(name) => name,
        }
    }
}

impl FromStr for ImplementationName {
    type Err = Error;

    /// Parse a Python implementation name from a string.
    ///
    /// Supports the full name and the platform compatibility tag style name.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::iter_all()
            .find(|implementation| {
                s.eq_ignore_ascii_case(implementation.long_name())
                    || implementation
                        .short_name()
                        .is_some_and(|name| s.eq_ignore_ascii_case(name))
            })
            .ok_or_else(|| Error::UnknownImplementation(s.to_string()))
    }
}

impl Display for ImplementationName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.long_name())
    }
}

impl From<&str> for LenientImplementationName {
    fn from(s: &str) -> Self {
        match ImplementationName::from_str(s) {
            Ok(implementation) => Self::Known(implementation),
            Err(_) => Self::Unknown(s.to_string()),
        }
    }
}

impl From<ImplementationName> for LenientImplementationName {
    fn from(implementation: ImplementationName) -> Self {
        Self::Known(implementation)
    }
}

impl Display for LenientImplementationName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Known(implementation) => implementation.fmt(f),
            Self::Unknown(name) => f.write_str(&name.to_ascii_lowercase()),
        }
    }
}

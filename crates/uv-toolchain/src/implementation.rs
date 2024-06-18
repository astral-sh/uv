use std::{
    fmt::{self, Display},
    str::FromStr,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Unknown Python implementation `{0}`")]
    UnknownImplementation(String),
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Default, PartialOrd, Ord)]
pub enum ImplementationName {
    #[default]
    CPython,
    PyPy,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum LenientImplementationName {
    Known(ImplementationName),
    Unknown(String),
}

impl ImplementationName {
    pub(crate) fn possible_names() -> impl Iterator<Item = &'static str> {
        ["cpython", "pypy", "cp", "pp"].into_iter()
    }

    pub fn pretty(self) -> &'static str {
        match self {
            Self::CPython => "CPython",
            Self::PyPy => "PyPy",
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
}

impl From<&ImplementationName> for &'static str {
    fn from(v: &ImplementationName) -> &'static str {
        match v {
            ImplementationName::CPython => "cpython",
            ImplementationName::PyPy => "pypy",
        }
    }
}

impl<'a> From<&'a LenientImplementationName> for &'a str {
    fn from(v: &'a LenientImplementationName) -> &'a str {
        match v {
            LenientImplementationName::Known(implementation) => implementation.into(),
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
        match s.to_ascii_lowercase().as_str() {
            "cpython" | "cp" => Ok(Self::CPython),
            "pypy" | "pp" => Ok(Self::PyPy),
            _ => Err(Error::UnknownImplementation(s.to_string())),
        }
    }
}

impl Display for ImplementationName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.into())
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

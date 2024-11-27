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

#[derive(Debug, Eq, PartialEq, Clone, Copy, Default, PartialOrd, Ord, Hash)]
pub enum ImplementationName {
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
    pub(crate) fn short_names() -> impl Iterator<Item = &'static str> {
        ["cp", "pp", "gp"].into_iter()
    }

    pub(crate) fn long_names() -> impl Iterator<Item = &'static str> {
        ["cpython", "pypy", "graalpy"].into_iter()
    }

    pub(crate) fn iter_all() -> impl Iterator<Item = Self> {
        [Self::CPython, Self::PyPy, Self::GraalPy].into_iter()
    }

    pub fn pretty(self) -> &'static str {
        match self {
            Self::CPython => "CPython",
            Self::PyPy => "PyPy",
            Self::GraalPy => "GraalPy",
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
    fn from(value: &ImplementationName) -> &'static str {
        match value {
            ImplementationName::CPython => "cpython",
            ImplementationName::PyPy => "pypy",
            ImplementationName::GraalPy => "graalpy",
        }
    }
}

impl From<ImplementationName> for &'static str {
    fn from(value: ImplementationName) -> &'static str {
        (&value).into()
    }
}

impl<'a> From<&'a LenientImplementationName> for &'a str {
    fn from(value: &'a LenientImplementationName) -> &'a str {
        match value {
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
            "graalpy" | "gp" => Ok(Self::GraalPy),
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

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

#[derive(Debug, Eq, PartialEq, Clone, Copy, Default)]
pub enum ImplementationName {
    #[default]
    CPython,
    PyPy,
}

impl ImplementationName {
    pub(crate) fn iter() -> impl Iterator<Item = &'static ImplementationName> {
        static NAMES: &[ImplementationName] =
            &[ImplementationName::CPython, ImplementationName::PyPy];
        NAMES.iter()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::CPython => "cpython",
            Self::PyPy => "pypy",
        }
    }
}

impl FromStr for ImplementationName {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "cpython" => Ok(Self::CPython),
            "pypy" => Ok(Self::PyPy),
            _ => Err(Error::UnknownImplementation(s.to_string())),
        }
    }
}

impl Display for ImplementationName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CPython => f.write_str("CPython"),
            Self::PyPy => f.write_str("PyPy"),
        }
    }
}

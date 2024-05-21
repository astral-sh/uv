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

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum ImplementationName {
    Cpython,
}

impl ImplementationName {
    pub(crate) fn iter() -> impl Iterator<Item = &'static ImplementationName> {
        static NAMES: &[ImplementationName] = &[ImplementationName::Cpython];
        NAMES.iter()
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Cpython => "cpython",
        }
    }
}

impl FromStr for ImplementationName {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "cpython" => Ok(Self::Cpython),
            _ => Err(Error::UnknownImplementation(s.to_string())),
        }
    }
}

impl Display for ImplementationName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::fmt::{Display, Formatter};

/// Source of a depdendency
/// Either a "-r requirement" or a "-c constraint"
#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Source {
    Requirement(String),
    Constraint(String),
    Override(String),
}

impl Source {
    pub fn to_dependency_string(&self) -> String {
        match self {
            Source::Requirement(name) => {
                format!("-r {name}")
            }
            Source::Constraint(name) => {
                format!("-c {name}")
            }
            Source::Override(name) => {
                format!("--override {name}")
            }
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Source {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Source::Requirement(s.to_string()))
    }
}

impl Display for Source {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

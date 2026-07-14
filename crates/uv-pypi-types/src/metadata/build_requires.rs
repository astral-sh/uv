use std::fmt::Formatter;

use serde::de::SeqAccess;
use serde::{Deserialize, Deserializer};
use uv_normalize::PackageName;
use uv_pep508::Requirement;

use crate::VerbatimParsedUrl;

/// The `build-system.requires` field in a `pyproject.toml` file.
///
/// See: <https://peps.python.org/pep-0518/>
#[derive(Debug, Clone)]
pub struct BuildRequires {
    pub name: Option<PackageName>,
    pub requires_dist: Vec<Requirement<VerbatimParsedUrl>>,
}

/// The `[build-system]` table as specified in PEP 518.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct BuildSystem {
    /// PEP 508 dependencies required to execute the build system.
    pub requires: Vec<Requirement<VerbatimParsedUrl>>,
    /// A string naming a Python object that will be used to perform the build.
    pub build_backend: Option<String>,
    /// A list of directories containing the in-tree build backend.
    pub backend_path: Option<BackendPath>,
}

/// The `backend-path` field in a `[build-system]` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendPath(Vec<String>);

impl BackendPath {
    /// Return an iterator over the backend paths.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }

    /// Return the backend paths.
    pub fn into_inner(self) -> Vec<String> {
        self.0
    }
}

impl<'de> Deserialize<'de> for BackendPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StringOrVec;

        impl<'de> serde::de::Visitor<'de> for StringOrVec {
            type Value = Vec<String>;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("list of strings")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                // Allow exactly `backend-path = "."`, as used in `flit_core==2.3.0`.
                if value == "." {
                    Ok(vec![".".to_string()])
                } else {
                    Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Str(value),
                        &self,
                    ))
                }
            }

            fn visit_seq<S>(self, sequence: S) -> Result<Self::Value, S::Error>
            where
                S: SeqAccess<'de>,
            {
                Deserialize::deserialize(serde::de::value::SeqAccessDeserializer::new(sequence))
            }
        }

        deserializer.deserialize_any(StringOrVec).map(BackendPath)
    }
}

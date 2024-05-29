//! Reads the following fields from `pyproject.toml`:
//!
//! * `project.{dependencies,optional-dependencies}`
//! * `tool.uv.sources`
//! * `tool.uv.workspace`
//!
//! Then lowers them into a dependency specification.

use std::collections::BTreeMap;
use std::ops::Deref;

use glob::Pattern;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use pep508_rs::Pep508Error;
use pypi_types::VerbatimParsedUrl;
use uv_normalize::{ExtraName, PackageName};

use crate::LoweringError;

#[derive(Debug, Error)]
pub enum Pep621Error {
    #[error(transparent)]
    Pep508(#[from] Box<Pep508Error<VerbatimParsedUrl>>),
    #[error("Must specify a `[project]` section alongside `[tool.uv.sources]`")]
    MissingProjectSection,
    #[error("pyproject.toml section is declared as dynamic, but must be static: `{0}`")]
    DynamicNotAllowed(&'static str),
    #[error("Failed to parse entry for: `{0}`")]
    LoweringError(PackageName, #[source] LoweringError),
}

impl From<Pep508Error<VerbatimParsedUrl>> for Pep621Error {
    fn from(error: Pep508Error<VerbatimParsedUrl>) -> Self {
        Self::Pep508(Box::new(error))
    }
}

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct PyProjectToml {
    /// PEP 621-compliant project metadata.
    pub project: Option<Project>,
    /// Tool-specific metadata.
    pub tool: Option<Tool>,
}

/// PEP 621 project metadata (`project`).
///
/// See <https://packaging.python.org/en/latest/specifications/pyproject-toml>.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct Project {
    /// The name of the project
    pub name: PackageName,
    /// The optional dependencies of the project.
    pub optional_dependencies: Option<BTreeMap<ExtraName, Vec<String>>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Tool {
    pub uv: Option<ToolUv>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ToolUv {
    pub sources: Option<BTreeMap<PackageName, Source>>,
    pub workspace: Option<ToolUvWorkspace>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ToolUvWorkspace {
    pub members: Option<Vec<SerdePattern>>,
    pub exclude: Option<Vec<SerdePattern>>,
}

/// (De)serialize globs as strings.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SerdePattern(#[serde(with = "serde_from_and_to_string")] pub Pattern);

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for SerdePattern {
    fn schema_name() -> String {
        <String as schemars::JsonSchema>::schema_name()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <String as schemars::JsonSchema>::json_schema(gen)
    }
}

impl Deref for SerdePattern {
    type Target = Pattern;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A `tool.uv.sources` value.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged, deny_unknown_fields)]
pub enum Source {
    /// A remote Git repository, available over HTTPS or SSH.
    ///
    /// Example:
    /// ```toml
    /// flask = { git = "https://github.com/pallets/flask", tag = "3.0.0" }
    /// ```
    Git {
        /// The repository URL (without the `git+` prefix).
        git: Url,
        /// The path to the directory with the `pyproject.toml`, if it's not in the archive root.
        subdirectory: Option<String>,
        // Only one of the three may be used; we'll validate this later and emit a custom error.
        rev: Option<String>,
        tag: Option<String>,
        branch: Option<String>,
    },
    /// A remote `http://` or `https://` URL, either a wheel (`.whl`) or a source distribution
    /// (`.zip`, `.tar.gz`).
    ///
    /// Example:
    /// ```toml
    /// flask = { url = "https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl" }
    /// ```
    Url {
        url: Url,
        /// For source distributions, the path to the directory with the `pyproject.toml`, if it's
        /// not in the archive root.
        subdirectory: Option<String>,
    },
    /// The path to a dependency, either a wheel (a `.whl` file), source distribution (a `.zip` or
    /// `.tag.gz` file), or source tree (i.e., a directory containing a `pyproject.toml` or
    /// `setup.py` file in the root).
    Path {
        path: String,
        /// `false` by default.
        editable: Option<bool>,
    },
    /// A dependency pinned to a specific index, e.g., `torch` after setting `torch` to `https://download.pytorch.org/whl/cu118`.
    Registry {
        // TODO(konstin): The string is more-or-less a placeholder
        index: String,
    },
    /// A dependency on another package in the workspace.
    Workspace {
        /// When set to `false`, the package will be fetched from the remote index, rather than
        /// included as a workspace package.
        workspace: bool,
        /// `true` by default.
        editable: Option<bool>,
    },
    /// A catch-all variant used to emit precise error messages when deserializing.
    CatchAll {
        git: String,
        subdirectory: Option<String>,
        rev: Option<String>,
        tag: Option<String>,
        branch: Option<String>,
        url: String,
        patch: String,
        index: String,
        workspace: bool,
    },
}
/// <https://github.com/serde-rs/serde/issues/1316#issue-332908452>
mod serde_from_and_to_string {
    use std::fmt::Display;
    use std::str::FromStr;

    use serde::{de, Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Display,
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub(super) fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: FromStr,
        T::Err: Display,
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

//! Reads the following fields from `pyproject.toml`:
//!
//! * `project.{dependencies,optional-dependencies}`
//! * `tool.uv.sources`
//! * `tool.uv.workspace`
//!
//! Then lowers them into a dependency specification.

use std::ops::Deref;
use std::{collections::BTreeMap, mem};

use glob::Pattern;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use pep440_rs::VersionSpecifiers;
use pypi_types::{RequirementSource, VerbatimParsedUrl};
use uv_git::GitReference;
use uv_normalize::{ExtraName, PackageName};

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct PyProjectToml {
    /// PEP 621-compliant project metadata.
    pub project: Option<Project>,
    /// Tool-specific metadata.
    pub tool: Option<Tool>,
    /// The raw unserialized document.
    #[serde(skip)]
    pub(crate) raw: String,
}

impl PyProjectToml {
    /// Parse a `PyProjectToml` from a raw TOML string.
    pub fn from_string(raw: String) -> Result<Self, toml::de::Error> {
        let pyproject = toml::from_str(&raw)?;
        Ok(PyProjectToml { raw, ..pyproject })
    }
}

// Ignore raw document in comparison.
impl PartialEq for PyProjectToml {
    fn eq(&self, other: &Self) -> bool {
        self.project.eq(&other.project) && self.tool.eq(&other.tool)
    }
}

impl Eq for PyProjectToml {}

/// PEP 621 project metadata (`project`).
///
/// See <https://packaging.python.org/en/latest/specifications/pyproject-toml>.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct Project {
    /// The name of the project
    pub name: PackageName,
    /// The Python versions this project is compatible with.
    pub requires_python: Option<VersionSpecifiers>,
    /// The optional dependencies of the project.
    pub optional_dependencies: Option<BTreeMap<ExtraName, Vec<String>>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Tool {
    pub uv: Option<ToolUv>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ToolUv {
    pub sources: Option<BTreeMap<PackageName, Source>>,
    pub workspace: Option<ToolUvWorkspace>,
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "Option<Vec<String>>",
            description = "PEP 508-style requirements, e.g., `flask==3.0.0`, or `black @ https://...`."
        )
    )]
    pub dev_dependencies: Option<Vec<pep508_rs::Requirement<VerbatimParsedUrl>>>,
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "Option<Vec<String>>",
            description = "PEP 508 style requirements, e.g. `flask==3.0.0`, or `black @ https://...`."
        )
    )]
    pub override_dependencies: Option<Vec<pep508_rs::Requirement<VerbatimParsedUrl>>>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
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
    /// `.tar.gz` file), or source tree (i.e., a directory containing a `pyproject.toml` or
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

#[derive(Error, Debug)]
pub enum SourceError {
    #[error("Cannot resolve git reference `{0}`.")]
    UnresolvedReference(String),
    #[error("Workspace dependency must be a local path.")]
    InvalidWorkspaceRequirement,
}

impl Source {
    pub fn from_requirement(
        source: RequirementSource,
        workspace: bool,
        editable: Option<bool>,
        rev: Option<String>,
        tag: Option<String>,
        branch: Option<String>,
    ) -> Result<Option<Source>, SourceError> {
        if workspace {
            match source {
                RequirementSource::Registry { .. } | RequirementSource::Directory { .. } => {}
                _ => return Err(SourceError::InvalidWorkspaceRequirement),
            }

            return Ok(Some(Source::Workspace {
                editable,
                workspace: true,
            }));
        }

        let source = match source {
            RequirementSource::Registry { .. } => return Ok(None),
            RequirementSource::Path { lock_path, .. } => Source::Path {
                editable,
                path: lock_path.to_string_lossy().into_owned(),
            },
            RequirementSource::Directory { lock_path, .. } => Source::Path {
                editable,
                path: lock_path.to_string_lossy().into_owned(),
            },
            RequirementSource::Url {
                subdirectory, url, ..
            } => Source::Url {
                url: url.to_url(),
                subdirectory: subdirectory.map(|path| path.to_string_lossy().into_owned()),
            },
            RequirementSource::Git {
                repository,
                mut reference,
                subdirectory,
                ..
            } => {
                // We can only resolve a full commit hash from a pep508 URL, everything else is ambiguous.
                let rev = match reference {
                    GitReference::FullCommit(ref mut rev) => Some(mem::take(rev)),
                    _ => None,
                }
                // Give precedence to an explicit argument.
                .or(rev);

                // Error if the user tried to specify a reference but didn't disambiguate.
                if reference != GitReference::DefaultBranch
                    && rev.is_none()
                    && tag.is_none()
                    && branch.is_none()
                {
                    return Err(SourceError::UnresolvedReference(
                        reference.as_str().unwrap().to_owned(),
                    ));
                }

                Source::Git {
                    rev,
                    tag,
                    branch,
                    git: repository,
                    subdirectory: subdirectory.map(|path| path.to_string_lossy().into_owned()),
                }
            }
        };

        Ok(Some(source))
    }
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

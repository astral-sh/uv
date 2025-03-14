//! Reads the following fields from `pyproject.toml`:
//!
//! * `project.{dependencies,optional-dependencies}`
//! * `tool.uv.sources`
//! * `tool.uv.workspace`
//!
//! Then lowers them into a dependency specification.

use std::collections::BTreeMap;
use std::fmt::Formatter;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use glob::Pattern;
use owo_colors::OwoColorize;
use rustc_hash::{FxBuildHasher, FxHashSet};
use serde::{de::IntoDeserializer, de::SeqAccess, Deserialize, Deserializer, Serialize};
use thiserror::Error;
use url::Url;

use uv_distribution_types::{Index, IndexName};
use uv_fs::{relative_to, PortablePathBuf};
use uv_git_types::GitReference;
use uv_macros::OptionsMetadata;
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::MarkerTree;
use uv_pypi_types::{
    Conflicts, DependencyGroups, RequirementSource, SchemaConflicts, SupportedEnvironments,
    VerbatimParsedUrl,
};

#[derive(Error, Debug)]
pub enum PyprojectTomlError {
    #[error(transparent)]
    TomlSyntax(#[from] toml_edit::TomlError),
    #[error(transparent)]
    TomlSchema(#[from] toml_edit::de::Error),
    #[error(
        "`pyproject.toml` is using the `[project]` table, but the required `project.name` field is not set"
    )]
    MissingName,
    #[error(
        "`pyproject.toml` is using the `[project]` table, but the required `project.version` field is neither set nor present in the `project.dynamic` list"
    )]
    MissingVersion,
}

/// A `pyproject.toml` as specified in PEP 517.
#[derive(Deserialize, Debug, Clone)]
#[cfg_attr(test, derive(Serialize))]
#[serde(rename_all = "kebab-case")]
pub struct PyProjectToml {
    /// PEP 621-compliant project metadata.
    pub project: Option<Project>,
    /// Tool-specific metadata.
    pub tool: Option<Tool>,
    /// Non-project dependency groups, as defined in PEP 735.
    pub dependency_groups: Option<DependencyGroups>,
    /// The raw unserialized document.
    #[serde(skip)]
    pub raw: String,

    /// Used to determine whether a `build-system` section is present.
    #[serde(default, skip_serializing)]
    build_system: Option<serde::de::IgnoredAny>,
}

impl PyProjectToml {
    /// Parse a `PyProjectToml` from a raw TOML string.
    pub fn from_string(raw: String) -> Result<Self, PyprojectTomlError> {
        let pyproject: toml_edit::ImDocument<_> =
            toml_edit::ImDocument::from_str(&raw).map_err(PyprojectTomlError::TomlSyntax)?;
        let pyproject = PyProjectToml::deserialize(pyproject.into_deserializer())
            .map_err(PyprojectTomlError::TomlSchema)?;
        Ok(PyProjectToml { raw, ..pyproject })
    }

    /// Returns `true` if the project should be considered a Python package, as opposed to a
    /// non-package ("virtual") project.
    pub fn is_package(&self) -> bool {
        // If `tool.uv.package` is set, defer to that explicit setting.
        if let Some(is_package) = self
            .tool
            .as_ref()
            .and_then(|tool| tool.uv.as_ref())
            .and_then(|uv| uv.package)
        {
            return is_package;
        }

        // Otherwise, a project is assumed to be a package if `build-system` is present.
        self.build_system.is_some()
    }

    /// Returns `true` if the project uses a dynamic version.
    pub fn is_dynamic(&self) -> bool {
        self.project
            .as_ref()
            .is_some_and(|project| project.version.is_none())
    }

    /// Returns whether the project manifest contains any script table.
    pub fn has_scripts(&self) -> bool {
        if let Some(ref project) = self.project {
            project.gui_scripts.is_some() || project.scripts.is_some()
        } else {
            false
        }
    }

    /// Returns the set of conflicts for the project.
    pub fn conflicts(&self) -> Conflicts {
        let empty = Conflicts::empty();
        let Some(project) = self.project.as_ref() else {
            return empty;
        };
        let Some(tool) = self.tool.as_ref() else {
            return empty;
        };
        let Some(tooluv) = tool.uv.as_ref() else {
            return empty;
        };
        let Some(conflicting) = tooluv.conflicts.as_ref() else {
            return empty;
        };
        conflicting.to_conflicts_with_package_name(&project.name)
    }
}

// Ignore raw document in comparison.
impl PartialEq for PyProjectToml {
    fn eq(&self, other: &Self) -> bool {
        self.project.eq(&other.project) && self.tool.eq(&other.tool)
    }
}

impl Eq for PyProjectToml {}

impl AsRef<[u8]> for PyProjectToml {
    fn as_ref(&self) -> &[u8] {
        self.raw.as_bytes()
    }
}

/// PEP 621 project metadata (`project`).
///
/// See <https://packaging.python.org/en/latest/specifications/pyproject-toml>.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(Serialize))]
#[serde(rename_all = "kebab-case", try_from = "ProjectWire")]
pub struct Project {
    /// The name of the project
    pub name: PackageName,
    /// The version of the project
    pub version: Option<Version>,
    /// The Python versions this project is compatible with.
    pub requires_python: Option<VersionSpecifiers>,
    /// The dependencies of the project.
    pub dependencies: Option<Vec<String>>,
    /// The optional dependencies of the project.
    pub optional_dependencies: Option<BTreeMap<ExtraName, Vec<String>>>,

    /// Used to determine whether a `gui-scripts` section is present.
    #[serde(default, skip_serializing)]
    pub(crate) gui_scripts: Option<serde::de::IgnoredAny>,
    /// Used to determine whether a `scripts` section is present.
    #[serde(default, skip_serializing)]
    pub(crate) scripts: Option<serde::de::IgnoredAny>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct ProjectWire {
    name: Option<PackageName>,
    version: Option<Version>,
    dynamic: Option<Vec<String>>,
    requires_python: Option<VersionSpecifiers>,
    dependencies: Option<Vec<String>>,
    optional_dependencies: Option<BTreeMap<ExtraName, Vec<String>>>,
    gui_scripts: Option<serde::de::IgnoredAny>,
    scripts: Option<serde::de::IgnoredAny>,
}

impl TryFrom<ProjectWire> for Project {
    type Error = PyprojectTomlError;

    fn try_from(value: ProjectWire) -> Result<Self, Self::Error> {
        // If `[project.name]` is not present, show a dedicated error message.
        let name = value.name.ok_or(PyprojectTomlError::MissingName)?;

        // If `[project.version]` is not present (or listed in `[project.dynamic]`), show a dedicated error message.
        if value.version.is_none()
            && !value
                .dynamic
                .as_ref()
                .is_some_and(|dynamic| dynamic.iter().any(|field| field == "version"))
        {
            return Err(PyprojectTomlError::MissingVersion);
        }

        Ok(Project {
            name,
            version: value.version,
            requires_python: value.requires_python,
            dependencies: value.dependencies,
            optional_dependencies: value.optional_dependencies,
            gui_scripts: value.gui_scripts,
            scripts: value.scripts,
        })
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(Serialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Tool {
    pub uv: Option<ToolUv>,
}

/// Validates that index names in the `tool.uv.index` field are unique.
///
/// This custom deserializer function checks for duplicate index names
/// and returns an error if any duplicates are found.
fn deserialize_index_vec<'de, D>(deserializer: D) -> Result<Option<Vec<Index>>, D::Error>
where
    D: Deserializer<'de>,
{
    let indexes = Option::<Vec<Index>>::deserialize(deserializer)?;
    if let Some(indexes) = indexes.as_ref() {
        let mut seen_names = FxHashSet::with_capacity_and_hasher(indexes.len(), FxBuildHasher);
        for index in indexes {
            if let Some(name) = index.name.as_ref() {
                if !seen_names.insert(name) {
                    return Err(serde::de::Error::custom(format!(
                        "duplicate index name `{name}`"
                    )));
                }
            }
        }
    }
    Ok(indexes)
}

// NOTE(charlie): When adding fields to this struct, mark them as ignored on `Options` in
// `crates/uv-settings/src/settings.rs`.
#[derive(Deserialize, OptionsMetadata, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(Serialize))]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ToolUv {
    /// The sources to use when resolving dependencies.
    ///
    /// `tool.uv.sources` enriches the dependency metadata with additional sources, incorporated
    /// during development. A dependency source can be a Git repository, a URL, a local path, or an
    /// alternative registry.
    ///
    /// See [Dependencies](../concepts/projects/dependencies.md) for more.
    #[option(
        default = "{}",
        value_type = "dict",
        example = r#"
            [tool.uv.sources]
            httpx = { git = "https://github.com/encode/httpx", tag = "0.27.0" }
            pytest = { url = "https://files.pythonhosted.org/packages/6b/77/7440a06a8ead44c7757a64362dd22df5760f9b12dc5f11b6188cd2fc27a0/pytest-8.3.3-py3-none-any.whl" }
            pydantic = { path = "/path/to/pydantic", editable = true }
        "#
    )]
    pub sources: Option<ToolUvSources>,

    /// The indexes to use when resolving dependencies.
    ///
    /// Accepts either a repository compliant with [PEP 503](https://peps.python.org/pep-0503/)
    /// (the simple repository API), or a local directory laid out in the same format.
    ///
    /// Indexes are considered in the order in which they're defined, such that the first-defined
    /// index has the highest priority. Further, the indexes provided by this setting are given
    /// higher priority than any indexes specified via [`index_url`](#index-url) or
    /// [`extra_index_url`](#extra-index-url). uv will only consider the first index that contains
    /// a given package, unless an alternative [index strategy](#index-strategy) is specified.
    ///
    /// If an index is marked as `explicit = true`, it will be used exclusively for the
    /// dependencies that select it explicitly via `[tool.uv.sources]`, as in:
    ///
    /// ```toml
    /// [[tool.uv.index]]
    /// name = "pytorch"
    /// url = "https://download.pytorch.org/whl/cu121"
    /// explicit = true
    ///
    /// [tool.uv.sources]
    /// torch = { index = "pytorch" }
    /// ```
    ///
    /// If an index is marked as `default = true`, it will be moved to the end of the prioritized list, such that it is
    /// given the lowest priority when resolving packages. Additionally, marking an index as default will disable the
    /// PyPI default index.
    #[option(
        default = "[]",
        value_type = "dict",
        example = r#"
            [[tool.uv.index]]
            name = "pytorch"
            url = "https://download.pytorch.org/whl/cu121"
        "#
    )]
    #[serde(deserialize_with = "deserialize_index_vec", default)]
    pub index: Option<Vec<Index>>,

    /// The workspace definition for the project, if any.
    #[option_group]
    pub workspace: Option<ToolUvWorkspace>,

    /// Whether the project is managed by uv. If `false`, uv will ignore the project when
    /// `uv run` is invoked.
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            managed = false
        "#
    )]
    pub managed: Option<bool>,

    /// Whether the project should be considered a Python package, or a non-package ("virtual")
    /// project.
    ///
    /// Packages are built and installed into the virtual environment in editable mode and thus
    /// require a build backend, while virtual projects are _not_ built or installed; instead, only
    /// their dependencies are included in the virtual environment.
    ///
    /// Creating a package requires that a `build-system` is present in the `pyproject.toml`, and
    /// that the project adheres to a structure that adheres to the build backend's expectations
    /// (e.g., a `src` layout).
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            package = false
        "#
    )]
    pub package: Option<bool>,

    /// The list of `dependency-groups` to install by default.
    #[option(
        default = r#"["dev"]"#,
        value_type = "list[str]",
        example = r#"
            default-groups = ["docs"]
        "#
    )]
    pub default_groups: Option<Vec<GroupName>>,

    /// The project's development dependencies.
    ///
    /// Development dependencies will be installed by default in `uv run` and `uv sync`, but will
    /// not appear in the project's published metadata.
    ///
    /// Use of this field is not recommend anymore. Instead, use the `dependency-groups.dev` field
    /// which is a standardized way to declare development dependencies. The contents of
    /// `tool.uv.dev-dependencies` and `dependency-groups.dev` are combined to determine the final
    /// requirements of the `dev` dependency group.
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "Option<Vec<String>>",
            description = "PEP 508-style requirements, e.g., `ruff==0.5.0`, or `ruff @ https://...`."
        )
    )]
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            dev-dependencies = ["ruff==0.5.0"]
        "#
    )]
    pub dev_dependencies: Option<Vec<uv_pep508::Requirement<VerbatimParsedUrl>>>,

    /// Overrides to apply when resolving the project's dependencies.
    ///
    /// Overrides are used to force selection of a specific version of a package, regardless of the
    /// version requested by any other package, and regardless of whether choosing that version
    /// would typically constitute an invalid resolution.
    ///
    /// While constraints are _additive_, in that they're combined with the requirements of the
    /// constituent packages, overrides are _absolute_, in that they completely replace the
    /// requirements of any constituent packages.
    ///
    /// Including a package as an override will _not_ trigger installation of the package on its
    /// own; instead, the package must be requested elsewhere in the project's first-party or
    /// transitive dependencies.
    ///
    /// !!! note
    ///     In `uv lock`, `uv sync`, and `uv run`, uv will only read `override-dependencies` from
    ///     the `pyproject.toml` at the workspace root, and will ignore any declarations in other
    ///     workspace members or `uv.toml` files.
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "Option<Vec<String>>",
            description = "PEP 508-style requirements, e.g., `ruff==0.5.0`, or `ruff @ https://...`."
        )
    )]
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            # Always install Werkzeug 2.3.0, regardless of whether transitive dependencies request
            # a different version.
            override-dependencies = ["werkzeug==2.3.0"]
        "#
    )]
    pub override_dependencies: Option<Vec<uv_pep508::Requirement<VerbatimParsedUrl>>>,

    /// Constraints to apply when resolving the project's dependencies.
    ///
    /// Constraints are used to restrict the versions of dependencies that are selected during
    /// resolution.
    ///
    /// Including a package as a constraint will _not_ trigger installation of the package on its
    /// own; instead, the package must be requested elsewhere in the project's first-party or
    /// transitive dependencies.
    ///
    /// !!! note
    ///     In `uv lock`, `uv sync`, and `uv run`, uv will only read `constraint-dependencies` from
    ///     the `pyproject.toml` at the workspace root, and will ignore any declarations in other
    ///     workspace members or `uv.toml` files.
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "Option<Vec<String>>",
            description = "PEP 508-style requirements, e.g., `ruff==0.5.0`, or `ruff @ https://...`."
        )
    )]
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            # Ensure that the grpcio version is always less than 1.65, if it's requested by a
            # direct or transitive dependency.
            constraint-dependencies = ["grpcio<1.65"]
        "#
    )]
    pub constraint_dependencies: Option<Vec<uv_pep508::Requirement<VerbatimParsedUrl>>>,

    /// Constraints to apply when solving build dependencies.
    ///
    /// Build constraints are used to restrict the versions of build dependencies that are selected
    /// when building a package during resolution or installation.
    ///
    /// Including a package as a constraint will _not_ trigger installation of the package during
    /// a build; instead, the package must be requested elsewhere in the project's build dependency
    /// graph.
    ///
    /// !!! note
    ///     In `uv lock`, `uv sync`, and `uv run`, uv will only read `build-constraint-dependencies` from
    ///     the `pyproject.toml` at the workspace root, and will ignore any declarations in other
    ///     workspace members or `uv.toml` files.
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "Option<Vec<String>>",
            description = "PEP 508-style requirements, e.g., `ruff==0.5.0`, or `ruff @ https://...`."
        )
    )]
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            # Ensure that the setuptools v60.0.0 is used whenever a package has a build dependency
            # on setuptools.
            build-constraint-dependencies = ["setuptools==60.0.0"]
        "#
    )]
    pub build_constraint_dependencies: Option<Vec<uv_pep508::Requirement<VerbatimParsedUrl>>>,

    /// A list of supported environments against which to resolve dependencies.
    ///
    /// By default, uv will resolve for all possible environments during a `uv lock` operation.
    /// However, you can restrict the set of supported environments to improve performance and avoid
    /// unsatisfiable branches in the solution space.
    ///
    /// These environments will also be respected when `uv pip compile` is invoked with the
    /// `--universal` flag.
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "Option<Vec<String>>",
            description = "A list of environment markers, e.g., `python_version >= '3.6'`."
        )
    )]
    #[option(
        default = "[]",
        value_type = "str | list[str]",
        example = r#"
            # Resolve for macOS, but not for Linux or Windows.
            environments = ["sys_platform == 'darwin'"]
        "#
    )]
    pub environments: Option<SupportedEnvironments>,

    /// A list of required platforms, for packages that lack source distributions.
    ///
    /// When a package does not have a source distribution, it's availability will be limited to
    /// the platforms supported by its built distributions (wheels). For example, if a package only
    /// publishes wheels for Linux, then it won't be installable on macOS or Windows.
    ///
    /// By default, uv requires each package to include at least one wheel that is compatible with
    /// the designated Python version. The `required-environments` setting can be used to ensure that
    /// the resulting resolution contains wheels for specific platforms, or fails if no such wheels
    /// are available.
    ///
    /// While the `environments` setting _limits_ the set of environments that uv will consider when
    /// resolving dependencies, `required-environments` _expands_ the set of platforms that uv _must_
    /// support when resolving dependencies.
    ///
    /// For example, `environments = ["sys_platform == 'darwin'"]` would limit uv to solving for
    /// macOS (and ignoring Linux and Windows). On the other hand, `required-environments = ["sys_platform == 'darwin'"]`
    /// would _require_ that any package without a source distribution include a wheel for macOS in
    /// order to be installable.
    #[cfg_attr(
        feature = "schemars",
        schemars(
            with = "Option<Vec<String>>",
            description = "A list of environment markers, e.g., `sys_platform == 'darwin'."
        )
    )]
    #[option(
        default = "[]",
        value_type = "str | list[str]",
        example = r#"
            # Require that the package is available for macOS ARM and x86 (Intel).
            required-environments = [
                "sys_platform == 'darwin' and platform_machine == 'arm64'",
                "sys_platform == 'darwin' and platform_machine == 'x86_64'",
            ]
        "#
    )]
    pub required_environments: Option<SupportedEnvironments>,

    /// Declare collections of extras or dependency groups that are conflicting
    /// (i.e., mutually exclusive).
    ///
    /// It's useful to declare conflicts when two or more extras have mutually
    /// incompatible dependencies. For example, extra `foo` might depend
    /// on `numpy==2.0.0` while extra `bar` depends on `numpy==2.1.0`. While these
    /// dependencies conflict, it may be the case that users are not expected to
    /// activate both `foo` and `bar` at the same time, making it possible to
    /// generate a universal resolution for the project despite the incompatibility.
    ///
    /// By making such conflicts explicit, uv can generate a universal resolution
    /// for a project, taking into account that certain combinations of extras and
    /// groups are mutually exclusive. In exchange, installation will fail if a
    /// user attempts to activate both conflicting extras.
    #[cfg_attr(
        feature = "schemars",
        schemars(description = "A list of sets of conflicting groups or extras.")
    )]
    #[option(
        default = r#"[]"#,
        value_type = "list[list[dict]]",
        example = r#"
            # Require that `package[extra1]` and `package[extra2]` are resolved
            # in different forks so that they cannot conflict with one another.
            conflicts = [
                [
                    { extra = "extra1" },
                    { extra = "extra2" },
                ]
            ]

            # Require that the dependency groups `group1` and `group2`
            # are resolved in different forks so that they cannot conflict
            # with one another.
            conflicts = [
                [
                    { group = "group1" },
                    { group = "group2" },
                ]
            ]
        "#
    )]
    pub conflicts: Option<SchemaConflicts>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(Serialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ToolUvSources(BTreeMap<PackageName, Sources>);

impl ToolUvSources {
    /// Returns the underlying `BTreeMap` of package names to sources.
    pub fn inner(&self) -> &BTreeMap<PackageName, Sources> {
        &self.0
    }

    /// Convert the [`ToolUvSources`] into its inner `BTreeMap`.
    #[must_use]
    pub fn into_inner(self) -> BTreeMap<PackageName, Sources> {
        self.0
    }
}

/// Ensure that all keys in the TOML table are unique.
impl<'de> serde::de::Deserialize<'de> for ToolUvSources {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SourcesVisitor;

        impl<'de> serde::de::Visitor<'de> for SourcesVisitor {
            type Value = ToolUvSources;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a map with unique keys")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                let mut sources = BTreeMap::new();
                while let Some((key, value)) = access.next_entry::<PackageName, Sources>()? {
                    match sources.entry(key) {
                        std::collections::btree_map::Entry::Occupied(entry) => {
                            return Err(serde::de::Error::custom(format!(
                                "duplicate sources for package `{}`",
                                entry.key()
                            )));
                        }
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(value);
                        }
                    }
                }
                Ok(ToolUvSources(sources))
            }
        }

        deserializer.deserialize_map(SourcesVisitor)
    }
}

#[derive(Deserialize, OptionsMetadata, Default, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(Serialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ToolUvWorkspace {
    /// Packages to include as workspace members.
    ///
    /// Supports both globs and explicit paths.
    ///
    /// For more information on the glob syntax, refer to the [`glob` documentation](https://docs.rs/glob/latest/glob/struct.Pattern.html).
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            members = ["member1", "path/to/member2", "libs/*"]
        "#
    )]
    pub members: Option<Vec<SerdePattern>>,
    /// Packages to exclude as workspace members. If a package matches both `members` and
    /// `exclude`, it will be excluded.
    ///
    /// Supports both globs and explicit paths.
    ///
    /// For more information on the glob syntax, refer to the [`glob` documentation](https://docs.rs/glob/latest/glob/struct.Pattern.html).
    #[option(
        default = "[]",
        value_type = "list[str]",
        example = r#"
            exclude = ["member1", "path/to/member2", "libs/*"]
        "#
    )]
    pub exclude: Option<Vec<SerdePattern>>,
}

/// (De)serialize globs as strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerdePattern(Pattern);

impl serde::ser::Serialize for SerdePattern {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.0.as_str().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for SerdePattern {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = SerdePattern;

            fn expecting(&self, f: &mut Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Pattern::from_str(v)
                    .map(SerdePattern)
                    .map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for SerdePattern {
    fn schema_name() -> String {
        <String as schemars::JsonSchema>::schema_name()
    }

    fn json_schema(r#gen: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        <String as schemars::JsonSchema>::json_schema(r#gen)
    }
}

impl Deref for SerdePattern {
    type Target = Pattern;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", try_from = "SourcesWire")]
pub struct Sources(#[cfg_attr(feature = "schemars", schemars(with = "SourcesWire"))] Vec<Source>);

impl Sources {
    /// Return an [`Iterator`] over the sources.
    ///
    /// If the iterator contains multiple entries, they will always use disjoint markers.
    ///
    /// The iterator will contain at most one registry source.
    pub fn iter(&self) -> impl Iterator<Item = &Source> {
        self.0.iter()
    }

    /// Returns `true` if the sources list is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of sources in the list.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl FromIterator<Source> for Sources {
    fn from_iter<T: IntoIterator<Item = Source>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl IntoIterator for Sources {
    type Item = Source;
    type IntoIter = std::vec::IntoIter<Source>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema), schemars(untagged))]
#[allow(clippy::large_enum_variant)]
enum SourcesWire {
    One(Source),
    Many(Vec<Source>),
}

impl<'de> serde::de::Deserialize<'de> for SourcesWire {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = SourcesWire;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a single source (as a map) or list of sources")
            }

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let sources = serde::de::Deserialize::deserialize(
                    serde::de::value::SeqAccessDeserializer::new(seq),
                )?;
                Ok(SourcesWire::Many(sources))
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                let source = serde::de::Deserialize::deserialize(
                    serde::de::value::MapAccessDeserializer::new(&mut map),
                )?;
                Ok(SourcesWire::One(source))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

impl TryFrom<SourcesWire> for Sources {
    type Error = SourceError;

    fn try_from(wire: SourcesWire) -> Result<Self, Self::Error> {
        match wire {
            SourcesWire::One(source) => Ok(Self(vec![source])),
            SourcesWire::Many(sources) => {
                for (lhs, rhs) in sources.iter().zip(sources.iter().skip(1)) {
                    if lhs.extra() != rhs.extra() {
                        continue;
                    };
                    if lhs.group() != rhs.group() {
                        continue;
                    };

                    let lhs = lhs.marker();
                    let rhs = rhs.marker();
                    if !lhs.is_disjoint(rhs) {
                        let Some(left) = lhs.contents().map(|contents| contents.to_string()) else {
                            return Err(SourceError::MissingMarkers);
                        };

                        let Some(right) = rhs.contents().map(|contents| contents.to_string())
                        else {
                            return Err(SourceError::MissingMarkers);
                        };

                        let mut hint = lhs.negate();
                        hint.and(rhs);
                        let hint = hint
                            .contents()
                            .map(|contents| contents.to_string())
                            .unwrap_or_else(|| "true".to_string());

                        return Err(SourceError::OverlappingMarkers(left, right, hint));
                    }
                }

                // Ensure that there is at least one source.
                if sources.is_empty() {
                    return Err(SourceError::EmptySources);
                }

                Ok(Self(sources))
            }
        }
    }
}

/// A `tool.uv.sources` value.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case", untagged, deny_unknown_fields)]
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
        subdirectory: Option<PortablePathBuf>,
        // Only one of the three may be used; we'll validate this later and emit a custom error.
        rev: Option<String>,
        tag: Option<String>,
        branch: Option<String>,
        #[serde(
            skip_serializing_if = "uv_pep508::marker::ser::is_empty",
            serialize_with = "uv_pep508::marker::ser::serialize",
            default
        )]
        marker: MarkerTree,
        extra: Option<ExtraName>,
        group: Option<GroupName>,
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
        subdirectory: Option<PortablePathBuf>,
        #[serde(
            skip_serializing_if = "uv_pep508::marker::ser::is_empty",
            serialize_with = "uv_pep508::marker::ser::serialize",
            default
        )]
        marker: MarkerTree,
        extra: Option<ExtraName>,
        group: Option<GroupName>,
    },
    /// The path to a dependency, either a wheel (a `.whl` file), source distribution (a `.zip` or
    /// `.tar.gz` file), or source tree (i.e., a directory containing a `pyproject.toml` or
    /// `setup.py` file in the root).
    Path {
        path: PortablePathBuf,
        /// `false` by default.
        editable: Option<bool>,
        /// Whether to treat the dependency as a buildable Python package (`true`) or as a virtual
        /// package (`false`). If `false`, the package will not be built or installed, but its
        /// dependencies will be included in the virtual environment.
        ///
        /// When omitted, the package status is inferred based on the presence of a `[build-system]`
        /// in the project's `pyproject.toml`.
        package: Option<bool>,
        #[serde(
            skip_serializing_if = "uv_pep508::marker::ser::is_empty",
            serialize_with = "uv_pep508::marker::ser::serialize",
            default
        )]
        marker: MarkerTree,
        extra: Option<ExtraName>,
        group: Option<GroupName>,
    },
    /// A dependency pinned to a specific index, e.g., `torch` after setting `torch` to `https://download.pytorch.org/whl/cu118`.
    Registry {
        index: IndexName,
        #[serde(
            skip_serializing_if = "uv_pep508::marker::ser::is_empty",
            serialize_with = "uv_pep508::marker::ser::serialize",
            default
        )]
        marker: MarkerTree,
        extra: Option<ExtraName>,
        group: Option<GroupName>,
    },
    /// A dependency on another package in the workspace.
    Workspace {
        /// When set to `false`, the package will be fetched from the remote index, rather than
        /// included as a workspace package.
        workspace: bool,
        #[serde(
            skip_serializing_if = "uv_pep508::marker::ser::is_empty",
            serialize_with = "uv_pep508::marker::ser::serialize",
            default
        )]
        marker: MarkerTree,
        extra: Option<ExtraName>,
        group: Option<GroupName>,
    },
}

/// A custom deserialization implementation for [`Source`]. This is roughly equivalent to
/// `#[serde(untagged)]`, but provides more detailed error messages.
impl<'de> Deserialize<'de> for Source {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize, Debug, Clone)]
        #[serde(rename_all = "kebab-case", deny_unknown_fields)]
        struct CatchAll {
            git: Option<Url>,
            subdirectory: Option<PortablePathBuf>,
            rev: Option<String>,
            tag: Option<String>,
            branch: Option<String>,
            url: Option<Url>,
            path: Option<PortablePathBuf>,
            editable: Option<bool>,
            package: Option<bool>,
            index: Option<IndexName>,
            workspace: Option<bool>,
            #[serde(
                skip_serializing_if = "uv_pep508::marker::ser::is_empty",
                serialize_with = "uv_pep508::marker::ser::serialize",
                default
            )]
            marker: MarkerTree,
            extra: Option<ExtraName>,
            group: Option<GroupName>,
        }

        // Attempt to deserialize as `CatchAll`.
        let CatchAll {
            git,
            subdirectory,
            rev,
            tag,
            branch,
            url,
            path,
            editable,
            package,
            index,
            workspace,
            marker,
            extra,
            group,
        } = CatchAll::deserialize(deserializer)?;

        // If both `extra` and `group` are set, return an error.
        if extra.is_some() && group.is_some() {
            return Err(serde::de::Error::custom(
                "cannot specify both `extra` and `group`",
            ));
        }

        // If the `git` field is set, we're dealing with a Git source.
        if let Some(git) = git {
            if index.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `git` and `index`",
                ));
            }
            if workspace.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `git` and `workspace`",
                ));
            }
            if path.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `git` and `path`",
                ));
            }
            if url.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `git` and `url`",
                ));
            }
            if editable.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `git` and `editable`",
                ));
            }
            if package.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `git` and `package`",
                ));
            }

            // At most one of `rev`, `tag`, or `branch` may be set.
            match (rev.as_ref(), tag.as_ref(), branch.as_ref()) {
                (None, None, None) => {}
                (Some(_), None, None) => {}
                (None, Some(_), None) => {}
                (None, None, Some(_)) => {}
                _ => {
                    return Err(serde::de::Error::custom(
                        "expected at most one of `rev`, `tag`, or `branch`",
                    ));
                }
            };

            // If the user prefixed the URL with `git+`, strip it.
            let git = if let Some(git) = git.as_str().strip_prefix("git+") {
                Url::parse(git).map_err(serde::de::Error::custom)?
            } else {
                git
            };

            return Ok(Self::Git {
                git,
                subdirectory,
                rev,
                tag,
                branch,
                marker,
                extra,
                group,
            });
        }

        // If the `url` field is set, we're dealing with a URL source.
        if let Some(url) = url {
            if index.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `url` and `index`",
                ));
            }
            if workspace.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `url` and `workspace`",
                ));
            }
            if path.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `url` and `path`",
                ));
            }
            if git.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `url` and `git`",
                ));
            }
            if rev.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `url` and `rev`",
                ));
            }
            if tag.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `url` and `tag`",
                ));
            }
            if branch.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `url` and `branch`",
                ));
            }
            if editable.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `url` and `editable`",
                ));
            }
            if package.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `url` and `package`",
                ));
            }

            return Ok(Self::Url {
                url,
                subdirectory,
                marker,
                extra,
                group,
            });
        }

        // If the `path` field is set, we're dealing with a path source.
        if let Some(path) = path {
            if index.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `path` and `index`",
                ));
            }
            if workspace.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `path` and `workspace`",
                ));
            }
            if git.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `path` and `git`",
                ));
            }
            if url.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `path` and `url`",
                ));
            }
            if rev.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `path` and `rev`",
                ));
            }
            if tag.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `path` and `tag`",
                ));
            }
            if branch.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `path` and `branch`",
                ));
            }

            // A project must be packaged in order to be installed as editable.
            if editable == Some(true) && package == Some(false) {
                return Err(serde::de::Error::custom(
                    "cannot specify both `editable = true` and `package = false`",
                ));
            }

            return Ok(Self::Path {
                path,
                editable,
                package,
                marker,
                extra,
                group,
            });
        }

        // If the `index` field is set, we're dealing with a registry source.
        if let Some(index) = index {
            if workspace.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `index` and `workspace`",
                ));
            }
            if git.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `index` and `git`",
                ));
            }
            if url.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `index` and `url`",
                ));
            }
            if path.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `index` and `path`",
                ));
            }
            if rev.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `index` and `rev`",
                ));
            }
            if tag.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `index` and `tag`",
                ));
            }
            if branch.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `index` and `branch`",
                ));
            }
            if editable.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `index` and `editable`",
                ));
            }
            if package.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `index` and `package`",
                ));
            }

            return Ok(Self::Registry {
                index,
                marker,
                extra,
                group,
            });
        }

        // If the `workspace` field is set, we're dealing with a workspace source.
        if let Some(workspace) = workspace {
            if index.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `workspace` and `index`",
                ));
            }
            if git.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `workspace` and `git`",
                ));
            }
            if url.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `workspace` and `url`",
                ));
            }
            if path.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `workspace` and `path`",
                ));
            }
            if rev.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `workspace` and `rev`",
                ));
            }
            if tag.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `workspace` and `tag`",
                ));
            }
            if branch.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `workspace` and `branch`",
                ));
            }
            if editable.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `workspace` and `editable`",
                ));
            }
            if package.is_some() {
                return Err(serde::de::Error::custom(
                    "cannot specify both `workspace` and `package`",
                ));
            }

            return Ok(Self::Workspace {
                workspace,
                marker,
                extra,
                group,
            });
        }

        // If none of the fields are set, we're dealing with an error.
        Err(serde::de::Error::custom(
            "expected one of `git`, `url`, `path`, `index`, or `workspace`",
        ))
    }
}

#[derive(Error, Debug)]
pub enum SourceError {
    #[error("Failed to resolve Git reference: `{0}`")]
    UnresolvedReference(String),
    #[error("Workspace dependency `{0}` must refer to local directory, not a Git repository")]
    WorkspacePackageGit(String),
    #[error("Workspace dependency `{0}` must refer to local directory, not a URL")]
    WorkspacePackageUrl(String),
    #[error("Workspace dependency `{0}` must refer to local directory, not a file")]
    WorkspacePackageFile(String),
    #[error(
        "`{0}` did not resolve to a Git repository, but a Git reference (`--rev {1}`) was provided."
    )]
    UnusedRev(String, String),
    #[error(
        "`{0}` did not resolve to a Git repository, but a Git reference (`--tag {1}`) was provided."
    )]
    UnusedTag(String, String),
    #[error(
        "`{0}` did not resolve to a Git repository, but a Git reference (`--branch {1}`) was provided."
    )]
    UnusedBranch(String, String),
    #[error(
        "`{0}` did not resolve to a local directory, but the `--editable` flag was provided. Editable installs are only supported for local directories."
    )]
    UnusedEditable(String),
    #[error(
        "Workspace dependency `{0}` was marked as `--no-editable`, but workspace dependencies are always added in editable mode. Pass `--no-editable` to `uv sync` or `uv run` to install workspace dependencies in non-editable mode."
    )]
    UnusedNoEditable(String),
    #[error("Failed to resolve absolute path")]
    Absolute(#[from] std::io::Error),
    #[error("Path contains invalid characters: `{}`", _0.display())]
    NonUtf8Path(PathBuf),
    #[error("Source markers must be disjoint, but the following markers overlap: `{0}` and `{1}`.\n\n{hint}{colon} replace `{1}` with `{2}`.", hint = "hint".bold().cyan(), colon = ":".bold()
    )]
    OverlappingMarkers(String, String, String),
    #[error(
        "When multiple sources are provided, each source must include a platform marker (e.g., `marker = \"sys_platform == 'linux'\"`)"
    )]
    MissingMarkers,
    #[error("Must provide at least one source")]
    EmptySources,
}

impl Source {
    pub fn from_requirement(
        name: &PackageName,
        source: RequirementSource,
        workspace: bool,
        editable: Option<bool>,
        index: Option<IndexName>,
        rev: Option<String>,
        tag: Option<String>,
        branch: Option<String>,
        root: &Path,
    ) -> Result<Option<Source>, SourceError> {
        // If we resolved to a non-Git source, and the user specified a Git reference, error.
        if !matches!(source, RequirementSource::Git { .. }) {
            if let Some(rev) = rev {
                return Err(SourceError::UnusedRev(name.to_string(), rev));
            }
            if let Some(tag) = tag {
                return Err(SourceError::UnusedTag(name.to_string(), tag));
            }
            if let Some(branch) = branch {
                return Err(SourceError::UnusedBranch(name.to_string(), branch));
            }
        }

        if workspace {
            // If a workspace source is added with `--no-editable`, error.
            if editable == Some(false) {
                return Err(SourceError::UnusedNoEditable(name.to_string()));
            }
        } else {
            // If we resolved a non-path source, and user specified an `--editable` flag, error.
            if !matches!(source, RequirementSource::Directory { .. }) {
                if editable == Some(true) {
                    return Err(SourceError::UnusedEditable(name.to_string()));
                }
            }
        }

        // If the source is a workspace package, error if the user tried to specify a source.
        if workspace {
            return match source {
                RequirementSource::Registry { .. } | RequirementSource::Directory { .. } => {
                    Ok(Some(Source::Workspace {
                        workspace: true,
                        marker: MarkerTree::TRUE,
                        extra: None,
                        group: None,
                    }))
                }
                RequirementSource::Url { .. } => {
                    Err(SourceError::WorkspacePackageUrl(name.to_string()))
                }
                RequirementSource::Git { .. } => {
                    Err(SourceError::WorkspacePackageGit(name.to_string()))
                }
                RequirementSource::Path { .. } => {
                    Err(SourceError::WorkspacePackageFile(name.to_string()))
                }
            };
        }

        let source = match source {
            RequirementSource::Registry { index: Some(_), .. } => {
                return Ok(None);
            }
            RequirementSource::Registry { index: None, .. } => {
                if let Some(index) = index {
                    Source::Registry {
                        index,
                        marker: MarkerTree::TRUE,
                        extra: None,
                        group: None,
                    }
                } else {
                    return Ok(None);
                }
            }
            RequirementSource::Path { install_path, .. }
            | RequirementSource::Directory { install_path, .. } => Source::Path {
                editable,
                package: None,
                path: PortablePathBuf::from(
                    relative_to(&install_path, root)
                        .or_else(|_| std::path::absolute(&install_path))
                        .map_err(SourceError::Absolute)?,
                ),
                marker: MarkerTree::TRUE,
                extra: None,
                group: None,
            },
            RequirementSource::Url {
                location,
                subdirectory,
                ..
            } => Source::Url {
                url: location,
                subdirectory: subdirectory.map(PortablePathBuf::from),
                marker: MarkerTree::TRUE,
                extra: None,
                group: None,
            },
            RequirementSource::Git {
                git, subdirectory, ..
            } => {
                if rev.is_none() && tag.is_none() && branch.is_none() {
                    let rev = match git.reference() {
                        GitReference::Branch(rev) => Some(rev),
                        GitReference::Tag(rev) => Some(rev),
                        GitReference::BranchOrTag(rev) => Some(rev),
                        GitReference::BranchOrTagOrCommit(rev) => Some(rev),
                        GitReference::NamedRef(rev) => Some(rev),
                        GitReference::DefaultBranch => None,
                    };
                    Source::Git {
                        rev: rev.cloned(),
                        tag,
                        branch,
                        git: git.repository().clone(),
                        subdirectory: subdirectory.map(PortablePathBuf::from),
                        marker: MarkerTree::TRUE,
                        extra: None,
                        group: None,
                    }
                } else {
                    Source::Git {
                        rev,
                        tag,
                        branch,
                        git: git.repository().clone(),
                        subdirectory: subdirectory.map(PortablePathBuf::from),
                        marker: MarkerTree::TRUE,
                        extra: None,
                        group: None,
                    }
                }
            }
        };

        Ok(Some(source))
    }

    /// Return the [`MarkerTree`] for the source.
    pub fn marker(&self) -> MarkerTree {
        match self {
            Source::Git { marker, .. } => *marker,
            Source::Url { marker, .. } => *marker,
            Source::Path { marker, .. } => *marker,
            Source::Registry { marker, .. } => *marker,
            Source::Workspace { marker, .. } => *marker,
        }
    }

    /// Return the extra name for the source.
    pub fn extra(&self) -> Option<&ExtraName> {
        match self {
            Source::Git { extra, .. } => extra.as_ref(),
            Source::Url { extra, .. } => extra.as_ref(),
            Source::Path { extra, .. } => extra.as_ref(),
            Source::Registry { extra, .. } => extra.as_ref(),
            Source::Workspace { extra, .. } => extra.as_ref(),
        }
    }

    /// Return the dependency group name for the source.
    pub fn group(&self) -> Option<&GroupName> {
        match self {
            Source::Git { group, .. } => group.as_ref(),
            Source::Url { group, .. } => group.as_ref(),
            Source::Path { group, .. } => group.as_ref(),
            Source::Registry { group, .. } => group.as_ref(),
            Source::Workspace { group, .. } => group.as_ref(),
        }
    }
}

/// The type of a dependency in a `pyproject.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyType {
    /// A dependency in `project.dependencies`.
    Production,
    /// A dependency in `tool.uv.dev-dependencies`.
    Dev,
    /// A dependency in `project.optional-dependencies.{0}`.
    Optional(ExtraName),
    /// A dependency in `dependency-groups.{0}`.
    Group(GroupName),
}

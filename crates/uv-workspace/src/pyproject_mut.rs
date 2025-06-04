use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::str::FromStr;
use std::{fmt, iter, mem};
use thiserror::Error;
use toml_edit::{
    Array, ArrayOfTables, DocumentMut, Formatted, Item, RawString, Table, TomlError, Value,
};

use uv_cache_key::CanonicalUrl;
use uv_distribution_types::Index;
use uv_fs::PortablePath;
use uv_normalize::GroupName;
use uv_pep440::{Version, VersionParseError, VersionSpecifier, VersionSpecifiers};
use uv_pep508::{ExtraName, MarkerTree, PackageName, Requirement, VersionOrUrl};
use uv_redacted::DisplaySafeUrl;

use crate::pyproject::{DependencyType, Source};

/// Raw and mutable representation of a `pyproject.toml`.
///
/// This is useful for operations that require editing an existing `pyproject.toml` while
/// preserving comments and other structure, such as `uv add` and `uv remove`.
pub struct PyProjectTomlMut {
    doc: DocumentMut,
    target: DependencyTarget,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to parse `pyproject.toml`")]
    Parse(#[from] Box<TomlError>),
    #[error("Failed to serialize `pyproject.toml`")]
    Serialize(#[from] Box<toml::ser::Error>),
    #[error("Failed to deserialize `pyproject.toml`")]
    Deserialize(#[from] Box<toml::de::Error>),
    #[error("Dependencies in `pyproject.toml` are malformed")]
    MalformedDependencies,
    #[error("Sources in `pyproject.toml` are malformed")]
    MalformedSources,
    #[error("Workspace in `pyproject.toml` is malformed")]
    MalformedWorkspace,
    #[error("Expected a dependency at index {0}")]
    MissingDependency(usize),
    #[error("Failed to parse `version` field of `pyproject.toml`")]
    VersionParse(#[from] VersionParseError),
    #[error("Cannot perform ambiguous update; found multiple entries for `{}`:\n{}", package_name, requirements.iter().map(|requirement| format!("- `{requirement}`")).join("\n"))]
    Ambiguous {
        package_name: PackageName,
        requirements: Vec<Requirement>,
    },
    #[error("Unknown bound king {0}")]
    UnknownBoundKind(String),
}

/// The result of editing an array in a TOML document.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ArrayEdit {
    /// An existing entry (at the given index) was updated.
    Update(usize),
    /// A new entry was added at the given index (typically, the end of the array).
    Add(usize),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum CommentType {
    /// A comment that appears on its own line.
    OwnLine,
    /// A comment that appears at the end of a line.
    EndOfLine,
}

#[derive(Debug, Clone)]
struct Comment {
    text: String,
    comment_type: CommentType,
}

impl ArrayEdit {
    pub fn index(&self) -> usize {
        match self {
            Self::Update(i) | Self::Add(i) => *i,
        }
    }
}

/// The default version specifier when adding a dependency.
// While PEP 440 allows an arbitrary number of version digits, the `major` and `minor` build on
// most projects sticking to two or three components and a SemVer-ish versioning system, so can
// bump the major or minor version of a major.minor or major.minor.patch input version.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum AddBoundsKind {
    /// Only a lower bound, e.g., `>=1.2.3`.
    #[default]
    Lower,
    /// Allow the same major version, similar to the semver caret, e.g., `>=1.2.3, <2.0.0`.
    ///
    /// Leading zeroes are skipped, e.g. `>=0.1.2, <0.2.0`.
    Major,
    /// Allow the same minor version, similar to the semver tilde, e.g., `>=1.2.3, <1.3.0`.
    ///
    /// Leading zeroes are skipped, e.g. `>=0.1.2, <0.1.3`.
    Minor,
    /// Pin the exact version, e.g., `==1.2.3`.
    ///
    /// This option is not recommended, as versions are already pinned in the uv lockfile.
    Exact,
}

impl Display for AddBoundsKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lower => write!(f, "lower"),
            Self::Major => write!(f, "major"),
            Self::Minor => write!(f, "minor"),
            Self::Exact => write!(f, "exact"),
        }
    }
}

impl AddBoundsKind {
    fn specifiers(self, version: Version) -> VersionSpecifiers {
        // Nomenclature: "major" is the most significant component of the version, "minor" is the
        // second most significant component, so most versions are either major.minor.patch or
        // 0.major.minor.
        match self {
            AddBoundsKind::Lower => {
                VersionSpecifiers::from(VersionSpecifier::greater_than_equal_version(version))
            }
            AddBoundsKind::Major => {
                let leading_zeroes = version
                    .release()
                    .iter()
                    .take_while(|digit| **digit == 0)
                    .count();

                // Special case: The version is 0.
                if leading_zeroes == version.release().len() {
                    let upper_bound = Version::new(
                        [0, 1]
                            .into_iter()
                            .chain(iter::repeat_n(0, version.release().iter().skip(2).len())),
                    );
                    return VersionSpecifiers::from_iter([
                        VersionSpecifier::greater_than_equal_version(version),
                        VersionSpecifier::less_than_version(upper_bound),
                    ]);
                }

                // Compute the new major version and pad it to the same length:
                // 1.2.3 -> 2.0.0
                // 1.2 -> 2.0
                // 1 -> 2
                // We ignore leading zeroes, adding Semver-style semantics to 0.x versions, too:
                // 0.1.2 -> 0.2.0
                // 0.0.1 -> 0.0.2
                let major = version.release().get(leading_zeroes).copied().unwrap_or(0);
                // The length of the lower bound minus the leading zero and bumped component.
                let trailing_zeros = version.release().iter().skip(leading_zeroes + 1).len();
                let upper_bound = Version::new(
                    iter::repeat_n(0, leading_zeroes)
                        .chain(iter::once(major + 1))
                        .chain(iter::repeat_n(0, trailing_zeros)),
                );

                VersionSpecifiers::from_iter([
                    VersionSpecifier::greater_than_equal_version(version),
                    VersionSpecifier::less_than_version(upper_bound),
                ])
            }
            AddBoundsKind::Minor => {
                let leading_zeroes = version
                    .release()
                    .iter()
                    .take_while(|digit| **digit == 0)
                    .count();

                // Special case: The version is 0.
                if leading_zeroes == version.release().len() {
                    let upper_bound = [0, 0, 1]
                        .into_iter()
                        .chain(iter::repeat_n(0, version.release().iter().skip(3).len()));
                    return VersionSpecifiers::from_iter([
                        VersionSpecifier::greater_than_equal_version(version),
                        VersionSpecifier::less_than_version(Version::new(upper_bound)),
                    ]);
                }

                // If both major and minor version are 0, the concept of bumping the minor version
                // instead of the major version is not useful. Instead, we bump the next
                // non-zero part of the version. This avoids extending the three components of 0.0.1
                // to the four components of 0.0.1.1.
                if leading_zeroes >= 2 {
                    let most_significant =
                        version.release().get(leading_zeroes).copied().unwrap_or(0);
                    // The length of the lower bound minus the leading zero and bumped component.
                    let trailing_zeros = version.release().iter().skip(leading_zeroes + 1).len();
                    let upper_bound = Version::new(
                        iter::repeat_n(0, leading_zeroes)
                            .chain(iter::once(most_significant + 1))
                            .chain(iter::repeat_n(0, trailing_zeros)),
                    );
                    return VersionSpecifiers::from_iter([
                        VersionSpecifier::greater_than_equal_version(version),
                        VersionSpecifier::less_than_version(upper_bound),
                    ]);
                }

                // Compute the new minor version and pad it to the same length where possible:
                // 1.2.3 -> 1.3.0
                // 1.2 -> 1.3
                // 1 -> 1.1
                // We ignore leading zero, adding Semver-style semantics to 0.x versions, too:
                // 0.1.2 -> 0.1.3
                // 0.0.1 -> 0.0.2

                // If the version has only one digit, say `1`, or if there are only leading zeroes,
                // pad with zeroes.
                let major = version.release().get(leading_zeroes).copied().unwrap_or(0);
                let minor = version
                    .release()
                    .get(leading_zeroes + 1)
                    .copied()
                    .unwrap_or(0);
                let upper_bound = Version::new(
                    iter::repeat_n(0, leading_zeroes)
                        .chain(iter::once(major))
                        .chain(iter::once(minor + 1))
                        .chain(iter::repeat_n(
                            0,
                            version.release().iter().skip(leading_zeroes + 2).len(),
                        )),
                );

                VersionSpecifiers::from_iter([
                    VersionSpecifier::greater_than_equal_version(version),
                    VersionSpecifier::less_than_version(upper_bound),
                ])
            }
            AddBoundsKind::Exact => {
                VersionSpecifiers::from_iter([VersionSpecifier::equals_version(version)])
            }
        }
    }
}

/// Specifies whether dependencies are added to a script file or a `pyproject.toml` file.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DependencyTarget {
    /// A PEP 723 script, with inline metadata.
    Script,
    /// A project with a `pyproject.toml`.
    PyProjectToml,
}

impl PyProjectTomlMut {
    /// Initialize a [`PyProjectTomlMut`] from a [`str`].
    pub fn from_toml(raw: &str, target: DependencyTarget) -> Result<Self, Error> {
        Ok(Self {
            doc: raw.parse().map_err(Box::new)?,
            target,
        })
    }

    /// Adds a project to the workspace.
    pub fn add_workspace(&mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        // Get or create `tool.uv.workspace.members`.
        let members = self
            .doc
            .entry("tool")
            .or_insert(implicit())
            .as_table_mut()
            .ok_or(Error::MalformedWorkspace)?
            .entry("uv")
            .or_insert(implicit())
            .as_table_mut()
            .ok_or(Error::MalformedWorkspace)?
            .entry("workspace")
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or(Error::MalformedWorkspace)?
            .entry("members")
            .or_insert(Item::Value(Value::Array(Array::new())))
            .as_array_mut()
            .ok_or(Error::MalformedWorkspace)?;

        // Add the path to the workspace.
        members.push(PortablePath::from(path.as_ref()).to_string());

        reformat_array_multiline(members);

        Ok(())
    }

    /// Retrieves a mutable reference to the `project` [`Table`] of the TOML document, creating the
    /// table if necessary.
    ///
    /// For a script, this returns the root table.
    fn project(&mut self) -> Result<&mut Table, Error> {
        let doc = match self.target {
            DependencyTarget::Script => self.doc.as_table_mut(),
            DependencyTarget::PyProjectToml => self
                .doc
                .entry("project")
                .or_insert(Item::Table(Table::new()))
                .as_table_mut()
                .ok_or(Error::MalformedDependencies)?,
        };
        Ok(doc)
    }

    /// Retrieves an optional mutable reference to the `project` [`Table`], returning `None` if it
    /// doesn't exist.
    ///
    /// For a script, this returns the root table.
    fn project_mut(&mut self) -> Result<Option<&mut Table>, Error> {
        let doc = match self.target {
            DependencyTarget::Script => Some(self.doc.as_table_mut()),
            DependencyTarget::PyProjectToml => self
                .doc
                .get_mut("project")
                .map(|project| project.as_table_mut().ok_or(Error::MalformedSources))
                .transpose()?,
        };
        Ok(doc)
    }

    /// Adds a dependency to `project.dependencies`.
    ///
    /// Returns `true` if the dependency was added, `false` if it was updated.
    pub fn add_dependency(
        &mut self,
        req: &Requirement,
        source: Option<&Source>,
        raw: bool,
    ) -> Result<ArrayEdit, Error> {
        // Get or create `project.dependencies`.
        let dependencies = self
            .project()?
            .entry("dependencies")
            .or_insert(Item::Value(Value::Array(Array::new())))
            .as_array_mut()
            .ok_or(Error::MalformedDependencies)?;

        let edit = add_dependency(req, dependencies, source.is_some(), raw)?;

        if let Some(source) = source {
            self.add_source(&req.name, source)?;
        }

        Ok(edit)
    }

    /// Adds a development dependency to `tool.uv.dev-dependencies`.
    ///
    /// Returns `true` if the dependency was added, `false` if it was updated.
    pub fn add_dev_dependency(
        &mut self,
        req: &Requirement,
        source: Option<&Source>,
        raw: bool,
    ) -> Result<ArrayEdit, Error> {
        // Get or create `tool.uv.dev-dependencies`.
        let dev_dependencies = self
            .doc
            .entry("tool")
            .or_insert(implicit())
            .as_table_mut()
            .ok_or(Error::MalformedSources)?
            .entry("uv")
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or(Error::MalformedSources)?
            .entry("dev-dependencies")
            .or_insert(Item::Value(Value::Array(Array::new())))
            .as_array_mut()
            .ok_or(Error::MalformedDependencies)?;

        let edit = add_dependency(req, dev_dependencies, source.is_some(), raw)?;

        if let Some(source) = source {
            self.add_source(&req.name, source)?;
        }

        Ok(edit)
    }

    /// Add an [`Index`] to `tool.uv.index`.
    pub fn add_index(&mut self, index: &Index) -> Result<(), Error> {
        let existing = self
            .doc
            .entry("tool")
            .or_insert(implicit())
            .as_table_mut()
            .ok_or(Error::MalformedSources)?
            .entry("uv")
            .or_insert(implicit())
            .as_table_mut()
            .ok_or(Error::MalformedSources)?
            .entry("index")
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .ok_or(Error::MalformedSources)?;

        // If there's already an index with the same name or URL, update it (and move it to the top).
        let mut table = existing
            .iter()
            .find(|table| {
                // If the index has the same name, reuse it.
                if let Some(index) = index.name.as_deref() {
                    if table
                        .get("name")
                        .and_then(|name| name.as_str())
                        .is_some_and(|name| name == index)
                    {
                        return true;
                    }
                }

                // If the index is the default, and there's another default index, reuse it.
                if index.default
                    && table
                        .get("default")
                        .is_some_and(|default| default.as_bool() == Some(true))
                {
                    return true;
                }

                // If there's another index with the same URL, reuse it.
                if table
                    .get("url")
                    .and_then(|item| item.as_str())
                    .and_then(|url| DisplaySafeUrl::parse(url).ok())
                    .is_some_and(|url| {
                        CanonicalUrl::new(&url) == CanonicalUrl::new(index.url.url())
                    })
                {
                    return true;
                }

                false
            })
            .cloned()
            .unwrap_or_default();

        // If necessary, update the name.
        if let Some(index) = index.name.as_deref() {
            if table
                .get("name")
                .and_then(|name| name.as_str())
                .is_none_or(|name| name != index)
            {
                let mut formatted = Formatted::new(index.to_string());
                if let Some(value) = table.get("name").and_then(Item::as_value) {
                    if let Some(prefix) = value.decor().prefix() {
                        formatted.decor_mut().set_prefix(prefix.clone());
                    }
                    if let Some(suffix) = value.decor().suffix() {
                        formatted.decor_mut().set_suffix(suffix.clone());
                    }
                }
                table.insert("name", Value::String(formatted).into());
            }
        }

        // If necessary, update the URL.
        if table
            .get("url")
            .and_then(|item| item.as_str())
            .and_then(|url| DisplaySafeUrl::parse(url).ok())
            .is_none_or(|url| CanonicalUrl::new(&url) != CanonicalUrl::new(index.url.url()))
        {
            let mut formatted = Formatted::new(index.url.without_credentials().to_string());
            if let Some(value) = table.get("url").and_then(Item::as_value) {
                if let Some(prefix) = value.decor().prefix() {
                    formatted.decor_mut().set_prefix(prefix.clone());
                }
                if let Some(suffix) = value.decor().suffix() {
                    formatted.decor_mut().set_suffix(suffix.clone());
                }
            }
            table.insert("url", Value::String(formatted).into());
        }

        // If necessary, update the default.
        if index.default {
            if !table
                .get("default")
                .and_then(Item::as_bool)
                .is_some_and(|default| default)
            {
                let mut formatted = Formatted::new(true);
                if let Some(value) = table.get("default").and_then(Item::as_value) {
                    if let Some(prefix) = value.decor().prefix() {
                        formatted.decor_mut().set_prefix(prefix.clone());
                    }
                    if let Some(suffix) = value.decor().suffix() {
                        formatted.decor_mut().set_suffix(suffix.clone());
                    }
                }
                table.insert("default", Value::Boolean(formatted).into());
            }
        }

        // Remove any replaced tables.
        existing.retain(|table| {
            // If the index has the same name, skip it.
            if let Some(index) = index.name.as_deref() {
                if table
                    .get("name")
                    .and_then(|name| name.as_str())
                    .is_some_and(|name| name == index)
                {
                    return false;
                }
            }

            // If there's another default index, skip it.
            if index.default
                && table
                    .get("default")
                    .is_some_and(|default| default.as_bool() == Some(true))
            {
                return false;
            }

            // If there's another index with the same URL, skip it.
            if table
                .get("url")
                .and_then(|item| item.as_str())
                .and_then(|url| DisplaySafeUrl::parse(url).ok())
                .is_some_and(|url| CanonicalUrl::new(&url) == CanonicalUrl::new(index.url.url()))
            {
                return false;
            }

            true
        });

        // Set the position to the minimum, if it's not already the first element.
        if let Some(min) = existing.iter().filter_map(Table::position).min() {
            table.set_position(min);

            // Increment the position of all existing elements.
            for table in existing.iter_mut() {
                if let Some(position) = table.position() {
                    table.set_position(position + 1);
                }
            }
        }

        // Push the item to the table.
        existing.push(table);

        Ok(())
    }

    /// Adds a dependency to `project.optional-dependencies`.
    ///
    /// Returns `true` if the dependency was added, `false` if it was updated.
    pub fn add_optional_dependency(
        &mut self,
        group: &ExtraName,
        req: &Requirement,
        source: Option<&Source>,
        raw: bool,
    ) -> Result<ArrayEdit, Error> {
        // Get or create `project.optional-dependencies`.
        let optional_dependencies = self
            .project()?
            .entry("optional-dependencies")
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .ok_or(Error::MalformedDependencies)?;

        // Try to find the existing group.
        let existing_group = optional_dependencies.iter_mut().find_map(|(key, value)| {
            if ExtraName::from_str(key.get()).is_ok_and(|g| g == *group) {
                Some(value)
            } else {
                None
            }
        });

        // If the group doesn't exist, create it.
        let group = match existing_group {
            Some(value) => value,
            None => optional_dependencies
                .entry(group.as_ref())
                .or_insert(Item::Value(Value::Array(Array::new()))),
        }
        .as_array_mut()
        .ok_or(Error::MalformedDependencies)?;

        let added = add_dependency(req, group, source.is_some(), raw)?;

        // If `project.optional-dependencies` is an inline table, reformat it.
        //
        // Reformatting can drop comments between keys, but you can't put comments
        // between items in an inline table anyway.
        if let Some(optional_dependencies) = self
            .project()?
            .get_mut("optional-dependencies")
            .and_then(Item::as_inline_table_mut)
        {
            optional_dependencies.fmt();
        }

        if let Some(source) = source {
            self.add_source(&req.name, source)?;
        }

        Ok(added)
    }

    /// Adds a dependency to `dependency-groups`.
    ///
    /// Returns `true` if the dependency was added, `false` if it was updated.
    pub fn add_dependency_group_requirement(
        &mut self,
        group: &GroupName,
        req: &Requirement,
        source: Option<&Source>,
        raw: bool,
    ) -> Result<ArrayEdit, Error> {
        // Get or create `dependency-groups`.
        let dependency_groups = self
            .doc
            .entry("dependency-groups")
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .ok_or(Error::MalformedDependencies)?;

        let was_sorted = dependency_groups
            .get_values()
            .iter()
            .filter_map(|(dotted_ks, _)| dotted_ks.first())
            .map(|k| k.get())
            .is_sorted();

        // Try to find the existing group.
        let existing_group = dependency_groups.iter_mut().find_map(|(key, value)| {
            if GroupName::from_str(key.get()).is_ok_and(|g| g == *group) {
                Some(value)
            } else {
                None
            }
        });

        // If the group doesn't exist, create it.
        let group = match existing_group {
            Some(value) => value,
            None => dependency_groups
                .entry(group.as_ref())
                .or_insert(Item::Value(Value::Array(Array::new()))),
        }
        .as_array_mut()
        .ok_or(Error::MalformedDependencies)?;

        let added = add_dependency(req, group, source.is_some(), raw)?;

        // To avoid churn in pyproject.toml, we only sort new group keys if the
        // existing keys were sorted.
        if was_sorted {
            dependency_groups.sort_values();
        }

        // If `dependency-groups` is an inline table, reformat it.
        //
        // Reformatting can drop comments between keys, but you can't put comments
        // between items in an inline table anyway.
        if let Some(dependency_groups) = self
            .doc
            .get_mut("dependency-groups")
            .and_then(Item::as_inline_table_mut)
        {
            dependency_groups.fmt();
        }

        if let Some(source) = source {
            self.add_source(&req.name, source)?;
        }

        Ok(added)
    }

    /// Set the constraint for a requirement for an existing dependency.
    pub fn set_dependency_bound(
        &mut self,
        dependency_type: &DependencyType,
        index: usize,
        version: Version,
        bound_kind: AddBoundsKind,
    ) -> Result<(), Error> {
        let group = match dependency_type {
            DependencyType::Production => self.dependencies_array()?,
            DependencyType::Dev => self.dev_dependencies_array()?,
            DependencyType::Optional(extra) => self.optional_dependencies_array(extra)?,
            DependencyType::Group(group) => self.dependency_groups_array(group)?,
        };

        let Some(req) = group.get(index) else {
            return Err(Error::MissingDependency(index));
        };

        let mut req = req
            .as_str()
            .and_then(try_parse_requirement)
            .ok_or(Error::MalformedDependencies)?;
        req.version_or_url = Some(VersionOrUrl::VersionSpecifier(
            bound_kind.specifiers(version),
        ));
        group.replace(index, req.to_string());

        Ok(())
    }

    /// Get the TOML array for `project.dependencies`.
    fn dependencies_array(&mut self) -> Result<&mut Array, Error> {
        // Get or create `project.dependencies`.
        let dependencies = self
            .project()?
            .entry("dependencies")
            .or_insert(Item::Value(Value::Array(Array::new())))
            .as_array_mut()
            .ok_or(Error::MalformedDependencies)?;

        Ok(dependencies)
    }

    /// Get the TOML array for `tool.uv.dev-dependencies`.
    fn dev_dependencies_array(&mut self) -> Result<&mut Array, Error> {
        // Get or create `tool.uv.dev-dependencies`.
        let dev_dependencies = self
            .doc
            .entry("tool")
            .or_insert(implicit())
            .as_table_mut()
            .ok_or(Error::MalformedSources)?
            .entry("uv")
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or(Error::MalformedSources)?
            .entry("dev-dependencies")
            .or_insert(Item::Value(Value::Array(Array::new())))
            .as_array_mut()
            .ok_or(Error::MalformedDependencies)?;

        Ok(dev_dependencies)
    }

    /// Get the TOML array for a `project.optional-dependencies` entry.
    fn optional_dependencies_array(&mut self, group: &ExtraName) -> Result<&mut Array, Error> {
        // Get or create `project.optional-dependencies`.
        let optional_dependencies = self
            .project()?
            .entry("optional-dependencies")
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .ok_or(Error::MalformedDependencies)?;

        // Try to find the existing extra.
        let existing_key = optional_dependencies.iter().find_map(|(key, _value)| {
            if ExtraName::from_str(key).is_ok_and(|g| g == *group) {
                Some(key.to_string())
            } else {
                None
            }
        });

        // If the group doesn't exist, create it.
        let group = optional_dependencies
            .entry(existing_key.as_deref().unwrap_or(group.as_ref()))
            .or_insert(Item::Value(Value::Array(Array::new())))
            .as_array_mut()
            .ok_or(Error::MalformedDependencies)?;

        Ok(group)
    }

    /// Get the TOML array for a `dependency-groups` entry.
    fn dependency_groups_array(&mut self, group: &GroupName) -> Result<&mut Array, Error> {
        // Get or create `dependency-groups`.
        let dependency_groups = self
            .doc
            .entry("dependency-groups")
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .ok_or(Error::MalformedDependencies)?;

        // Try to find the existing group.
        let existing_key = dependency_groups.iter().find_map(|(key, _value)| {
            if GroupName::from_str(key).is_ok_and(|g| g == *group) {
                Some(key.to_string())
            } else {
                None
            }
        });

        // If the group doesn't exist, create it.
        let group = dependency_groups
            .entry(existing_key.as_deref().unwrap_or(group.as_ref()))
            .or_insert(Item::Value(Value::Array(Array::new())))
            .as_array_mut()
            .ok_or(Error::MalformedDependencies)?;

        Ok(group)
    }

    /// Adds a source to `tool.uv.sources`.
    fn add_source(&mut self, name: &PackageName, source: &Source) -> Result<(), Error> {
        // Get or create `tool.uv.sources`.
        let sources = self
            .doc
            .entry("tool")
            .or_insert(implicit())
            .as_table_mut()
            .ok_or(Error::MalformedSources)?
            .entry("uv")
            .or_insert(implicit())
            .as_table_mut()
            .ok_or(Error::MalformedSources)?
            .entry("sources")
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or(Error::MalformedSources)?;

        if let Some(key) = find_source(name, sources) {
            sources.remove(&key);
        }
        add_source(name, source, sources)?;

        Ok(())
    }

    /// Removes all occurrences of dependencies with the given name.
    pub fn remove_dependency(&mut self, name: &PackageName) -> Result<Vec<Requirement>, Error> {
        // Try to get `project.dependencies`.
        let Some(dependencies) = self
            .project_mut()?
            .and_then(|project| project.get_mut("dependencies"))
            .map(|dependencies| {
                dependencies
                    .as_array_mut()
                    .ok_or(Error::MalformedDependencies)
            })
            .transpose()?
        else {
            return Ok(Vec::new());
        };

        let requirements = remove_dependency(name, dependencies);
        self.remove_source(name)?;

        Ok(requirements)
    }

    /// Removes all occurrences of development dependencies with the given name.
    pub fn remove_dev_dependency(&mut self, name: &PackageName) -> Result<Vec<Requirement>, Error> {
        // Try to get `tool.uv.dev-dependencies`.
        let Some(dev_dependencies) = self
            .doc
            .get_mut("tool")
            .map(|tool| tool.as_table_mut().ok_or(Error::MalformedDependencies))
            .transpose()?
            .and_then(|tool| tool.get_mut("uv"))
            .map(|tool_uv| tool_uv.as_table_mut().ok_or(Error::MalformedDependencies))
            .transpose()?
            .and_then(|tool_uv| tool_uv.get_mut("dev-dependencies"))
            .map(|dependencies| {
                dependencies
                    .as_array_mut()
                    .ok_or(Error::MalformedDependencies)
            })
            .transpose()?
        else {
            return Ok(Vec::new());
        };

        let requirements = remove_dependency(name, dev_dependencies);
        self.remove_source(name)?;

        Ok(requirements)
    }

    /// Removes all occurrences of optional dependencies in the group with the given name.
    pub fn remove_optional_dependency(
        &mut self,
        name: &PackageName,
        group: &ExtraName,
    ) -> Result<Vec<Requirement>, Error> {
        // Try to get `project.optional-dependencies.<group>`.
        let Some(optional_dependencies) = self
            .project_mut()?
            .and_then(|project| project.get_mut("optional-dependencies"))
            .map(|extras| {
                extras
                    .as_table_like_mut()
                    .ok_or(Error::MalformedDependencies)
            })
            .transpose()?
            .and_then(|extras| {
                extras.iter_mut().find_map(|(key, value)| {
                    if ExtraName::from_str(key.get()).is_ok_and(|g| g == *group) {
                        Some(value)
                    } else {
                        None
                    }
                })
            })
            .map(|dependencies| {
                dependencies
                    .as_array_mut()
                    .ok_or(Error::MalformedDependencies)
            })
            .transpose()?
        else {
            return Ok(Vec::new());
        };

        let requirements = remove_dependency(name, optional_dependencies);
        self.remove_source(name)?;

        Ok(requirements)
    }

    /// Removes all occurrences of the dependency in the group with the given name.
    pub fn remove_dependency_group_requirement(
        &mut self,
        name: &PackageName,
        group: &GroupName,
    ) -> Result<Vec<Requirement>, Error> {
        // Try to get `project.optional-dependencies.<group>`.
        let Some(group_dependencies) = self
            .doc
            .get_mut("dependency-groups")
            .map(|groups| {
                groups
                    .as_table_like_mut()
                    .ok_or(Error::MalformedDependencies)
            })
            .transpose()?
            .and_then(|groups| {
                groups.iter_mut().find_map(|(key, value)| {
                    if GroupName::from_str(key.get()).is_ok_and(|g| g == *group) {
                        Some(value)
                    } else {
                        None
                    }
                })
            })
            .map(|dependencies| {
                dependencies
                    .as_array_mut()
                    .ok_or(Error::MalformedDependencies)
            })
            .transpose()?
        else {
            return Ok(Vec::new());
        };

        let requirements = remove_dependency(name, group_dependencies);
        self.remove_source(name)?;

        Ok(requirements)
    }

    /// Remove a matching source from `tool.uv.sources`, if it exists.
    fn remove_source(&mut self, name: &PackageName) -> Result<(), Error> {
        // If the dependency is still in use, don't remove the source.
        if !self.find_dependency(name, None).is_empty() {
            return Ok(());
        }

        if let Some(sources) = self
            .doc
            .get_mut("tool")
            .map(|tool| tool.as_table_mut().ok_or(Error::MalformedSources))
            .transpose()?
            .and_then(|tool| tool.get_mut("uv"))
            .map(|tool_uv| tool_uv.as_table_mut().ok_or(Error::MalformedSources))
            .transpose()?
            .and_then(|tool_uv| tool_uv.get_mut("sources"))
            .map(|sources| sources.as_table_mut().ok_or(Error::MalformedSources))
            .transpose()?
        {
            if let Some(key) = find_source(name, sources) {
                sources.remove(&key);

                // Remove the `tool.uv.sources` table if it is empty.
                if sources.is_empty() {
                    self.doc
                        .entry("tool")
                        .or_insert(implicit())
                        .as_table_mut()
                        .ok_or(Error::MalformedSources)?
                        .entry("uv")
                        .or_insert(implicit())
                        .as_table_mut()
                        .ok_or(Error::MalformedSources)?
                        .remove("sources");
                }
            }
        }

        Ok(())
    }

    /// Returns `true` if the `tool.uv.dev-dependencies` table is present.
    pub fn has_dev_dependencies(&self) -> bool {
        self.doc
            .get("tool")
            .and_then(Item::as_table)
            .and_then(|tool| tool.get("uv"))
            .and_then(Item::as_table)
            .and_then(|uv| uv.get("dev-dependencies"))
            .is_some()
    }

    /// Returns `true` if the `dependency-groups` table is present and contains the given group.
    pub fn has_dependency_group(&self, group: &GroupName) -> bool {
        self.doc
            .get("dependency-groups")
            .and_then(Item::as_table)
            .and_then(|groups| groups.get(group.as_ref()))
            .is_some()
    }

    /// Returns all the places in this `pyproject.toml` that contain a dependency with the given
    /// name.
    ///
    /// This method searches `project.dependencies`, `tool.uv.dev-dependencies`, and
    /// `tool.uv.optional-dependencies`.
    pub fn find_dependency(
        &self,
        name: &PackageName,
        marker: Option<&MarkerTree>,
    ) -> Vec<DependencyType> {
        let mut types = Vec::new();

        if let Some(project) = self.doc.get("project").and_then(Item::as_table) {
            // Check `project.dependencies`.
            if let Some(dependencies) = project.get("dependencies").and_then(Item::as_array) {
                if !find_dependencies(name, marker, dependencies).is_empty() {
                    types.push(DependencyType::Production);
                }
            }

            // Check `project.optional-dependencies`.
            if let Some(extras) = project
                .get("optional-dependencies")
                .and_then(Item::as_table)
            {
                for (extra, dependencies) in extras {
                    let Some(dependencies) = dependencies.as_array() else {
                        continue;
                    };
                    let Ok(extra) = ExtraName::from_str(extra) else {
                        continue;
                    };

                    if !find_dependencies(name, marker, dependencies).is_empty() {
                        types.push(DependencyType::Optional(extra));
                    }
                }
            }
        }

        // Check `dependency-groups`.
        if let Some(groups) = self.doc.get("dependency-groups").and_then(Item::as_table) {
            for (group, dependencies) in groups {
                let Some(dependencies) = dependencies.as_array() else {
                    continue;
                };
                let Ok(group) = GroupName::from_str(group) else {
                    continue;
                };

                if !find_dependencies(name, marker, dependencies).is_empty() {
                    types.push(DependencyType::Group(group));
                }
            }
        }

        // Check `tool.uv.dev-dependencies`.
        if let Some(dev_dependencies) = self
            .doc
            .get("tool")
            .and_then(Item::as_table)
            .and_then(|tool| tool.get("uv"))
            .and_then(Item::as_table)
            .and_then(|uv| uv.get("dev-dependencies"))
            .and_then(Item::as_array)
        {
            if !find_dependencies(name, marker, dev_dependencies).is_empty() {
                types.push(DependencyType::Dev);
            }
        }

        types
    }

    pub fn version(&mut self) -> Result<Version, Error> {
        let version = self
            .doc
            .get("project")
            .and_then(Item::as_table)
            .and_then(|project| project.get("version"))
            .and_then(Item::as_str)
            .ok_or(Error::MalformedWorkspace)?;

        Ok(Version::from_str(version)?)
    }

    pub fn has_dynamic_version(&mut self) -> bool {
        let Some(dynamic) = self
            .doc
            .get("project")
            .and_then(Item::as_table)
            .and_then(|project| project.get("dynamic"))
            .and_then(Item::as_array)
        else {
            return false;
        };

        dynamic.iter().any(|val| val.as_str() == Some("version"))
    }

    pub fn set_version(&mut self, version: &Version) -> Result<(), Error> {
        let project = self
            .doc
            .get_mut("project")
            .and_then(Item::as_table_mut)
            .ok_or(Error::MalformedWorkspace)?;
        project.insert(
            "version",
            Item::Value(Value::String(Formatted::new(version.to_string()))),
        );

        Ok(())
    }
}

/// Returns an implicit table.
fn implicit() -> Item {
    let mut table = Table::new();
    table.set_implicit(true);
    Item::Table(table)
}

/// Adds a dependency to the given `deps` array.
///
/// Returns `true` if the dependency was added, `false` if it was updated.
pub fn add_dependency(
    req: &Requirement,
    deps: &mut Array,
    has_source: bool,
    raw: bool,
) -> Result<ArrayEdit, Error> {
    let mut to_replace = find_dependencies(&req.name, Some(&req.marker), deps);

    match to_replace.as_slice() {
        [] => {
            #[derive(Debug, Copy, Clone)]
            enum Sort {
                /// The list is sorted in a case-insensitive manner.
                CaseInsensitive,
                /// The list is sorted naively in a case-insensitive manner.
                CaseInsensitiveNaive,
                /// The list is sorted in a case-sensitive manner.
                CaseSensitive,
                /// The list is sorted naively in a case-sensitive manner.
                CaseSensitiveNaive,
                /// The list is unsorted.
                Unsorted,
            }

            fn is_sorted<T, I>(items: I) -> bool
            where
                I: IntoIterator<Item = T>,
                T: PartialOrd + Copy,
            {
                items.into_iter().tuple_windows().all(|(a, b)| a <= b)
            }

            // `deps` are either requirements (strings) or include groups (inline tables).
            // Here we pull out just the requirements for determining the sort.
            let reqs: Vec<_> = deps.iter().filter_map(Value::as_str).collect();
            let reqs_lowercase: Vec<_> = reqs.iter().copied().map(str::to_lowercase).collect();

            // Determine if the dependency list is sorted prior to
            // adding the new dependency; the new dependency list
            // will be sorted only when the original list is sorted
            // so that user's custom dependency ordering is preserved.
            //
            // Any items which aren't strings are ignored, e.g.
            // `{ include-group = "..." }` in dependency-groups.
            //
            // We account for both case-sensitive and case-insensitive sorting.
            let sort = if is_sorted(
                reqs_lowercase
                    .iter()
                    .map(String::as_str)
                    .map(split_specifiers),
            ) {
                Sort::CaseInsensitive
            } else if is_sorted(reqs.iter().copied().map(split_specifiers)) {
                Sort::CaseSensitive
            } else if is_sorted(reqs_lowercase.iter().map(String::as_str)) {
                Sort::CaseInsensitiveNaive
            } else if is_sorted(reqs) {
                Sort::CaseSensitiveNaive
            } else {
                Sort::Unsorted
            };

            let req_string = if raw {
                req.displayable_with_credentials().to_string()
            } else {
                req.to_string()
            };
            let index = match sort {
                Sort::CaseInsensitive => deps.iter().position(|dep| {
                    dep.as_str().is_some_and(|dep| {
                        split_specifiers(&dep.to_lowercase())
                            > split_specifiers(&req_string.to_lowercase())
                    })
                }),
                Sort::CaseInsensitiveNaive => deps.iter().position(|dep| {
                    dep.as_str()
                        .is_some_and(|dep| dep.to_lowercase() > req_string.to_lowercase())
                }),
                Sort::CaseSensitive => deps.iter().position(|dep| {
                    dep.as_str()
                        .is_some_and(|dep| split_specifiers(dep) > split_specifiers(&req_string))
                }),
                Sort::CaseSensitiveNaive => deps
                    .iter()
                    .position(|dep| dep.as_str().is_some_and(|dep| *dep > *req_string)),
                Sort::Unsorted => None,
            };
            let index = index.unwrap_or_else(|| {
                // The dependency should be added to the end, ignoring any
                // `include-group` items. This preserves the order for users who
                // keep their `include-groups` at the bottom.
                deps.iter()
                    .enumerate()
                    .filter_map(|(i, dep)| if dep.is_str() { Some(i + 1) } else { None })
                    .last()
                    .unwrap_or(deps.len())
            });

            let mut value = Value::from(req_string.as_str());

            let decor = value.decor_mut();

            // Ensure comments remain on the correct line, post-insertion
            match index {
                val if val == deps.len() => {
                    // If we're adding to the end of the list, treat trailing comments as leading comments
                    // on the added dependency.
                    //
                    // For example, given:
                    // ```toml
                    // dependencies = [
                    //     "anyio", # trailing comment
                    // ]
                    // ```
                    //
                    // If we add `flask` to the end, we want to retain the comment on `anyio`:
                    // ```toml
                    // dependencies = [
                    //     "anyio", # trailing comment
                    //     "flask",
                    // ]
                    // ```
                    decor.set_prefix(deps.trailing().clone());
                    deps.set_trailing("");
                }
                0 => {
                    // If the dependency is prepended to a non-empty list, do nothing
                }
                val => {
                    // Retain position of end-of-line comments when a dependency is inserted right below it.
                    //
                    // For example, given:
                    // ```toml
                    // dependencies = [
                    //     "anyio", # end-of-line comment
                    //     "flask",
                    // ]
                    // ```
                    //
                    // If we add `pydantic` (between `anyio` and `flask`), we want to retain the comment on `anyio`:
                    // ```toml
                    // dependencies = [
                    //     "anyio", # end-of-line comment
                    //     "pydantic",
                    //     "flask",
                    // ]
                    // ```
                    let targeted_decor = deps.get_mut(val).unwrap().decor_mut();
                    decor.set_prefix(targeted_decor.prefix().unwrap().clone());
                    targeted_decor.set_prefix(""); // Re-formatted later by `reformat_array_multiline`
                }
            }

            deps.insert_formatted(index, value);

            // `reformat_array_multiline` uses the indentation of the first dependency entry.
            // Therefore, we retrieve the indentation of the first dependency entry and apply it to
            // the new entry. Note that it is only necessary if the newly added dependency is going
            // to be the first in the list _and_ the dependency list was not empty prior to adding
            // the new dependency.
            if deps.len() > 1 && index == 0 {
                let prefix = deps
                    .clone()
                    .get(index + 1)
                    .unwrap()
                    .decor()
                    .prefix()
                    .unwrap()
                    .clone();

                // However, if the prefix includes a comment, we don't want to duplicate it.
                // Depending on the location of the comment, we either want to leave it as-is, or
                // attach it to the entry that's being moved to the next line.
                //
                // For example, given:
                // ```toml
                // dependencies = [ # comment
                //     "flask",
                // ]
                // ```
                //
                // If we add `anyio` to the beginning, we want to retain the comment on the open
                // bracket:
                // ```toml
                // dependencies = [ # comment
                //     "anyio",
                //     "flask",
                // ]
                // ```
                //
                // However, given:
                // ```toml
                // dependencies = [
                //     # comment
                //     "flask",
                // ]
                // ```
                //
                // If we add `anyio` to the beginning, we want the comment to move down with the
                // existing entry:
                // entry:
                // ```toml
                // dependencies = [
                //     "anyio",
                //     # comment
                //     "flask",
                // ]
                if let Some(prefix) = prefix.as_str() {
                    // Treat anything before the first own-line comment as a prefix on the new
                    // entry; anything after the first own-line comment is a prefix on the existing
                    // entry.
                    //
                    // This is equivalent to using the first and last line content as the prefix for
                    // the new entry, and the rest as the prefix for the existing entry.
                    if let Some((first_line, rest)) = prefix.split_once(['\r', '\n']) {
                        // Determine the appropriate newline character.
                        let newline = {
                            let mut chars = prefix[first_line.len()..].chars();
                            match (chars.next(), chars.next()) {
                                (Some('\r'), Some('\n')) => "\r\n",
                                (Some('\r'), _) => "\r",
                                (Some('\n'), _) => "\n",
                                _ => "\n",
                            }
                        };
                        let last_line = rest.lines().last().unwrap_or_default();
                        let prefix = format!("{first_line}{newline}{last_line}");
                        deps.get_mut(index).unwrap().decor_mut().set_prefix(prefix);

                        let prefix = format!("{newline}{rest}");
                        deps.get_mut(index + 1)
                            .unwrap()
                            .decor_mut()
                            .set_prefix(prefix);
                    } else {
                        deps.get_mut(index).unwrap().decor_mut().set_prefix(prefix);
                    }
                } else {
                    deps.get_mut(index).unwrap().decor_mut().set_prefix(prefix);
                }
            }

            reformat_array_multiline(deps);

            Ok(ArrayEdit::Add(index))
        }
        [_] => {
            let (i, mut old_req) = to_replace.remove(0);
            update_requirement(&mut old_req, req, has_source);
            deps.replace(i, old_req.to_string());
            reformat_array_multiline(deps);
            Ok(ArrayEdit::Update(i))
        }
        // Cannot perform ambiguous updates.
        _ => Err(Error::Ambiguous {
            package_name: req.name.clone(),
            requirements: to_replace
                .into_iter()
                .map(|(_, requirement)| requirement)
                .collect(),
        }),
    }
}

/// Update an existing requirement.
fn update_requirement(old: &mut Requirement, new: &Requirement, has_source: bool) {
    // Add any new extras.
    let mut extras = old.extras.to_vec();
    extras.extend(new.extras.iter().cloned());
    extras.sort_unstable();
    extras.dedup();
    old.extras = extras.into_boxed_slice();

    // Clear the requirement source if we are going to add to `tool.uv.sources`.
    if has_source {
        old.clear_url();
    }

    // Update the source if a new one was specified.
    match &new.version_or_url {
        None => {}
        Some(VersionOrUrl::VersionSpecifier(specifier)) if specifier.is_empty() => {}
        Some(version_or_url) => old.version_or_url = Some(version_or_url.clone()),
    }

    // Update the marker expression.
    if new.marker.contents().is_some() {
        old.marker = new.marker;
    }
}

/// Removes all occurrences of dependencies with the given name from the given `deps` array.
fn remove_dependency(name: &PackageName, deps: &mut Array) -> Vec<Requirement> {
    // Remove matching dependencies.
    let removed = find_dependencies(name, None, deps)
        .into_iter()
        .rev() // Reverse to preserve indices as we remove them.
        .filter_map(|(i, _)| {
            deps.remove(i)
                .as_str()
                .and_then(|req| Requirement::from_str(req).ok())
        })
        .collect::<Vec<_>>();

    if !removed.is_empty() {
        reformat_array_multiline(deps);
    }

    removed
}

/// Returns a `Vec` containing the all dependencies with the given name, along with their positions
/// in the array.
fn find_dependencies(
    name: &PackageName,
    marker: Option<&MarkerTree>,
    deps: &Array,
) -> Vec<(usize, Requirement)> {
    let mut to_replace = Vec::new();
    for (i, dep) in deps.iter().enumerate() {
        if let Some(req) = dep.as_str().and_then(try_parse_requirement) {
            if marker.is_none_or(|m| *m == req.marker) && *name == req.name {
                to_replace.push((i, req));
            }
        }
    }
    to_replace
}

/// Returns the key in `tool.uv.sources` that matches the given package name.
fn find_source(name: &PackageName, sources: &Table) -> Option<String> {
    for (key, _) in sources {
        if PackageName::from_str(key).is_ok_and(|ref key| key == name) {
            return Some(key.to_string());
        }
    }
    None
}

// Add a source to `tool.uv.sources`.
fn add_source(req: &PackageName, source: &Source, sources: &mut Table) -> Result<(), Error> {
    // Serialize as an inline table.
    let mut doc = toml::to_string(&source)
        .map_err(Box::new)?
        .parse::<DocumentMut>()
        .unwrap();
    let table = mem::take(doc.as_table_mut()).into_inline_table();

    sources.insert(req.as_ref(), Item::Value(Value::InlineTable(table)));

    Ok(())
}

impl fmt::Display for PyProjectTomlMut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.doc.fmt(f)
    }
}

fn try_parse_requirement(req: &str) -> Option<Requirement> {
    Requirement::from_str(req).ok()
}

/// Reformats a TOML array to multi line while trying to preserve all comments
/// and move them around. This also formats the array to have a trailing comma.
fn reformat_array_multiline(deps: &mut Array) {
    fn find_comments(s: Option<&RawString>) -> Box<dyn Iterator<Item = Comment> + '_> {
        let iter = s
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .lines()
            .scan(
                (false, false),
                |(prev_line_was_empty, prev_line_was_comment), line| {
                    let trimmed_line = line.trim();
                    if let Some(index) = trimmed_line.find('#') {
                        let comment_text = trimmed_line[index..].trim().to_string();
                        let comment_type = if (*prev_line_was_empty) || (*prev_line_was_comment) {
                            CommentType::OwnLine
                        } else {
                            CommentType::EndOfLine
                        };
                        *prev_line_was_empty = trimmed_line.is_empty();
                        *prev_line_was_comment = true;
                        Some(Some(Comment {
                            text: comment_text,
                            comment_type,
                        }))
                    } else {
                        *prev_line_was_empty = trimmed_line.is_empty();
                        *prev_line_was_comment = false;
                        Some(None)
                    }
                },
            )
            .flatten();

        Box::new(iter)
    }

    let mut indentation_prefix = None;

    for item in deps.iter_mut() {
        let decor = item.decor_mut();
        let mut prefix = String::new();

        // Calculate the indentation prefix based on the indentation of the first dependency entry.
        if indentation_prefix.is_none() {
            let decor_prefix = decor
                .prefix()
                .and_then(|s| s.as_str())
                .and_then(|s| s.lines().last())
                .unwrap_or_default();

            let decor_prefix = decor_prefix
                .split_once('#')
                .map(|(s, _)| s)
                .unwrap_or(decor_prefix);

            indentation_prefix = (!decor_prefix.is_empty()).then_some(decor_prefix.to_string());
        }

        let indentation_prefix_str =
            format!("\n{}", indentation_prefix.as_deref().unwrap_or("    "));

        for comment in find_comments(decor.prefix()).chain(find_comments(decor.suffix())) {
            match comment.comment_type {
                CommentType::OwnLine => {
                    prefix.push_str(&indentation_prefix_str);
                }
                CommentType::EndOfLine => {
                    prefix.push(' ');
                }
            }
            prefix.push_str(&comment.text);
        }
        prefix.push_str(&indentation_prefix_str);
        decor.set_prefix(prefix);
        decor.set_suffix("");
    }

    deps.set_trailing(&{
        let mut comments = find_comments(Some(deps.trailing())).peekable();
        let mut rv = String::new();
        if comments.peek().is_some() {
            for comment in comments {
                match comment.comment_type {
                    CommentType::OwnLine => {
                        let indentation_prefix_str =
                            format!("\n{}", indentation_prefix.as_deref().unwrap_or("    "));
                        rv.push_str(&indentation_prefix_str);
                    }
                    CommentType::EndOfLine => {
                        rv.push(' ');
                    }
                }
                rv.push_str(&comment.text);
            }
        }
        if !rv.is_empty() || !deps.is_empty() {
            rv.push('\n');
        }
        rv
    });
    deps.set_trailing_comma(true);
}

/// Split a requirement into the package name and its dependency specifiers.
///
/// E.g., given `flask>=1.0`, this function returns `("flask", ">=1.0")`. But given
/// `Flask>=1.0`, this function returns `("Flask", ">=1.0")`.
///
/// Extras are retained, such that `flask[dotenv]>=1.0` returns `("flask[dotenv]", ">=1.0")`.
fn split_specifiers(req: &str) -> (&str, &str) {
    let (name, specifiers) = req
        .find(['>', '<', '=', '~', '!', '@'])
        .map_or((req, ""), |pos| {
            let (name, specifiers) = req.split_at(pos);
            (name, specifiers)
        });
    (name.trim(), specifiers.trim())
}

#[cfg(test)]
mod test {
    use super::{AddBoundsKind, split_specifiers};
    use std::str::FromStr;
    use uv_pep440::Version;

    #[test]
    fn split() {
        assert_eq!(split_specifiers("flask>=1.0"), ("flask", ">=1.0"));
        assert_eq!(split_specifiers("Flask>=1.0"), ("Flask", ">=1.0"));
        assert_eq!(
            split_specifiers("flask[dotenv]>=1.0"),
            ("flask[dotenv]", ">=1.0")
        );
        assert_eq!(split_specifiers("flask[dotenv]"), ("flask[dotenv]", ""));
        assert_eq!(
            split_specifiers(
                "flask @ https://files.pythonhosted.org/packages/af/47/93213ee66ef8fae3b93b3e29206f6b251e65c97bd91d8e1c5596ef15af0a/flask-3.1.0-py3-none-any.whl"
            ),
            (
                "flask",
                "@ https://files.pythonhosted.org/packages/af/47/93213ee66ef8fae3b93b3e29206f6b251e65c97bd91d8e1c5596ef15af0a/flask-3.1.0-py3-none-any.whl"
            )
        );
    }

    #[test]
    fn bound_kind_to_specifiers_exact() {
        let tests = [
            ("0", "==0"),
            ("0.0", "==0.0"),
            ("0.0.0", "==0.0.0"),
            ("0.1", "==0.1"),
            ("0.0.1", "==0.0.1"),
            ("0.0.0.1", "==0.0.0.1"),
            ("1.0.0", "==1.0.0"),
            ("1.2", "==1.2"),
            ("1.2.3", "==1.2.3"),
            ("1.2.3.4", "==1.2.3.4"),
            ("1.2.3.4a1.post1", "==1.2.3.4a1.post1"),
        ];

        for (version, expected) in tests {
            let actual = AddBoundsKind::Exact
                .specifiers(Version::from_str(version).unwrap())
                .to_string();
            assert_eq!(actual, expected, "{version}");
        }
    }

    #[test]
    fn bound_kind_to_specifiers_lower() {
        let tests = [
            ("0", ">=0"),
            ("0.0", ">=0.0"),
            ("0.0.0", ">=0.0.0"),
            ("0.1", ">=0.1"),
            ("0.0.1", ">=0.0.1"),
            ("0.0.0.1", ">=0.0.0.1"),
            ("1", ">=1"),
            ("1.0.0", ">=1.0.0"),
            ("1.2", ">=1.2"),
            ("1.2.3", ">=1.2.3"),
            ("1.2.3.4", ">=1.2.3.4"),
            ("1.2.3.4a1.post1", ">=1.2.3.4a1.post1"),
        ];

        for (version, expected) in tests {
            let actual = AddBoundsKind::Lower
                .specifiers(Version::from_str(version).unwrap())
                .to_string();
            assert_eq!(actual, expected, "{version}");
        }
    }

    #[test]
    fn bound_kind_to_specifiers_major() {
        let tests = [
            ("0", ">=0, <0.1"),
            ("0.0", ">=0.0, <0.1"),
            ("0.0.0", ">=0.0.0, <0.1.0"),
            ("0.0.0.0", ">=0.0.0.0, <0.1.0.0"),
            ("0.1", ">=0.1, <0.2"),
            ("0.0.1", ">=0.0.1, <0.0.2"),
            ("0.0.1.1", ">=0.0.1.1, <0.0.2.0"),
            ("0.0.0.1", ">=0.0.0.1, <0.0.0.2"),
            ("1", ">=1, <2"),
            ("1.0.0", ">=1.0.0, <2.0.0"),
            ("1.2", ">=1.2, <2.0"),
            ("1.2.3", ">=1.2.3, <2.0.0"),
            ("1.2.3.4", ">=1.2.3.4, <2.0.0.0"),
            ("1.2.3.4a1.post1", ">=1.2.3.4a1.post1, <2.0.0.0"),
        ];

        for (version, expected) in tests {
            let actual = AddBoundsKind::Major
                .specifiers(Version::from_str(version).unwrap())
                .to_string();
            assert_eq!(actual, expected, "{version}");
        }
    }

    #[test]
    fn bound_kind_to_specifiers_minor() {
        let tests = [
            ("0", ">=0, <0.0.1"),
            ("0.0", ">=0.0, <0.0.1"),
            ("0.0.0", ">=0.0.0, <0.0.1"),
            ("0.0.0.0", ">=0.0.0.0, <0.0.1.0"),
            ("0.1", ">=0.1, <0.1.1"),
            ("0.0.1", ">=0.0.1, <0.0.2"),
            ("0.0.1.1", ">=0.0.1.1, <0.0.2.0"),
            ("0.0.0.1", ">=0.0.0.1, <0.0.0.2"),
            ("1", ">=1, <1.1"),
            ("1.0.0", ">=1.0.0, <1.1.0"),
            ("1.2", ">=1.2, <1.3"),
            ("1.2.3", ">=1.2.3, <1.3.0"),
            ("1.2.3.4", ">=1.2.3.4, <1.3.0.0"),
            ("1.2.3.4a1.post1", ">=1.2.3.4a1.post1, <1.3.0.0"),
        ];

        for (version, expected) in tests {
            let actual = AddBoundsKind::Minor
                .specifiers(Version::from_str(version).unwrap())
                .to_string();
            assert_eq!(actual, expected, "{version}");
        }
    }
}

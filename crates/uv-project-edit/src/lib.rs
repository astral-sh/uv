//! Edit project and script metadata while preserving TOML formatting.

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use itertools::Itertools;
use thiserror::Error;
use toml_edit::{
    Array, ArrayOfTables, DocumentMut, Formatted, Item, RawString, Table, TomlError, Value,
};

use uv_cache_key::CanonicalUrl;
use uv_configuration::AddBoundsKind;
use uv_distribution_types::{Index, IndexFormat, IndexUrl};
use uv_fs::{PortablePath, is_same_file_allow_missing, try_relative_to_if};
use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep440::{Version, VersionParseError};
use uv_pep508::{MarkerTree, Requirement, VersionOrUrl};
use uv_workspace::dependency_groups::FlatDependencyGroups;
use uv_workspace::pyproject::{DependencyType, PyProjectToml, Source};

/// Raw and mutable representation of a `pyproject.toml`.
///
/// This is useful for operations that require editing an existing `pyproject.toml` while
/// preserving comments and other structure, such as `uv add` and `uv remove`.
pub struct PyProjectTomlMut {
    doc: DocumentMut,
    target: DependencyTarget,
}

fn index_locations_equal(existing: &str, incoming: &IndexUrl, root_dir: &Path) -> bool {
    let Ok(existing) = IndexUrl::parse(existing, Some(root_dir)) else {
        return false;
    };

    if let (IndexUrl::Path(existing), IndexUrl::Path(incoming)) = (&existing, incoming)
        && let (Ok(existing), Ok(incoming)) = (existing.to_file_path(), incoming.to_file_path())
        && let Some(equal) = is_same_file_allow_missing(&existing, &incoming)
    {
        return equal;
    }

    CanonicalUrl::new(existing.url().clone()) == CanonicalUrl::new(incoming.url().clone())
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to parse `pyproject.toml`")]
    Parse(#[from] Box<TomlError>),
    #[error("Failed to serialize `pyproject.toml`")]
    Serialize(#[from] Box<toml_edit::ser::Error>),
    #[error("Failed to deserialize `pyproject.toml`")]
    Deserialize(#[from] Box<toml::de::Error>),
    #[error("Dependencies in `pyproject.toml` are malformed")]
    MalformedDependencies,
    #[error("Sources in `pyproject.toml` are malformed")]
    MalformedSources,
    #[error(
        "Cannot update the source for `{0}` because it overlaps another source in `tool.uv.sources`. Use disjoint markers or edit the sources manually."
    )]
    OverlappingSource(PackageName),
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum CommentType {
    /// A comment that appears on its own line.
    OwnLine,
    /// A comment that appears at the end of a line.
    EndOfLine { leading_whitespace: String },
}

#[derive(Debug, Clone)]
struct Comment {
    text: String,
    kind: CommentType,
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

    /// Replaces every exact match for a dependency declaration without modifying its source.
    ///
    /// Returns the position of every dependency that was replaced.
    pub fn replace_dependency_declaration(
        &mut self,
        dependency_type: &DependencyType,
        existing: &Requirement,
        replacement: &Requirement,
    ) -> Result<Vec<ArrayEdit>, Error> {
        let Some(dependencies) = self.dependency_type_array_mut(dependency_type)? else {
            return Ok(Vec::new());
        };

        let replacement = replacement.to_string();
        let mut edits = Vec::new();
        for (index, requirement) in
            find_dependencies(&existing.name, Some(&existing.marker), dependencies)
        {
            if same_requirement_declaration(&requirement, existing) {
                dependencies.replace(index, replacement.clone());
                edits.push(ArrayEdit::Update(index));
            }
        }
        Ok(edits)
    }

    /// Removes every exact string match for a dependency declaration without modifying its source.
    ///
    /// Returns the position of every dependency that was removed.
    pub fn remove_dependency_declaration_text(
        &mut self,
        dependency_type: &DependencyType,
        existing: &str,
    ) -> Result<Vec<ArrayEdit>, Error> {
        let Some(dependencies) = self.dependency_type_array_mut(dependency_type)? else {
            return Ok(Vec::new());
        };

        let mut edits = Vec::new();
        for index in dependencies
            .iter()
            .enumerate()
            .filter_map(|(index, dependency)| {
                (dependency.as_str() == Some(existing)).then_some(index)
            })
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            remove_dependency_at(index, dependencies);
            edits.push(ArrayEdit::Update(index));
        }
        if !edits.is_empty() {
            reformat_array_multiline(dependencies);
        }
        edits.reverse();
        Ok(edits)
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
    pub fn add_index(&mut self, index: &Index, root_dir: &Path) -> Result<(), Error> {
        let size = self.doc.len();
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
                if let Some(index) = index.name.as_deref()
                    && table
                        .get("name")
                        .and_then(|name| name.as_str())
                        .is_some_and(|name| name == index)
                {
                    return true;
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
                    .is_some_and(|url| index_locations_equal(url, &index.url, root_dir))
                {
                    return true;
                }

                false
            })
            .cloned()
            .unwrap_or_default();

        // If necessary, update the name.
        if let Some(index) = index.name.as_deref()
            && table
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

        let url = if let IndexUrl::Path(url) = &index.url
            && let Ok(path) = url.to_file_path()
            && let Ok(path) = try_relative_to_if(path, root_dir, url.prefers_relative())
        {
            PortablePath::from(&path).to_string()
        } else {
            index.url.without_credentials().to_string()
        };
        let existing_url = table.get("url").and_then(|item| item.as_str());

        // Update the stored URL independently of whether the index location changed.
        let url_needs_update = existing_url.is_none_or(|existing| existing != url);
        let index_location_changed = existing_url
            .is_none_or(|existing| !index_locations_equal(existing, &index.url, root_dir));

        // If necessary, update the URL.
        if url_needs_update {
            let mut formatted = Formatted::new(url);
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

        // If the index location changed, sync the format to match the incoming index.
        if index_location_changed {
            match index.format {
                IndexFormat::Flat => {
                    if table
                        .get("format")
                        .and_then(Item::as_str)
                        .is_none_or(|format| format != "flat")
                    {
                        let mut formatted = Formatted::new("flat".to_string());
                        if let Some(value) = table.get("format").and_then(Item::as_value) {
                            if let Some(prefix) = value.decor().prefix() {
                                formatted.decor_mut().set_prefix(prefix.clone());
                            }
                            if let Some(suffix) = value.decor().suffix() {
                                formatted.decor_mut().set_suffix(suffix.clone());
                            }
                        }
                        table.insert("format", Value::String(formatted).into());
                    }
                }
                IndexFormat::Simple => {
                    // Remove the format key if it exists (Simple is the default).
                    table.remove("format");
                }
            }
        }

        // Remove any replaced tables.
        existing.retain(|table| {
            // If the index has the same name, skip it.
            if let Some(index) = index.name.as_deref()
                && table
                    .get("name")
                    .and_then(|name| name.as_str())
                    .is_some_and(|name| name == index)
            {
                return false;
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
                .is_some_and(|url| index_locations_equal(url, &index.url, root_dir))
            {
                return false;
            }

            true
        });

        // Set the position to the minimum, if it's not already the first element.
        if let Some(min) = existing.iter().filter_map(Table::position).min() {
            table.set_position(Some(min));

            // Increment the position of all existing elements.
            for table in existing.iter_mut() {
                if let Some(position) = table.position() {
                    table.set_position(Some(position + 1));
                }
            }
        } else {
            let position = isize::try_from(size).expect("TOML table size fits in `isize`");
            table.set_position(Some(position));
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

    /// Ensure that an optional dependency group exists, creating an empty group if it doesn't.
    pub fn ensure_optional_dependency(&mut self, extra: &ExtraName) -> Result<(), Error> {
        // Get or create `project.optional-dependencies`.
        let optional_dependencies = self
            .project()?
            .entry("optional-dependencies")
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .ok_or(Error::MalformedDependencies)?;

        // Check if the extra already exists.
        let extra_exists = optional_dependencies
            .iter()
            .any(|(key, _value)| ExtraName::from_str(key).is_ok_and(|e| e == *extra));

        // If the extra doesn't exist, create it.
        if !extra_exists {
            optional_dependencies.insert(extra.as_ref(), Item::Value(Value::Array(Array::new())));
        }

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

        Ok(())
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

    /// Ensure that a dependency group exists, creating an empty group if it doesn't.
    pub fn ensure_dependency_group(&mut self, group: &GroupName) -> Result<(), Error> {
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

        // Check if the group already exists.
        let group_exists = dependency_groups
            .iter()
            .any(|(key, _value)| GroupName::from_str(key).is_ok_and(|g| g == *group));

        // If the group doesn't exist, create it.
        if !group_exists {
            dependency_groups.insert(group.as_ref(), Item::Value(Value::Array(Array::new())));

            // To avoid churn in pyproject.toml, we only sort new group keys if the
            // existing keys were sorted.
            if was_sorted {
                dependency_groups.sort_values();
            }
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

        Ok(())
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

    /// Get an existing TOML array for a dependency type.
    fn dependency_type_array_mut(
        &mut self,
        dependency_type: &DependencyType,
    ) -> Result<Option<&mut Array>, Error> {
        let dependencies = match dependency_type {
            DependencyType::Production => self
                .project_mut()?
                .and_then(|project| project.get_mut("dependencies"))
                .map(|dependencies| {
                    dependencies
                        .as_array_mut()
                        .ok_or(Error::MalformedDependencies)
                })
                .transpose()?,
            DependencyType::Dev => self
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
                .transpose()?,
            DependencyType::Optional(extra) => self
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
                        if ExtraName::from_str(key.get()).is_ok_and(|name| name == *extra) {
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
                .transpose()?,
            DependencyType::Group(group) => self
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
                        if GroupName::from_str(key.get()).is_ok_and(|name| name == *group) {
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
                .transpose()?,
        };

        Ok(dependencies)
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

        let value = if let Some(key) = find_source(name, sources) {
            let merged = merge_source(name, source, &sources[&key])?;
            if key != name.as_ref() {
                sources.remove(&key);
            }
            merged
        } else {
            Item::Value(source_value(source)?)
        };
        sources[name.as_ref()] = value;

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

    /// Remove sources whose dependencies are no longer declared in the project.
    fn remove_source(&mut self, name: &PackageName) -> Result<(), Error> {
        let Some(sources) = self
            .doc
            .get("tool")
            .map(|tool| tool.as_table().ok_or(Error::MalformedSources))
            .transpose()?
            .and_then(|tool| tool.get("uv"))
            .map(|uv| uv.as_table().ok_or(Error::MalformedSources))
            .transpose()?
            .and_then(|uv| uv.get("sources"))
            .map(|sources| sources.as_table().ok_or(Error::MalformedSources))
            .transpose()?
        else {
            return Ok(());
        };
        let Some(key) = find_source(name, sources) else {
            return Ok(());
        };
        let declarations = self.find_dependency(name, None);
        if !declarations.is_empty() {
            let source = &sources[&key];
            let count = source
                .as_array()
                .map(Array::len)
                .or_else(|| source.as_array_of_tables().map(ArrayOfTables::len));
            let mut groups = None;
            let mut unused = Vec::new();
            for index in 0..count.unwrap_or(1) {
                let entry = if count.is_some() {
                    &source[index]
                } else {
                    source
                };
                let used = if let Some(raw) = entry.get("extra").and_then(Item::as_str) {
                    let extra = ExtraName::from_str(raw).map_err(|_| Error::MalformedSources)?;
                    self.doc
                        .get("project")
                        .and_then(|project| project.get("dependencies"))
                        .and_then(Item::as_array)
                        .is_some_and(|deps| {
                            find_dependencies(name, None, deps).iter().any(|(_, req)| {
                                req.marker.top_level_extra_name().as_deref() == Some(&extra)
                            })
                        })
                        || declarations.contains(&DependencyType::Optional(extra))
                } else if let Some(raw) = entry.get("group").and_then(Item::as_str) {
                    let group = GroupName::from_str(raw).map_err(|_| Error::MalformedSources)?;
                    // A group can still depend on this package through an include-group.
                    if groups.is_none() {
                        let project: PyProjectToml = toml_edit::de::from_document(self.doc.clone())
                            .map_err(|_| Error::MalformedDependencies)?;
                        groups = Some(
                            FlatDependencyGroups::from_pyproject_toml(
                                Path::new("pyproject.toml"),
                                &project,
                            )
                            .map_err(|_| Error::MalformedDependencies)?,
                        );
                    }
                    groups
                        .as_ref()
                        .and_then(|groups| groups.get(&group))
                        .is_some_and(|group| group.requirements.iter().any(|req| req.name == *name))
                } else {
                    true
                };
                if !used {
                    unused.push(index);
                }
            }
            if unused.is_empty() {
                return Ok(());
            }
            if count.is_some_and(|count| unused.len() < count) {
                let source = &mut self.doc["tool"]["uv"]["sources"][&key];
                for index in unused.into_iter().rev() {
                    if let Some(array) = source.as_array_mut() {
                        remove_array_item(index, array);
                    } else if let Some(array) = source.as_array_of_tables_mut() {
                        array.remove(index);
                    }
                }
                return Ok(());
            }
        }

        let uv = self.doc["tool"]["uv"]
            .as_table_mut()
            .ok_or(Error::MalformedSources)?;
        let sources = uv["sources"]
            .as_table_mut()
            .ok_or(Error::MalformedSources)?;
        sources.remove(&key);
        if sources.is_empty() {
            uv.remove("sources");
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

        if let Some(project) = self.doc.get("project").and_then(Item::as_table_like) {
            // Check `project.dependencies`.
            if let Some(dependencies) = project.get("dependencies").and_then(Item::as_array)
                && !find_dependencies(name, marker, dependencies).is_empty()
            {
                types.push(DependencyType::Production);
            }

            // Check `project.optional-dependencies`.
            if let Some(extras) = project
                .get("optional-dependencies")
                .and_then(Item::as_table_like)
            {
                for (extra, dependencies) in extras.iter() {
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
        if let Some(groups) = self
            .doc
            .get("dependency-groups")
            .and_then(Item::as_table_like)
        {
            for (group, dependencies) in groups.iter() {
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
            .and_then(Item::as_table_like)
            .and_then(|tool| tool.get("uv"))
            .and_then(Item::as_table_like)
            .and_then(|uv| uv.get("dev-dependencies"))
            .and_then(Item::as_array)
            && !find_dependencies(name, marker, dev_dependencies).is_empty()
        {
            types.push(DependencyType::Dev);
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

        if let Some(existing) = project.get_mut("version") {
            if let Some(value) = existing.as_value_mut() {
                let mut formatted = Value::from(version.to_string());
                *formatted.decor_mut() = value.decor().clone();
                *value = formatted;
            } else {
                *existing = Item::Value(Value::from(version.to_string()));
            }
        } else {
            project.insert("version", Item::Value(Value::from(version.to_string())));
        }

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
fn add_dependency(
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
    // Remove in reverse to preserve indices. Before each removal, transfer the item's
    // prefix (which may contain end-of-line comments belonging to the previous line) to
    // the next item or array trailing so comments are not lost.
    //
    // For example, in:
    // ```toml
    // dependencies = [
    //     "numpy>=2.4.3", # essential comment
    //     "requests>=2.32.5",
    // ]
    // ```
    //
    // The comment `# essential comment` is stored by `toml_edit` in the prefix of
    // `requests`. When `requests` is removed, we transfer it so it remains on the
    // `numpy` line.
    let removed = find_dependencies(name, None, deps)
        .into_iter()
        .rev()
        .filter_map(|(i, _)| remove_dependency_at(i, deps))
        .collect::<Vec<_>>();

    if !removed.is_empty() {
        reformat_array_multiline(deps);
    }

    removed
}

fn remove_dependency_at(index: usize, deps: &mut Array) -> Option<Requirement> {
    remove_array_item(index, deps)
        .as_str()
        .and_then(|req| Requirement::from_str(req).ok())
}

/// Remove an array item, retaining comments that TOML stores between consecutive values.
fn remove_array_item(index: usize, array: &mut Array) -> Value {
    let removed = array.remove(index);
    let Some(prefix) = removed.decor().prefix().and_then(RawString::as_str) else {
        return removed;
    };
    let join = |following: Option<&str>| {
        let following = following.unwrap_or_default();
        let start = following.trim_start_matches([' ', '\t']);
        if !prefix.contains('#') && !start.starts_with('#') {
            return None;
        }
        // The next line already has its own indentation.
        let prefix = if start.starts_with(['\r', '\n']) {
            prefix.trim_end()
        } else {
            prefix
        };
        Some(format!("{prefix}{following}"))
    };
    if let Some(next) = array.get_mut(index) {
        let decor = next.decor_mut();
        if let Some(prefix) = join(decor.prefix().and_then(RawString::as_str)) {
            decor.set_prefix(prefix);
        }
    } else if let Some(trailing) = join(array.trailing().as_str()) {
        array.set_trailing(trailing);
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
        if let Some(req) = dep.as_str().and_then(try_parse_requirement)
            && marker.is_none_or(|m| *m == req.marker)
            && *name == req.name
        {
            to_replace.push((i, req));
        }
    }
    to_replace
}

/// Return whether two requirements have the same serialized fields, ignoring their parsed origin.
fn same_requirement_declaration(left: &Requirement, right: &Requirement) -> bool {
    left.name == right.name
        && left.extras == right.extras
        && left.version_or_url == right.version_or_url
        && left.marker == right.marker
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

/// Serialize a source as an inline table for `tool.uv.sources`.
fn source_value(source: &Source) -> Result<Value, Error> {
    let doc = toml_edit::ser::to_document(source).map_err(Box::new)?;
    Ok(Value::InlineTable(doc.into_table().into_inline_table()))
}

/// Update a single source while preserving selections for other markers and dependency groups.
fn merge_source(name: &PackageName, source: &Source, original: &Item) -> Result<Item, Error> {
    let value = original
        .clone()
        .into_value()
        .map_err(|_| Error::MalformedSources)?;
    let decor = value.decor().clone();
    let mut entries = match value {
        Value::Array(entries) => entries,
        mut value => {
            value.decor_mut().clear();
            Array::from_iter([value])
        }
    };

    let mut marker = source.marker();
    let mut replacement = None;
    let mut already_covered = false;
    for index in (0..entries.len()).rev() {
        let table = entries
            .get_mut(index)
            .and_then(Value::as_inline_table_mut)
            .ok_or(Error::MalformedSources)?;
        let existing: Source =
            toml_edit::de::from_document(DocumentMut::from(table.clone().into_table()))
                .map_err(|_| Error::MalformedSources)?;
        if source.marker() != existing.marker() && source.marker().is_disjoint(existing.marker()) {
            continue;
        }

        let same_origin = (source.extra(), source.group()) == (existing.extra(), existing.group());
        let existing_global = existing.extra().is_none() && existing.group().is_none();
        let incoming_global = source.extra().is_none() && source.group().is_none();
        if !same_origin && !existing_global && !incoming_global {
            continue;
        }
        let same_payload = source.clone().with_selectors(
            existing.marker(),
            existing.extra().cloned(),
            existing.group().cloned(),
        ) == existing;
        if same_payload
            && (same_origin || existing_global)
            && source.marker().is_disjoint(existing.marker().negate())
        {
            // The requested source already applies everywhere it is needed.
            already_covered = true;
            continue;
        }
        if !same_origin {
            // An unqualified source applies within named extras and groups as well.
            // Narrowing it here would also change those other declarations.
            if !same_payload || !incoming_global {
                return Err(Error::OverlappingSource(name.clone()));
            }
            let remaining = existing.marker().and(source.marker().negate());
            if !remaining.is_false()
                && let Some(remaining) = remaining.contents()
            {
                table.insert("marker", Value::from(remaining.to_string()));
                continue;
            }
        } else if same_payload {
            marker = marker.or(existing.marker());
        } else if source.marker() != existing.marker() {
            if !existing.marker().is_true() {
                return Err(Error::OverlappingSource(name.clone()));
            }
            // An existing unmarked source still applies outside the new marker.
            if let Some(remaining) = source.marker().negate().contents() {
                table.insert("marker", Value::from(remaining.to_string()));
            }
            continue;
        }
        replacement = Some((index, entries.remove(index).decor().clone()));
    }

    if already_covered {
        return Ok(original.clone());
    }

    let mut value = source_value(&source.clone().with_selectors(
        marker,
        source.extra().cloned(),
        source.group().cloned(),
    ))?;
    if let Some((index, decor)) = replacement {
        *value.decor_mut() = decor;
        entries.insert_formatted(index, value);
    } else {
        entries.push(value);
    }
    let mut value = if entries.len() == 1 {
        entries.remove(0)
    } else {
        reformat_array_multiline(&mut entries);
        Value::Array(entries)
    };
    *value.decor_mut() = decor;
    Ok(Item::Value(value))
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

                    if let Some((before, comment)) = line.split_once('#') {
                        let comment_text = format!("#{}", comment.trim_end());

                        let comment_kind = if (*prev_line_was_empty) || (*prev_line_was_comment) {
                            CommentType::OwnLine
                        } else {
                            CommentType::EndOfLine {
                                leading_whitespace: before
                                    .chars()
                                    .rev()
                                    .take_while(|c| c.is_whitespace())
                                    .collect::<String>()
                                    .chars()
                                    .rev()
                                    .collect(),
                            }
                        };

                        *prev_line_was_empty = trimmed_line.is_empty();
                        *prev_line_was_comment = true;

                        Some(Some(Comment {
                            text: comment_text,
                            kind: comment_kind,
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

    // Without a trailing comma, `toml_edit` stores comments after the final item in its
    // suffix. Once we add a trailing comma, those comments must follow the comma instead.
    if !deps.trailing_comma()
        && let Some(last) = deps.iter_mut().last()
        && let Some(suffix) = last.decor().suffix().and_then(RawString::as_str)
        && suffix.contains('#')
    {
        let suffix = suffix.to_string();
        last.decor_mut().set_suffix("");
        let trailing = deps.trailing().as_str().unwrap_or_default();
        deps.set_trailing(format!("{suffix}{trailing}"));
    }

    let mut indentation_prefix = None;

    // Calculate the indentation prefix based on the indentation of the first dependency entry.
    if let Some(first_item) = deps.iter().next() {
        let decor_prefix = first_item
            .decor()
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

    let indentation_prefix_str = format!("\n{}", indentation_prefix.as_deref().unwrap_or("    "));

    for item in deps.iter_mut() {
        let decor = item.decor_mut();
        let mut prefix = String::new();

        for comment in find_comments(decor.prefix()).chain(find_comments(decor.suffix())) {
            match &comment.kind {
                CommentType::OwnLine => {
                    prefix.push_str(&indentation_prefix_str);
                }
                CommentType::EndOfLine { leading_whitespace } => {
                    prefix.push_str(leading_whitespace);
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
                match &comment.kind {
                    CommentType::OwnLine => {
                        let indentation_prefix_str =
                            format!("\n{}", indentation_prefix.as_deref().unwrap_or("    "));
                        rv.push_str(&indentation_prefix_str);
                    }
                    CommentType::EndOfLine { leading_whitespace } => {
                        rv.push_str(leading_whitespace);
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
    use super::{
        ArrayEdit, DependencyTarget, PyProjectTomlMut, reformat_array_multiline, remove_dependency,
        split_specifiers,
    };
    use anyhow::Result;
    use insta::assert_snapshot;
    use std::path::Path;
    use std::str::FromStr;
    use toml_edit::DocumentMut;
    use uv_distribution_types::Index;
    use uv_normalize::{ExtraName, GroupName, PackageName};
    use uv_pep508::{Requirement, RequirementOrigin};
    use uv_workspace::pyproject::DependencyType;

    #[test]
    fn add_marked_source_preserves_default() -> Result<()> {
        let name = PackageName::from_str("demo")?;
        let mut doc = PyProjectTomlMut::from_toml(
            r#"[tool.uv.sources]
demo = { path = "original" }
"#,
            DependencyTarget::PyProjectToml,
        )?;
        let windows = toml::from_str(
            r#"path = "windows"
marker = "sys_platform == 'win32'""#,
        )?;
        doc.add_source(&name, &windows)?;

        let saved = doc.to_string();
        let expected = r#"
            [tool.uv.sources]
            demo = [
                { path = "original", marker = "sys_platform != 'win32'" },
                { path = "windows", marker = "sys_platform == 'win32'" },
            ]
        "#;
        assert_eq!(
            saved.parse::<toml::Table>()?,
            expected.parse::<toml::Table>()?
        );

        let overlapping = toml::from_str(
            r#"path = "arm"
marker = "platform_machine == 'aarch64'""#,
        )?;
        assert!(matches!(
            doc.add_source(&name, &overlapping),
            Err(super::Error::OverlappingSource(..))
        ));
        let unmarked = toml::from_str(r#"path = "replacement""#)?;
        assert!(matches!(
            doc.add_source(&name, &unmarked),
            Err(super::Error::OverlappingSource(..))
        ));
        assert_eq!(doc.to_string(), saved);
        Ok(())
    }

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

    /// Removing a scoped dependency must keep sources for groups that still include it indirectly.
    #[test]
    fn remove_source_preserves_included_group() -> Result<()> {
        let mut manifest = PyProjectTomlMut::from_toml(
            r#"
[dependency-groups]
docs = ["demo"]
lint = ["demo"]
ci = [{ include-group = "lint" }]

[tool.uv.sources]
demo = [
    { path = "wheel", group = "ci" }, # CI includes lint
    { path = "wheel", group = "docs" },
]
"#,
            DependencyTarget::PyProjectToml,
        )?;
        manifest.remove_dependency_group_requirement(&"demo".parse()?, &"docs".parse()?)?;
        assert_snapshot!(
            manifest.to_string(),
            @r#"
        [dependency-groups]
        docs = []
        lint = ["demo"]
        ci = [{ include-group = "lint" }]

        [tool.uv.sources]
        demo = [
            { path = "wheel", group = "ci" }, # CI includes lint
        ]
        "#,
        );
        Ok(())
    }

    #[test]
    fn reformat_preserves_inline_comment_spacing() {
        let mut doc: DocumentMut = r#"
[project]
dependencies = [
    "attrs>=25.4.0",     # comment
]
"#
        .parse()
        .unwrap();

        reformat_array_multiline(
            doc["project"]["dependencies"]
                .as_array_mut()
                .expect("dependencies array"),
        );

        let serialized = doc.to_string();

        assert!(
            serialized.contains("\"attrs>=25.4.0\",     # comment"),
            "inline comment spacing should be preserved:\n{serialized}"
        );
    }

    #[test]
    fn reformat_preserves_inline_comment_without_padding() {
        let mut doc: DocumentMut = r#"
[project]
dependencies = [
    "attrs>=25.4.0",#comment
]
"#
        .parse()
        .unwrap();

        reformat_array_multiline(
            doc["project"]["dependencies"]
                .as_array_mut()
                .expect("dependencies array"),
        );

        let serialized = doc.to_string();

        assert!(
            serialized.contains("\"attrs>=25.4.0\",#comment"),
            "inline comment spacing without padding should be preserved:\n{serialized}"
        );
    }

    #[test]
    fn replace_dependency_updates_every_exact_match() -> Result<()> {
        let mut pyproject = PyProjectTomlMut::from_toml(
            r#"[project]
dependencies = ["anyio<=2", "anyio>=1", "anyio<=2"]

[tool.uv.sources]
anyio = { index = "internal" }
            "#,
            DependencyTarget::PyProjectToml,
        )?;
        let existing = Requirement::from_str("anyio<=2")?.with_origin(RequirementOrigin::Workspace);
        let replacement = Requirement::from_str("anyio<3")?;

        let replaced = pyproject.replace_dependency_declaration(
            &DependencyType::Production,
            &existing,
            &replacement,
        )?;
        assert_eq!(replaced, vec![ArrayEdit::Update(0), ArrayEdit::Update(2)]);

        assert_snapshot!(
            pyproject.to_string(),
            @r#"
[project]
dependencies = ["anyio<3", "anyio>=1", "anyio<3"]

[tool.uv.sources]
anyio = { index = "internal" }
"#
        );
        Ok(())
    }

    #[test]
    fn replace_dependency_declaration_updates_selected_type() -> Result<()> {
        let mut pyproject = PyProjectTomlMut::from_toml(
            r#"[project]
dependencies = ["anyio<=2"]

[project.optional-dependencies]
test = ["anyio<=2"]

[dependency-groups]
dev = ["anyio<=2"]
            "#,
            DependencyTarget::PyProjectToml,
        )?;
        let existing = Requirement::from_str("anyio<=2")?;
        let optional_replacement = Requirement::from_str("anyio<3")?;
        let group_replacement = Requirement::from_str("anyio<4")?;

        let replaced = pyproject.replace_dependency_declaration(
            &DependencyType::Optional(ExtraName::from_str("test")?),
            &existing,
            &optional_replacement,
        )?;
        assert_eq!(replaced, vec![ArrayEdit::Update(0)]);

        let replaced = pyproject.replace_dependency_declaration(
            &DependencyType::Group(GroupName::from_str("dev")?),
            &existing,
            &group_replacement,
        )?;
        assert_eq!(replaced, vec![ArrayEdit::Update(0)]);

        assert_snapshot!(
            pyproject.to_string(),
            @r#"
[project]
dependencies = ["anyio<=2"]

[project.optional-dependencies]
test = ["anyio<3"]

[dependency-groups]
dev = ["anyio<4"]
"#
        );
        Ok(())
    }

    #[test]
    fn remove_preserves_end_of_line_comment_on_previous_item() {
        let toml = r#"
[project]
dependencies = [
    "numpy>=2.4.3", # this comment is clearly essential
    "requests>=2.32.5",
]
"#;
        let mut doc: DocumentMut = toml.parse().unwrap();
        let deps = doc["project"]["dependencies"]
            .as_array_mut()
            .expect("dependencies array");

        let name = PackageName::from_str("requests").unwrap();
        remove_dependency(&name, deps);

        assert_snapshot!(
            doc.to_string(),
            @r#"
[project]
dependencies = [
    "numpy>=2.4.3", # this comment is clearly essential
]
"#
        );
    }

    #[test]
    fn remove_preserves_end_of_line_comment_on_previous_item_middle() {
        let toml = r#"
[project]
dependencies = [
    "numpy>=2.4.3", # numpy comment
    "requests>=2.32.5",
    "flask>=3.0.0",
]
"#;
        let mut doc: DocumentMut = toml.parse().unwrap();
        let deps = doc["project"]["dependencies"]
            .as_array_mut()
            .expect("dependencies array");

        let name = PackageName::from_str("requests").unwrap();
        remove_dependency(&name, deps);

        assert_snapshot!(
            doc.to_string(),
            @r#"
[project]
dependencies = [
    "numpy>=2.4.3", # numpy comment
    "flask>=3.0.0",
]
"#
        );
    }

    #[test]
    fn remove_preserves_own_line_comment_above_removed_item() {
        let toml = r#"
[project]
dependencies = [
    "numpy>=2.4.3",
    # This is a comment about requests
    "requests>=2.32.5",
]
"#;
        let mut doc: DocumentMut = toml.parse().unwrap();
        let deps = doc["project"]["dependencies"]
            .as_array_mut()
            .expect("dependencies array");

        let name = PackageName::from_str("requests").unwrap();
        remove_dependency(&name, deps);

        assert_snapshot!(
            doc.to_string(),
            @r#"
[project]
dependencies = [
    "numpy>=2.4.3",
    # This is a comment about requests
]
"#
        );
    }

    #[test]
    fn remove_item_with_trailing_comment_last() {
        // When the removed item itself has an end-of-line comment and is the last item,
        // toml_edit stores the comment in the array trailing. The comment is preserved
        // (as an own-line comment in the trailing section) but moves position since it
        // can no longer be on the removed item's line.
        let toml = r#"
[project]
dependencies = [
    "requests>=2.32.5",
    "numpy>=2.4.3", # comment on numpy
]
"#;
        let mut doc: DocumentMut = toml.parse().unwrap();
        let deps = doc["project"]["dependencies"]
            .as_array_mut()
            .expect("dependencies array");

        let name = PackageName::from_str("numpy").unwrap();
        remove_dependency(&name, deps);

        assert_snapshot!(
            doc.to_string(),
            @r#"
[project]
dependencies = [
    "requests>=2.32.5",
    # comment on numpy
]
"#
        );
    }

    #[test]
    fn remove_last_item_with_trailing_comment_preserves_previous_comment() {
        let toml = r#"
[project]
dependencies = [
    "boto3", # this is boto3
    "requests", # this is requests
]
"#;
        let mut doc: DocumentMut = toml.parse().unwrap();
        let deps = doc["project"]["dependencies"]
            .as_array_mut()
            .expect("dependencies array");

        let name = PackageName::from_str("requests").unwrap();
        remove_dependency(&name, deps);

        assert_snapshot!(
            doc.to_string(),
            @r#"
[project]
dependencies = [
    "boto3", # this is boto3
    # this is requests
]
"#
        );
    }

    #[test]
    fn remove_item_with_trailing_comment_middle() {
        // When the removed item has an end-of-line comment and is in the middle,
        // toml_edit stores the comment in the next item's prefix. After removal,
        // reformat_array_multiline repositions it as an own-line comment.
        let toml = r#"
[project]
dependencies = [
    "requests>=2.32.5",
    "numpy>=2.4.3", # comment on numpy
    "flask>=3.0.0",
]
"#;
        let mut doc: DocumentMut = toml.parse().unwrap();
        let deps = doc["project"]["dependencies"]
            .as_array_mut()
            .expect("dependencies array");

        let name = PackageName::from_str("numpy").unwrap();
        remove_dependency(&name, deps);

        assert_snapshot!(
            doc.to_string(),
            @r#"
[project]
dependencies = [
    "requests>=2.32.5",
    # comment on numpy
    "flask>=3.0.0",
]
"#
        );
    }

    #[test]
    fn remove_first_item_with_trailing_comment_preserves_leading_comments() {
        let toml = r#"
[project]
dependencies = [
    # should be in alphabetical order
    "basedmypy[faster-cache]>=2.8.1", # this is a comment
    "basedpyright>=1.18.2,<2.0.0",
]
"#;
        let mut doc: DocumentMut = toml.parse().unwrap();
        let deps = doc["project"]["dependencies"]
            .as_array_mut()
            .expect("dependencies array");

        let name = PackageName::from_str("basedmypy").unwrap();
        remove_dependency(&name, deps);

        assert_snapshot!(
            doc.to_string(),
            @r#"
[project]
dependencies = [
    # should be in alphabetical order
    # this is a comment
    "basedpyright>=1.18.2,<2.0.0",
]
"#
        );
    }

    #[test]
    fn remove_multiple_adjacent_matches_preserves_comment_order() {
        let toml = r#"
[project]
dependencies = [
    "iniconfig>=2.0.0", # comment on iniconfig
    "typing-extensions>=4.0.0 ; python_version < '3.11'", # comment on first typing-extensions
    "typing-extensions>=4.0.0 ; python_version >= '3.11'",
    "sniffio>=1.3.0",
]
"#;
        let mut doc: DocumentMut = toml.parse().unwrap();
        let deps = doc["project"]["dependencies"]
            .as_array_mut()
            .expect("dependencies array");

        let name = PackageName::from_str("typing-extensions").unwrap();
        remove_dependency(&name, deps);

        assert_snapshot!(
            doc.to_string(),
            @r#"
[project]
dependencies = [
    "iniconfig>=2.0.0", # comment on iniconfig
    # comment on first typing-extensions
    "sniffio>=1.3.0",
]
"#
        );
    }

    #[test]
    fn add_index_syncs_format_on_url_update() {
        let toml = r#"
[[tool.uv.index]]
name = "index"
url = "https://example.com/flat/"
format = "flat"
"#;

        let mut doc = PyProjectTomlMut::from_toml(toml, DependencyTarget::PyProjectToml).unwrap();

        // The URL spelling changes, but the canonical URL does not, so format should be preserved.
        let equivalent_index = Index::from_str("index=https://example.com/flat").unwrap();
        doc.add_index(&equivalent_index, Path::new(".")).unwrap();

        assert_snapshot!(doc.to_string(), @r#"

[[tool.uv.index]]
name = "index"
url = "https://example.com/flat"
format = "flat"
"#);

        let new_index = Index::from_str("index=https://pypi.org/simple").unwrap();
        doc.add_index(&new_index, Path::new(".")).unwrap();

        assert_snapshot!(doc.to_string(), @r#"

[[tool.uv.index]]
name = "index"
url = "https://pypi.org/simple"
"#);
    }

    #[cfg(windows)]
    #[test]
    fn add_index_preserves_format_when_windows_path_unchanged() -> Result<()> {
        let toml = r#"
[[tool.uv.index]]
name = "index"
url = 'C:\links'
format = "flat"
"#;

        let mut doc = PyProjectTomlMut::from_toml(toml, DependencyTarget::PyProjectToml)?;

        let new_index = Index::from_str(r"index=C:\links")?;
        doc.add_index(&new_index, &std::env::current_dir()?)?;

        let index = doc.doc["tool"]["uv"]["index"]
            .as_array_of_tables()
            .and_then(|indexes| indexes.get(0))
            .expect("index table");
        assert_eq!(
            index.get("url").and_then(|item| item.as_str()),
            Some("C:/links")
        );
        assert_eq!(
            index.get("format").and_then(|item| item.as_str()),
            Some("flat")
        );

        Ok(())
    }
}

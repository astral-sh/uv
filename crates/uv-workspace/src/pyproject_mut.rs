use itertools::Itertools;
use pep440_rs::{Version, VersionSpecifier, VersionSpecifiers};
use pep508_rs::{ExtraName, MarkerTree, PackageName, Requirement, VersionOrUrl};
use std::path::Path;
use std::str::FromStr;
use std::{fmt, mem};
use thiserror::Error;
use toml_edit::{Array, DocumentMut, Item, RawString, Table, TomlError, Value};
use uv_fs::PortablePath;

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
    #[error("Cannot perform ambiguous update; found multiple entries with matching package names")]
    Ambiguous,
}

/// The result of editing an array in a TOML document.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ArrayEdit {
    /// An existing entry (at the given index) was updated.
    Update(usize),
    /// A new entry was added at the given index (typically, the end of the array).
    Add(usize),
}

impl ArrayEdit {
    pub fn index(&self) -> usize {
        match self {
            Self::Update(i) | Self::Add(i) => *i,
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

        Ok(())
    }

    /// Retrieves a mutable reference to the root [`Table`] of the TOML document, creating the
    /// `project` table if necessary.
    fn doc(&mut self) -> Result<&mut Table, Error> {
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
    fn doc_mut(&mut self) -> Result<Option<&mut Table>, Error> {
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
    ) -> Result<ArrayEdit, Error> {
        // Get or create `project.dependencies`.
        let dependencies = self
            .doc()?
            .entry("dependencies")
            .or_insert(Item::Value(Value::Array(Array::new())))
            .as_array_mut()
            .ok_or(Error::MalformedDependencies)?;

        let name = req.name.clone();
        let edit = add_dependency(req, dependencies, source.is_some())?;

        if let Some(source) = source {
            self.add_source(&name, source)?;
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

        let name = req.name.clone();
        let edit = add_dependency(req, dev_dependencies, source.is_some())?;

        if let Some(source) = source {
            self.add_source(&name, source)?;
        }

        Ok(edit)
    }

    /// Adds a dependency to `project.optional-dependencies`.
    ///
    /// Returns `true` if the dependency was added, `false` if it was updated.
    pub fn add_optional_dependency(
        &mut self,
        group: &ExtraName,
        req: &Requirement,
        source: Option<&Source>,
    ) -> Result<ArrayEdit, Error> {
        // Get or create `project.optional-dependencies`.
        let optional_dependencies = self
            .doc()?
            .entry("optional-dependencies")
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .ok_or(Error::MalformedDependencies)?;

        let group = optional_dependencies
            .entry(group.as_ref())
            .or_insert(Item::Value(Value::Array(Array::new())))
            .as_array_mut()
            .ok_or(Error::MalformedDependencies)?;

        let name = req.name.clone();
        let added = add_dependency(req, group, source.is_some())?;

        optional_dependencies.fmt();

        if let Some(source) = source {
            self.add_source(&name, source)?;
        }

        Ok(added)
    }

    /// Set the minimum version for an existing dependency in `project.dependencies`.
    pub fn set_dependency_minimum_version(
        &mut self,
        index: usize,
        version: Version,
    ) -> Result<(), Error> {
        // Get or create `project.dependencies`.
        let dependencies = self
            .doc()?
            .entry("dependencies")
            .or_insert(Item::Value(Value::Array(Array::new())))
            .as_array_mut()
            .ok_or(Error::MalformedDependencies)?;

        let Some(req) = dependencies.get(index) else {
            return Err(Error::MissingDependency(index));
        };

        let mut req = req
            .as_str()
            .and_then(try_parse_requirement)
            .ok_or(Error::MalformedDependencies)?;
        req.version_or_url = Some(VersionOrUrl::VersionSpecifier(VersionSpecifiers::from(
            VersionSpecifier::greater_than_equal_version(version),
        )));
        dependencies.replace(index, req.to_string());

        Ok(())
    }

    /// Set the minimum version for an existing dependency in `tool.uv.dev-dependencies`.
    pub fn set_dev_dependency_minimum_version(
        &mut self,
        index: usize,
        version: Version,
    ) -> Result<(), Error> {
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

        let Some(req) = dev_dependencies.get(index) else {
            return Err(Error::MissingDependency(index));
        };

        let mut req = req
            .as_str()
            .and_then(try_parse_requirement)
            .ok_or(Error::MalformedDependencies)?;
        req.version_or_url = Some(VersionOrUrl::VersionSpecifier(VersionSpecifiers::from(
            VersionSpecifier::greater_than_equal_version(version),
        )));
        dev_dependencies.replace(index, req.to_string());

        Ok(())
    }

    /// Set the minimum version for an existing dependency in `project.optional-dependencies`.
    pub fn set_optional_dependency_minimum_version(
        &mut self,
        group: &ExtraName,
        index: usize,
        version: Version,
    ) -> Result<(), Error> {
        // Get or create `project.optional-dependencies`.
        let optional_dependencies = self
            .doc()?
            .entry("optional-dependencies")
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .ok_or(Error::MalformedDependencies)?;

        let group = optional_dependencies
            .entry(group.as_ref())
            .or_insert(Item::Value(Value::Array(Array::new())))
            .as_array_mut()
            .ok_or(Error::MalformedDependencies)?;

        let Some(req) = group.get(index) else {
            return Err(Error::MissingDependency(index));
        };

        let mut req = req
            .as_str()
            .and_then(try_parse_requirement)
            .ok_or(Error::MalformedDependencies)?;
        req.version_or_url = Some(VersionOrUrl::VersionSpecifier(VersionSpecifiers::from(
            VersionSpecifier::greater_than_equal_version(version),
        )));
        group.replace(index, req.to_string());

        Ok(())
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

        add_source(name, source, sources)?;
        Ok(())
    }

    /// Removes all occurrences of dependencies with the given name.
    pub fn remove_dependency(&mut self, name: &PackageName) -> Result<Vec<Requirement>, Error> {
        // Try to get `project.dependencies`.
        let Some(dependencies) = self
            .doc_mut()?
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
            .doc_mut()?
            .and_then(|project| project.get_mut("optional-dependencies"))
            .map(|extras| {
                extras
                    .as_table_like_mut()
                    .ok_or(Error::MalformedDependencies)
            })
            .transpose()?
            .and_then(|extras| extras.get_mut(group.as_ref()))
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

    /// Remove a matching source from `tool.uv.sources`, if it exists.
    fn remove_source(&mut self, name: &PackageName) -> Result<(), Error> {
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
            sources.remove(name.as_ref());
        }

        Ok(())
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
                    let Ok(extra) = ExtraName::new(extra.to_string()) else {
                        continue;
                    };

                    if !find_dependencies(name, marker, dependencies).is_empty() {
                        types.push(DependencyType::Optional(extra));
                    }
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
            .and_then(|tool| tool.get("dev-dependencies"))
            .and_then(Item::as_array)
        {
            if !find_dependencies(name, marker, dev_dependencies).is_empty() {
                types.push(DependencyType::Dev);
            }
        }

        types
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
) -> Result<ArrayEdit, Error> {
    let mut to_replace = find_dependencies(&req.name, Some(&req.marker), deps);

    match to_replace.as_slice() {
        [] => {
            // Determine if the dependency list is sorted prior to
            // adding the new dependency; the new dependency list
            // will be sorted only when the original list is sorted
            // so that user's custom dependency ordering is preserved.
            // Additionally, if the table is invalid (i.e. contains non-string values)
            // we still treat it as unsorted for the sake of simplicity.
            let sorted = deps.iter().all(toml_edit::Value::is_str)
                && deps
                    .iter()
                    .tuple_windows()
                    .all(|(a, b)| a.as_str() <= b.as_str());

            let req_string = req.to_string();
            let index = if sorted {
                deps.iter()
                    .position(|d: &Value| d.as_str() > Some(req_string.as_str()))
                    .unwrap_or(deps.len())
            } else {
                deps.len()
            };

            deps.insert(index, req_string);
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

                deps.get_mut(index).unwrap().decor_mut().set_prefix(prefix);
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
        _ => Err(Error::Ambiguous),
    }
}

/// Update an existing requirement.
fn update_requirement(old: &mut Requirement, new: &Requirement, has_source: bool) {
    // Add any new extras.
    old.extras.extend(new.extras.iter().cloned());
    old.extras.sort_unstable();
    old.extras.dedup();

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
        old.marker = new.marker.clone();
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
            if marker.map_or(true, |m| *m == req.marker) && *name == req.name {
                to_replace.push((i, req));
            }
        }
    }
    to_replace
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
    fn find_comments(s: Option<&RawString>) -> impl Iterator<Item = &str> {
        s.and_then(|x| x.as_str())
            .unwrap_or("")
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                line.starts_with('#').then_some(line)
            })
    }

    let mut indentation_prefix = None;

    for item in deps.iter_mut() {
        let decor = item.decor_mut();
        let mut prefix = String::new();
        // calculating the indentation prefix as the indentation of the first dependency entry
        if indentation_prefix.is_none() {
            let decor_prefix = decor
                .prefix()
                .and_then(|s| s.as_str())
                .map(|s| s.split('#').next().unwrap_or("").to_string())
                .unwrap_or(String::new())
                .trim_start_matches('\n')
                .to_string();

            // if there is no indentation then apply a default one
            indentation_prefix = Some(if decor_prefix.is_empty() {
                "    ".to_string()
            } else {
                decor_prefix
            });
        }

        let indentation_prefix_str = format!("\n{}", indentation_prefix.as_ref().unwrap());

        for comment in find_comments(decor.prefix()).chain(find_comments(decor.suffix())) {
            prefix.push_str(&indentation_prefix_str);
            prefix.push_str(comment);
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
                rv.push_str("\n    ");
                rv.push_str(comment);
            }
        }
        if !rv.is_empty() || !deps.is_empty() {
            rv.push('\n');
        }
        rv
    });
    deps.set_trailing_comma(true);
}

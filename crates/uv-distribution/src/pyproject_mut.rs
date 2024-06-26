use std::str::FromStr;
use std::{fmt, mem};

use thiserror::Error;
use toml_edit::{Array, DocumentMut, Item, RawString, Table, TomlError, Value};

use pep508_rs::{PackageName, Requirement, VersionOrUrl};

use crate::pyproject::{PyProjectToml, Source};

/// Raw and mutable representation of a `pyproject.toml`.
///
/// This is useful for operations that require editing an existing `pyproject.toml` while
/// preserving comments and other structure, such as `uv add` and `uv remove`.
pub struct PyProjectTomlMut {
    doc: DocumentMut,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to parse `pyproject.toml`")]
    Parse(#[from] Box<TomlError>),
    #[error("Failed to serialize `pyproject.toml`")]
    Serialize(#[from] Box<toml::ser::Error>),
    #[error("Dependencies in `pyproject.toml` are malformed")]
    MalformedDependencies,
    #[error("Sources in `pyproject.toml` are malformed")]
    MalformedSources,
    #[error("Cannot perform ambiguous update; multiple entries with matching package names.")]
    Ambiguous,
}

impl PyProjectTomlMut {
    /// Initialize a `PyProjectTomlMut` from a `PyProjectToml`.
    pub fn from_toml(pyproject: &PyProjectToml) -> Result<Self, Error> {
        Ok(Self {
            doc: pyproject.raw.parse().map_err(Box::new)?,
        })
    }

    /// Adds a dependency to `project.dependencies`.
    pub fn add_dependency(
        &mut self,
        req: Requirement,
        source: Option<Source>,
    ) -> Result<(), Error> {
        // Get or create `project.dependencies`.
        let dependencies = self
            .doc
            .entry("project")
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or(Error::MalformedDependencies)?
            .entry("dependencies")
            .or_insert(Item::Value(Value::Array(Array::new())))
            .as_array_mut()
            .ok_or(Error::MalformedDependencies)?;

        let name = req.name.clone();
        add_dependency(req, dependencies, source.is_some())?;

        if let Some(source) = source {
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

            add_source(&name, &source, sources)?;
        }

        Ok(())
    }

    /// Adds a development dependency to `tool.uv.dev-dependencies`.
    pub fn add_dev_dependency(
        &mut self,
        req: Requirement,
        source: Option<Source>,
    ) -> Result<(), Error> {
        // Get or create `tool.uv`.
        let tool_uv = self
            .doc
            .entry("tool")
            .or_insert(implicit())
            .as_table_mut()
            .ok_or(Error::MalformedSources)?
            .entry("uv")
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or(Error::MalformedSources)?;

        // Get or create the `tool.uv.dev-dependencies` array.
        let dev_dependencies = tool_uv
            .entry("dev-dependencies")
            .or_insert(Item::Value(Value::Array(Array::new())))
            .as_array_mut()
            .ok_or(Error::MalformedDependencies)?;

        let name = req.name.clone();
        add_dependency(req, dev_dependencies, source.is_some())?;

        if let Some(source) = source {
            // Get or create `tool.uv.sources`.
            let sources = tool_uv
                .entry("sources")
                .or_insert(Item::Table(Table::new()))
                .as_table_mut()
                .ok_or(Error::MalformedSources)?;

            add_source(&name, &source, sources)?;
        }

        Ok(())
    }

    /// Removes all occurrences of dependencies with the given name.
    pub fn remove_dependency(&mut self, req: &PackageName) -> Result<Vec<Requirement>, Error> {
        // Try to get `project.dependencies`.
        let Some(dependencies) = self
            .doc
            .get_mut("project")
            .and_then(Item::as_table_mut)
            .and_then(|project| project.get_mut("dependencies"))
        else {
            return Ok(Vec::new());
        };
        let dependencies = dependencies
            .as_array_mut()
            .ok_or(Error::MalformedDependencies)?;

        let requirements = remove_dependency(req, dependencies);

        // Remove a matching source from `tool.uv.sources`, if it exists.
        if let Some(sources) = self
            .doc
            .get_mut("tool")
            .and_then(Item::as_table_mut)
            .and_then(|tool| tool.get_mut("uv"))
            .and_then(Item::as_table_mut)
            .and_then(|tool_uv| tool_uv.get_mut("sources"))
        {
            let sources = sources.as_table_mut().ok_or(Error::MalformedSources)?;
            sources.remove(req.as_ref());
        }

        Ok(requirements)
    }

    /// Removes all occurrences of development dependencies with the given name.
    pub fn remove_dev_dependency(&mut self, req: &PackageName) -> Result<Vec<Requirement>, Error> {
        let Some(tool_uv) = self
            .doc
            .get_mut("tool")
            .and_then(Item::as_table_mut)
            .and_then(|tool| tool.get_mut("uv"))
            .and_then(Item::as_table_mut)
        else {
            return Ok(Vec::new());
        };

        // Try to get `tool.uv.dev-dependencies`.
        let Some(dev_dependencies) = tool_uv.get_mut("dev-dependencies") else {
            return Ok(Vec::new());
        };
        let dev_dependencies = dev_dependencies
            .as_array_mut()
            .ok_or(Error::MalformedDependencies)?;

        let requirements = remove_dependency(req, dev_dependencies);

        // Remove a matching source from `tool.uv.sources`, if it exists.
        if let Some(sources) = tool_uv.get_mut("sources") {
            let sources = sources.as_table_mut().ok_or(Error::MalformedSources)?;
            sources.remove(req.as_ref());
        };

        Ok(requirements)
    }
}

/// Returns an implicit table.
fn implicit() -> Item {
    let mut table = Table::new();
    table.set_implicit(true);
    Item::Table(table)
}

/// Adds a dependency to the given `deps` array.
pub fn add_dependency(req: Requirement, deps: &mut Array, has_source: bool) -> Result<(), Error> {
    // Find matching dependencies.
    let mut to_replace = find_dependencies(&req.name, deps);
    match to_replace.as_slice() {
        [] => deps.push(req.to_string()),
        [_] => {
            let (i, mut old_req) = to_replace.remove(0);
            update_requirement(&mut old_req, req, has_source);
            deps.replace(i, old_req.to_string());
        }
        // Cannot perform ambiguous updates.
        _ => return Err(Error::Ambiguous),
    }
    reformat_array_multiline(deps);
    Ok(())
}

/// Update an existing requirement.
fn update_requirement(old: &mut Requirement, new: Requirement, has_source: bool) {
    // Add any new extras.
    old.extras.extend(new.extras);
    old.extras.sort_unstable();
    old.extras.dedup();

    // Clear the requirement source if we are going to add to `tool.uv.sources`.
    if has_source {
        old.version_or_url = None;
    }

    // Update the source if a new one was specified.
    match new.version_or_url {
        None => {}
        Some(VersionOrUrl::VersionSpecifier(specifier)) if specifier.is_empty() => {}
        Some(version_or_url) => old.version_or_url = Some(version_or_url),
    }

    // Update the marker expression.
    if let Some(marker) = new.marker {
        old.marker = Some(marker);
    }
}

/// Removes all occurrences of dependencies with the given name from the given `deps` array.
fn remove_dependency(req: &PackageName, deps: &mut Array) -> Vec<Requirement> {
    // Remove matching dependencies.
    let removed = find_dependencies(req, deps)
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

// Returns a `Vec` containing the all dependencies with the given name, along with their positions
// in the array.
fn find_dependencies(name: &PackageName, deps: &Array) -> Vec<(usize, Requirement)> {
    let mut to_replace = Vec::new();
    for (i, dep) in deps.iter().enumerate() {
        if let Some(req) = dep.as_str().and_then(try_parse_requirement) {
            if req.name == *name {
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

    for item in deps.iter_mut() {
        let decor = item.decor_mut();
        let mut prefix = String::new();
        for comment in find_comments(decor.prefix()).chain(find_comments(decor.suffix())) {
            prefix.push_str("\n    ");
            prefix.push_str(comment);
        }
        prefix.push_str("\n    ");
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

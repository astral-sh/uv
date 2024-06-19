use std::str::FromStr;
use std::{fmt, mem};

use thiserror::Error;
use toml_edit::{Array, DocumentMut, Item, RawString, Table, TomlError, Value};

use pep508_rs::{PackageName, Requirement};
use pypi_types::VerbatimParsedUrl;

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
        req: &Requirement,
        source: Option<&Source>,
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

        add_dependency(req, dependencies);

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

            add_source(req, source, sources)?;
        }

        Ok(())
    }

    /// Adds a development dependency to `tool.uv.dev-dependencies`.
    pub fn add_dev_dependency(
        &mut self,
        req: &Requirement,
        source: Option<&Source>,
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

        add_dependency(req, dev_dependencies);

        if let Some(source) = source {
            // Get or create `tool.uv.sources`.
            let sources = tool_uv
                .entry("sources")
                .or_insert(Item::Table(Table::new()))
                .as_table_mut()
                .ok_or(Error::MalformedSources)?;

            add_source(req, source, sources)?;
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
pub fn add_dependency(req: &Requirement, deps: &mut Array) {
    // Find matching dependencies.
    let to_replace = find_dependencies(&req.name, deps);
    if to_replace.is_empty() {
        deps.push(req.to_string());
    } else {
        // Replace the first occurrence of the dependency and remove the rest.
        deps.replace(to_replace[0], req.to_string());
        for &i in to_replace[1..].iter().rev() {
            deps.remove(i);
        }
    }
    reformat_array_multiline(deps);
}

/// Removes all occurrences of dependencies with the given name from the given `deps` array.
fn remove_dependency(req: &PackageName, deps: &mut Array) -> Vec<Requirement> {
    // Remove matching dependencies.
    let removed = find_dependencies(req, deps)
        .into_iter()
        .rev() // Reverse to preserve indices as we remove them.
        .filter_map(|i| {
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

// Returns a `Vec` containing the indices of all dependencies with the given name.
fn find_dependencies(name: &PackageName, deps: &Array) -> Vec<usize> {
    let mut to_replace = Vec::new();
    for (i, dep) in deps.iter().enumerate() {
        if dep
            .as_str()
            .and_then(try_parse_requirement)
            .filter(|dep| dep.name == *name)
            .is_some()
        {
            to_replace.push(i);
        }
    }
    to_replace
}

// Add a source to `tool.uv.sources`.
fn add_source(req: &Requirement, source: &Source, sources: &mut Table) -> Result<(), Error> {
    // Serialize as an inline table.
    let mut doc = toml::to_string(source)
        .map_err(Box::new)?
        .parse::<DocumentMut>()
        .unwrap();
    let table = mem::take(doc.as_table_mut()).into_inline_table();

    sources.insert(req.name.as_ref(), Item::Value(Value::InlineTable(table)));

    Ok(())
}

impl fmt::Display for PyProjectTomlMut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.doc.fmt(f)
    }
}

fn try_parse_requirement(req: &str) -> Option<Requirement<VerbatimParsedUrl>> {
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

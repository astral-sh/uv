use std::fmt;
use std::str::FromStr;

use thiserror::Error;
use toml_edit::{Array, DocumentMut, Item, RawString, TomlError, Value};

use pep508_rs::Requirement;
use pypi_types::{LenientRequirement, VerbatimParsedUrl};

use crate::pyproject::PyProjectToml;

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
    Parse(#[from] TomlError),
    #[error("Dependencies in pyproject.toml are malformed")]
    MalformedDependencies,
}

impl PyProjectTomlMut {
    /// Initialize a `PyProjectTomlMut` from a `PyProjectToml`.
    pub fn from_toml(pyproject: &PyProjectToml) -> Result<Self, Error> {
        Ok(Self {
            doc: pyproject.raw.parse()?,
        })
    }

    /// Adds a dependency.
    pub fn add_dependency(&mut self, req: &Requirement) -> Result<(), Error> {
        let deps = &mut self.doc["project"]["dependencies"];
        if deps.is_none() {
            *deps = Item::Value(Value::Array(Array::new()));
        }
        let deps = deps.as_array_mut().ok_or(Error::MalformedDependencies)?;

        // Try to find a matching dependency.
        let mut to_replace = None;
        for (i, dep) in deps.iter().enumerate() {
            if let Some(dep) = dep.as_str().and_then(try_parse_requirement) {
                if dep.name.as_ref().eq_ignore_ascii_case(req.name.as_ref()) {
                    to_replace = Some(i);
                    break;
                }
            }
        }

        match to_replace {
            Some(i) => {
                deps.replace(i, req.to_string());
            }
            None => deps.push(req.to_string()),
        }

        reformat_array_multiline(deps);

        Ok(())
    }

    /// Removes all occurrences of dependencies with the given name.
    pub fn remove_dependency(&mut self, req: &String) -> Result<Vec<Requirement>, Error> {
        let deps = &mut self.doc["project"]["dependencies"];
        if deps.is_none() {
            return Ok(Vec::new());
        }

        let deps = deps.as_array_mut().ok_or(Error::MalformedDependencies)?;

        // Try to find matching dependencies.
        let mut to_remove = Vec::new();
        for (i, dep) in deps.iter().enumerate() {
            if let Some(dep) = dep.as_str().and_then(try_parse_requirement) {
                if dep.name.as_ref().eq_ignore_ascii_case(req.as_ref()) {
                    to_remove.push(i);
                }
            }
        }

        let removed = to_remove
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

        Ok(removed)
    }
}

impl fmt::Display for PyProjectTomlMut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.doc.fmt(f)
    }
}

fn try_parse_requirement(req: &str) -> Option<Requirement<VerbatimParsedUrl>> {
    LenientRequirement::from_str(req)
        .map(Requirement::from)
        .ok()
}

/// Reformats a TOML array to multi line while trying to preserve all comments
/// and move them around. This also formats the array to have a trailing comma.
pub fn reformat_array_multiline(deps: &mut Array) {
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

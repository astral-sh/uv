//! Parses a subset of requirement.txt syntax
//!
//! <https://pip.pypa.io/en/stable/reference/requirements-file-format/>
//!
//! Supported:
//!  * [PEP 508 requirements](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
//!  * `-r`
//!  * `-c`
//!  * `--hash` (postfix)
//!  * `-e`
//!
//! Unsupported:
//!  * `<path>`. TBD
//!  * `<archive_url>`. TBD
//!  * Options without a requirement, such as `--find-links` or `--index-url`
//!
//! Grammar as implemented:
//!
//! ```text
//! file = (statement | empty ('#' any*)? '\n')*
//! empty = whitespace*
//! statement = constraint_include | requirements_include | editable_requirement | requirement
//! constraint_include = '-c' ('=' | wrappable_whitespaces) filepath
//! requirements_include = '-r' ('=' | wrappable_whitespaces) filepath
//! editable_requirement = '-e' ('=' | wrappable_whitespaces) requirement
//! # We check whether the line starts with a letter or a number, in that case we assume it's a
//! # PEP 508 requirement
//! # https://packaging.python.org/en/latest/specifications/name-normalization/#valid-non-normalized-names
//! # This does not (yet?) support plain files or urls, we use a letter or a number as first
//! # character to assume a PEP 508 requirement
//! requirement = [a-zA-Z0-9] pep508_grammar_tail wrappable_whitespaces hashes
//! hashes = ('--hash' ('=' | wrappable_whitespaces) [a-zA-Z0-9-_]+ ':' [a-zA-Z0-9-_] wrappable_whitespaces+)*
//! # This should indicate a single backslash before a newline
//! wrappable_whitespaces = whitespace ('\\\n' | whitespace)*
//! ```

use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};

use fs_err as fs;
use serde::{Deserialize, Serialize};
use tracing::warn;
use unscanny::{Pattern, Scanner};
use url::Url;

use pep508_rs::{split_scheme, Extras, Pep508Error, Pep508ErrorSource, Requirement, VerbatimUrl};
use puffin_fs::NormalizedDisplay;
use puffin_normalize::ExtraName;

/// We emit one of those for each requirements.txt entry
enum RequirementsTxtStatement {
    /// `-r` inclusion filename
    Requirements {
        filename: String,
        start: usize,
        end: usize,
    },
    /// `-c` inclusion filename
    Constraint {
        filename: String,
        start: usize,
        end: usize,
    },
    /// PEP 508 requirement plus metadata
    RequirementEntry(RequirementEntry),
    /// `-e`
    EditableRequirement(EditableRequirement),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EditableRequirement {
    pub url: VerbatimUrl,
    pub extras: Vec<ExtraName>,
    pub path: PathBuf,
}

impl EditableRequirement {
    pub fn url(&self) -> &VerbatimUrl {
        &self.url
    }

    pub fn raw(&self) -> &Url {
        self.url.raw()
    }
}

impl EditableRequirement {
    /// Parse a raw string for an editable requirement (`pip install -e <editable>`), which could be
    /// a URL or a local path, and could contain unexpanded environment variables.
    ///
    /// For example:
    /// - `file:///home/ferris/project/scripts/...`
    /// - `file:../editable/`
    /// - `../editable/`
    ///
    /// We disallow URLs with schemes other than `file://` (e.g., `https://...`).
    pub fn parse(
        given: &str,
        working_dir: impl AsRef<Path>,
    ) -> Result<EditableRequirement, RequirementsTxtParserError> {
        // Identify the extras.
        let (requirement, extras) = if let Some((requirement, extras)) = Self::split_extras(given) {
            let extras = Extras::parse(extras).map_err(|err| {
                // Map from error on the extras to error on the whole requirement.
                let err = Pep508Error {
                    message: err.message,
                    start: requirement.len() + err.start,
                    len: err.len,
                    input: given.to_string(),
                };
                match err.message {
                    Pep508ErrorSource::String(_) => RequirementsTxtParserError::Pep508 {
                        start: err.start,
                        end: err.start + err.len,
                        source: err,
                    },
                    Pep508ErrorSource::UrlError(_) => RequirementsTxtParserError::Pep508 {
                        start: err.start,
                        end: err.start + err.len,
                        source: err,
                    },
                    Pep508ErrorSource::UnsupportedRequirement(_) => {
                        RequirementsTxtParserError::UnsupportedRequirement {
                            start: err.start,
                            end: err.start + err.len,
                            source: err,
                        }
                    }
                }
            })?;
            (requirement, extras.into_vec())
        } else {
            (given, vec![])
        };

        // Create a `VerbatimUrl` to represent the editable requirement.
        let url = if let Some((scheme, path)) = split_scheme(requirement) {
            if scheme == "file" {
                if let Some(path) = path.strip_prefix("//") {
                    // Ex) `file:///home/ferris/project/scripts/...`
                    VerbatimUrl::from_path(path, working_dir.as_ref())
                } else {
                    // Ex) `file:../editable/`
                    VerbatimUrl::from_path(path, working_dir.as_ref())
                }
            } else {
                // Ex) `https://...`
                return Err(RequirementsTxtParserError::UnsupportedUrl(
                    requirement.to_string(),
                ));
            }
        } else {
            // Ex) `../editable/`
            VerbatimUrl::from_path(requirement, working_dir.as_ref())
        };

        // Create a `PathBuf`.
        let path = url.to_file_path().map_err(|()| {
            RequirementsTxtParserError::InvalidEditablePath(requirement.to_string())
        })?;

        // Add the verbatim representation of the URL to the `VerbatimUrl`.
        let url = url.with_given(requirement.to_string());

        Ok(EditableRequirement { url, extras, path })
    }

    /// Identify the extras in an editable URL (e.g., `../editable[dev]`).
    ///
    /// Pip uses `m = re.match(r'^(.+)(\[[^]]+])$', path)`. Our strategy is:
    /// - If the string ends with a closing bracket (`]`)...
    /// - Iterate backwards until you find the open bracket (`[`)...
    /// - But abort if you find another closing bracket (`]`) first.
    pub fn split_extras(given: &str) -> Option<(&str, &str)> {
        let mut chars = given.char_indices().rev();

        // If the string ends with a closing bracket (`]`)...
        if !matches!(chars.next(), Some((_, ']'))) {
            return None;
        }

        // Iterate backwards until you find the open bracket (`[`)...
        let (index, _) = chars
            .take_while(|(_, c)| *c != ']')
            .find(|(_, c)| *c == '[')?;

        Some(given.split_at(index))
    }
}

impl Display for EditableRequirement {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.url, f)
    }
}

/// A [Requirement] with additional metadata from the requirements.txt, currently only hashes but in
/// the future also editable an similar information
#[derive(Debug, Deserialize, Clone, Eq, PartialEq, Serialize)]
pub struct RequirementEntry {
    /// The actual PEP 508 requirement
    pub requirement: Requirement,
    /// Hashes of the downloadable packages
    pub hashes: Vec<String>,
    /// Editable installation, see e.g. <https://stackoverflow.com/q/35064426/3549270>
    pub editable: bool,
}

impl Display for RequirementEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.editable {
            write!(f, "-e ")?;
        }
        write!(f, "{}", self.requirement)?;
        for hash in &self.hashes {
            write!(f, " --hash {hash}")?;
        }
        Ok(())
    }
}

/// Parsed and flattened requirements.txt with requirements and constraints
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct RequirementsTxt {
    /// The actual requirements with the hashes
    pub requirements: Vec<RequirementEntry>,
    /// Constraints included with `-c`
    pub constraints: Vec<Requirement>,
    /// Editables with `-e`
    pub editables: Vec<EditableRequirement>,
}

impl RequirementsTxt {
    /// See module level documentation
    pub fn parse(
        requirements_txt: impl AsRef<Path>,
        working_dir: impl AsRef<Path>,
    ) -> Result<Self, RequirementsTxtFileError> {
        let content =
            fs::read_to_string(&requirements_txt).map_err(|err| RequirementsTxtFileError {
                file: requirements_txt.as_ref().to_path_buf(),
                error: RequirementsTxtParserError::IO(err),
            })?;
        let data = Self::parse_inner(&content, working_dir.as_ref()).map_err(|err| {
            RequirementsTxtFileError {
                file: requirements_txt.as_ref().to_path_buf(),
                error: err,
            }
        })?;
        if data == Self::default() {
            warn!(
                "Requirements file {} does not contain any dependencies",
                requirements_txt.as_ref().display()
            );
        }
        Ok(data)
    }

    /// See module level documentation
    ///
    /// Note that all relative paths are dependent on the current working dir, not on the location
    /// of the file
    pub fn parse_inner(
        content: &str,
        working_dir: &Path,
    ) -> Result<Self, RequirementsTxtParserError> {
        let mut s = Scanner::new(content);

        let mut data = Self::default();
        while let Some(statement) = parse_entry(&mut s, content, working_dir)? {
            match statement {
                RequirementsTxtStatement::Requirements {
                    filename,
                    start,
                    end,
                } => {
                    let sub_file = working_dir.join(filename);
                    let sub_requirements = Self::parse(&sub_file, working_dir).map_err(|err| {
                        RequirementsTxtParserError::Subfile {
                            source: Box::new(err),
                            start,
                            end,
                        }
                    })?;
                    // Add each to the correct category
                    data.update_from(sub_requirements);
                }
                RequirementsTxtStatement::Constraint {
                    filename,
                    start,
                    end,
                } => {
                    let sub_file = working_dir.join(filename);
                    let sub_constraints = Self::parse(&sub_file, working_dir).map_err(|err| {
                        RequirementsTxtParserError::Subfile {
                            source: Box::new(err),
                            start,
                            end,
                        }
                    })?;
                    // Treat any nested requirements or constraints as constraints. This differs
                    // from `pip`, which seems to treat `-r` requirements in constraints files as
                    // _requirements_, but we don't want to support that.
                    data.constraints.extend(
                        sub_constraints
                            .requirements
                            .into_iter()
                            .map(|requirement_entry| requirement_entry.requirement),
                    );
                    data.constraints.extend(sub_constraints.constraints);
                }
                RequirementsTxtStatement::RequirementEntry(requirement_entry) => {
                    data.requirements.push(requirement_entry);
                }
                RequirementsTxtStatement::EditableRequirement(editable) => {
                    data.editables.push(editable);
                }
            }
        }
        Ok(data)
    }

    /// Merges other into self
    pub fn update_from(&mut self, other: RequirementsTxt) {
        self.requirements.extend(other.requirements);
        self.constraints.extend(other.constraints);
    }
}

/// Parse a single entry, that is a requirement, an inclusion or a comment line
///
/// Consumes all preceding trivia (whitespace and comments). If it returns None, we've reached
/// the end of file
fn parse_entry(
    s: &mut Scanner,
    content: &str,
    working_dir: &Path,
) -> Result<Option<RequirementsTxtStatement>, RequirementsTxtParserError> {
    // Eat all preceding whitespace, this may run us to the end of file
    eat_wrappable_whitespace(s);
    while s.at(['\n', '\r', '#']) {
        // skip comments
        eat_trailing_line(s)?;
        eat_wrappable_whitespace(s);
    }

    let start = s.cursor();
    Ok(Some(if s.eat_if("-r") {
        let requirements_file = parse_value(s, |c: char| !['\n', '\r', '#'].contains(&c))?;
        let end = s.cursor();
        eat_trailing_line(s)?;
        RequirementsTxtStatement::Requirements {
            filename: requirements_file.to_string(),
            start,
            end,
        }
    } else if s.eat_if("-c") {
        let constraints_file = parse_value(s, |c: char| !['\n', '\r', '#'].contains(&c))?;
        let end = s.cursor();
        eat_trailing_line(s)?;
        RequirementsTxtStatement::Constraint {
            filename: constraints_file.to_string(),
            start,
            end,
        }
    } else if s.eat_if("-e") {
        let path_or_url = parse_value(s, |c: char| !['\n', '\r'].contains(&c))?;
        let editable_requirement = EditableRequirement::parse(path_or_url, working_dir)
            .map_err(|err| err.with_offset(start))?;
        RequirementsTxtStatement::EditableRequirement(editable_requirement)
    } else if s.at(char::is_ascii_alphanumeric) {
        let (requirement, hashes) = parse_requirement_and_hashes(s, content, working_dir)?;
        RequirementsTxtStatement::RequirementEntry(RequirementEntry {
            requirement,
            hashes,
            editable: false,
        })
    } else if let Some(char) = s.peek() {
        return Err(RequirementsTxtParserError::Parser {
            message: format!(
                "Unexpected '{char}', expected '-c', '-e', '-r' or the start of a requirement"
            ),
            location: s.cursor(),
        });
    } else {
        // EOF
        return Ok(None);
    }))
}

/// Eat whitespace and ignore newlines escaped with a backslash
fn eat_wrappable_whitespace<'a>(s: &mut Scanner<'a>) -> &'a str {
    let start = s.cursor();
    s.eat_while([' ', '\t']);
    // Allow multiple escaped line breaks
    // With the order we support `\n`, `\r`, `\r\n` without accidentally eating a `\n\r`
    while s.eat_if("\\\n") || s.eat_if("\\\r\n") || s.eat_if("\\\r") {
        s.eat_while([' ', '\t']);
    }
    s.from(start)
}

/// Eats the end of line or a potential trailing comma
fn eat_trailing_line(s: &mut Scanner) -> Result<(), RequirementsTxtParserError> {
    s.eat_while([' ', '\t']);
    match s.eat() {
        None | Some('\n') => {} // End of file or end of line, nothing to do
        Some('\r') => {
            s.eat_if('\n'); // `\r\n`, but just `\r` is also accepted
        }
        Some('#') => {
            s.eat_until(['\r', '\n']);
            if s.at('\r') {
                s.eat_if('\n'); // `\r\n`, but just `\r` is also accepted
            }
        }
        Some(other) => {
            return Err(RequirementsTxtParserError::Parser {
                message: format!("Expected comment or end-of-line, found '{other}'"),
                location: s.cursor(),
            });
        }
    }
    Ok(())
}

/// Parse a PEP 508 requirement with optional trailing hashes
fn parse_requirement_and_hashes(
    s: &mut Scanner,
    content: &str,
    working_dir: &Path,
) -> Result<(Requirement, Vec<String>), RequirementsTxtParserError> {
    // PEP 508 requirement
    let start = s.cursor();
    // Termination: s.eat() eventually becomes None
    let (end, has_hashes) = loop {
        let end = s.cursor();

        //  We look for the end of the line ...
        if s.eat_if('\n') {
            break (end, false);
        }
        if s.eat_if('\r') {
            s.eat_if('\n'); // Support `\r\n` but also accept stray `\r`
            break (end, false);
        }
        // ... or `--hash`, an escaped newline or a comment separated by whitespace ...
        if !eat_wrappable_whitespace(s).is_empty() {
            if s.after().starts_with("--") {
                break (end, true);
            } else if s.eat_if('#') {
                s.eat_until(['\r', '\n']);
                if s.at('\r') {
                    s.eat_if('\n'); // `\r\n`, but just `\r` is also accepted
                }
                break (end, false);
            }
            continue;
        }
        // ... or the end of the file, which works like the end of line
        if s.eat().is_none() {
            break (end, false);
        }
    };
    let requirement =
        Requirement::parse(&content[start..end], Some(working_dir)).map_err(|err| {
            match err.message {
                Pep508ErrorSource::String(_) => RequirementsTxtParserError::Pep508 {
                    source: err,
                    start,
                    end,
                },
                Pep508ErrorSource::UrlError(_) => RequirementsTxtParserError::Pep508 {
                    source: err,
                    start,
                    end,
                },
                Pep508ErrorSource::UnsupportedRequirement(_) => {
                    RequirementsTxtParserError::UnsupportedRequirement {
                        source: err,
                        start,
                        end,
                    }
                }
            }
        })?;
    let hashes = if has_hashes {
        let hashes = parse_hashes(s)?;
        eat_trailing_line(s)?;
        hashes
    } else {
        Vec::new()
    };
    Ok((requirement, hashes))
}

/// Parse `--hash=... --hash ...` after a requirement
fn parse_hashes(s: &mut Scanner) -> Result<Vec<String>, RequirementsTxtParserError> {
    let mut hashes = Vec::new();
    if s.eat_while("--hash").is_empty() {
        return Err(RequirementsTxtParserError::Parser {
            message: format!(
                "Expected '--hash', found '{:?}'",
                s.eat_while(|c: char| !c.is_whitespace())
            ),
            location: s.cursor(),
        });
    }
    let hash = parse_value(s, |c: char| !c.is_whitespace())?;
    hashes.push(hash.to_string());
    loop {
        eat_wrappable_whitespace(s);
        if !s.eat_if("--hash") {
            break;
        }
        let hash = parse_value(s, |c: char| !c.is_whitespace())?;
        hashes.push(hash.to_string());
    }
    Ok(hashes)
}

/// In `-<key>=<value>` or `-<key> value`, this parses the part after the key
fn parse_value<'a, T>(
    s: &mut Scanner<'a>,
    while_pattern: impl Pattern<T>,
) -> Result<&'a str, RequirementsTxtParserError> {
    if s.eat_if('=') {
        // Explicit equals sign
        Ok(s.eat_while(while_pattern).trim_end())
    } else if s.eat_if(char::is_whitespace) {
        // Key and value are separated by whitespace instead
        s.eat_whitespace();
        Ok(s.eat_while(while_pattern).trim_end())
    } else {
        Err(RequirementsTxtParserError::Parser {
            message: format!("Expected '=' or whitespace, found {:?}", s.peek()),
            location: s.cursor(),
        })
    }
}

/// Error parsing requirements.txt, wrapper with filename
#[derive(Debug)]
pub struct RequirementsTxtFileError {
    file: PathBuf,
    error: RequirementsTxtParserError,
}

/// Error parsing requirements.txt, error disambiguation
#[derive(Debug)]
pub enum RequirementsTxtParserError {
    IO(io::Error),
    InvalidEditablePath(String),
    UnsupportedUrl(String),
    Parser {
        message: String,
        location: usize,
    },
    UnsupportedRequirement {
        source: Pep508Error,
        start: usize,
        end: usize,
    },
    Pep508 {
        source: Pep508Error,
        start: usize,
        end: usize,
    },
    Subfile {
        source: Box<RequirementsTxtFileError>,
        start: usize,
        end: usize,
    },
}

impl RequirementsTxtParserError {
    /// Add a fixed offset to the location of the error.
    #[must_use]
    fn with_offset(self, offset: usize) -> Self {
        match self {
            RequirementsTxtParserError::IO(err) => RequirementsTxtParserError::IO(err),
            RequirementsTxtParserError::InvalidEditablePath(given) => {
                RequirementsTxtParserError::InvalidEditablePath(given)
            }
            RequirementsTxtParserError::UnsupportedUrl(url) => {
                RequirementsTxtParserError::UnsupportedUrl(url)
            }
            RequirementsTxtParserError::Parser { message, location } => {
                RequirementsTxtParserError::Parser {
                    message,
                    location: location + offset,
                }
            }
            RequirementsTxtParserError::UnsupportedRequirement { source, start, end } => {
                RequirementsTxtParserError::UnsupportedRequirement {
                    source,
                    start: start + offset,
                    end: end + offset,
                }
            }
            RequirementsTxtParserError::Pep508 { source, start, end } => {
                RequirementsTxtParserError::Pep508 {
                    source,
                    start: start + offset,
                    end: end + offset,
                }
            }
            RequirementsTxtParserError::Subfile { source, start, end } => {
                RequirementsTxtParserError::Subfile {
                    source,
                    start: start + offset,
                    end: end + offset,
                }
            }
        }
    }
}

impl Display for RequirementsTxtParserError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RequirementsTxtParserError::IO(err) => err.fmt(f),
            RequirementsTxtParserError::InvalidEditablePath(given) => {
                write!(f, "Invalid editable path: {given}")
            }
            RequirementsTxtParserError::UnsupportedUrl(url) => {
                write!(f, "Unsupported URL (expected a `file://` scheme): {url}")
            }
            RequirementsTxtParserError::Parser { message, location } => {
                write!(f, "{message} at position {location}")
            }
            RequirementsTxtParserError::UnsupportedRequirement { start, end, .. } => {
                write!(f, "Unsupported requirement in position {start} to {end}")
            }
            RequirementsTxtParserError::Pep508 { start, .. } => {
                write!(f, "Couldn't parse requirement at position {start}")
            }
            RequirementsTxtParserError::Subfile { start, .. } => {
                write!(f, "Error parsing included file at position {start}")
            }
        }
    }
}

impl std::error::Error for RequirementsTxtParserError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self {
            RequirementsTxtParserError::IO(err) => err.source(),
            RequirementsTxtParserError::InvalidEditablePath(_) => None,
            RequirementsTxtParserError::UnsupportedUrl(_) => None,
            RequirementsTxtParserError::UnsupportedRequirement { source, .. } => Some(source),
            RequirementsTxtParserError::Pep508 { source, .. } => Some(source),
            RequirementsTxtParserError::Subfile { source, .. } => Some(source.as_ref()),
            RequirementsTxtParserError::Parser { .. } => None,
        }
    }
}

impl Display for RequirementsTxtFileError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.error {
            RequirementsTxtParserError::IO(err) => err.fmt(f),
            RequirementsTxtParserError::InvalidEditablePath(given) => {
                write!(
                    f,
                    "Invalid editable path in `{}`: {given}",
                    self.file.normalized_display()
                )
            }
            RequirementsTxtParserError::UnsupportedUrl(url) => {
                write!(
                    f,
                    "Unsupported URL (expected a `file://` scheme) in `{}`: {url}",
                    self.file.normalized_display(),
                )
            }
            RequirementsTxtParserError::Parser { message, location } => {
                write!(
                    f,
                    "{message} in `{}` at position {location}",
                    self.file.normalized_display(),
                )
            }
            RequirementsTxtParserError::UnsupportedRequirement { start, end, .. } => {
                write!(
                    f,
                    "Unsupported requirement in {} position {} to {}",
                    self.file.normalized_display(),
                    start,
                    end,
                )
            }
            RequirementsTxtParserError::Pep508 { start, .. } => {
                write!(
                    f,
                    "Couldn't parse requirement in `{}` at position {start}",
                    self.file.normalized_display(),
                )
            }
            RequirementsTxtParserError::Subfile { start, .. } => {
                write!(
                    f,
                    "Error parsing included file in `{}` at position {start}",
                    self.file.normalized_display(),
                )
            }
        }
    }
}

impl std::error::Error for RequirementsTxtFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.error.source()
    }
}

impl From<io::Error> for RequirementsTxtParserError {
    fn from(err: io::Error) -> Self {
        Self::IO(err)
    }
}

#[cfg(test)]
mod test {
    use std::path::{Path, PathBuf};

    use anyhow::Result;
    use assert_fs::prelude::*;
    use fs_err as fs;
    use indoc::indoc;
    use itertools::Itertools;
    use tempfile::tempdir;
    use test_case::test_case;

    use crate::{EditableRequirement, RequirementsTxt};

    fn workspace_test_data_dir() -> PathBuf {
        PathBuf::from("./test-data")
    }

    #[test_case(Path::new("basic.txt"))]
    #[test_case(Path::new("constraints-a.txt"))]
    #[test_case(Path::new("constraints-b.txt"))]
    #[test_case(Path::new("empty.txt"))]
    #[test_case(Path::new("for-poetry.txt"))]
    #[test_case(Path::new("include-a.txt"))]
    #[test_case(Path::new("include-b.txt"))]
    #[test_case(Path::new("poetry-with-hashes.txt"))]
    #[test_case(Path::new("small.txt"))]
    #[test_case(Path::new("whitespace.txt"))]
    fn parse(path: &Path) {
        let working_dir = workspace_test_data_dir().join("requirements-txt");
        let requirements_txt = working_dir.join(path);

        let actual = RequirementsTxt::parse(requirements_txt, &working_dir).unwrap();

        let snapshot = format!("parse-{}", path.to_string_lossy());
        insta::assert_debug_snapshot!(snapshot, actual);
    }

    #[test_case(Path::new("basic.txt"))]
    #[test_case(Path::new("constraints-a.txt"))]
    #[test_case(Path::new("constraints-b.txt"))]
    #[test_case(Path::new("empty.txt"))]
    #[test_case(Path::new("for-poetry.txt"))]
    #[test_case(Path::new("include-a.txt"))]
    #[test_case(Path::new("include-b.txt"))]
    #[test_case(Path::new("poetry-with-hashes.txt"))]
    #[test_case(Path::new("small.txt"))]
    #[test_case(Path::new("whitespace.txt"))]
    #[test_case(Path::new("editable.txt"))]
    fn line_endings(path: &Path) {
        let working_dir = workspace_test_data_dir().join("requirements-txt");
        let requirements_txt = working_dir.join(path);

        // Replace line endings with the other choice. This works even if you use git with LF
        // only on windows.
        let contents = fs::read_to_string(requirements_txt).unwrap();
        let contents = if contents.contains("\r\n") {
            contents.replace("\r\n", "\n")
        } else {
            contents.replace('\n', "\r\n")
        };

        // Write to a new file.
        let temp_dir = tempdir().unwrap();
        let requirements_txt = temp_dir.path().join(path);
        fs::write(&requirements_txt, contents).unwrap();

        let actual = RequirementsTxt::parse(&requirements_txt, &working_dir).unwrap();

        let snapshot = format!("line-endings-{}", path.to_string_lossy());
        insta::assert_debug_snapshot!(snapshot, actual);
    }

    #[test]
    fn invalid_include_missing_file() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let missing_txt = temp_dir.child("missing.txt");
        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            -r missing.txt
        "})?;

        let error = RequirementsTxt::parse(requirements_txt.path(), temp_dir.path()).unwrap_err();
        let errors = anyhow::Error::new(error)
            .chain()
            // The last error is operating-system specific.
            .take(2)
            .map(|err| err.to_string().replace('\\', "/"))
            .join("\n");

        insta::with_settings!({
            filters => vec![
                (requirements_txt.path().to_str().unwrap(), "<REQUIREMENTS_TXT>"),
                (missing_txt.path().to_str().unwrap(), "<MISSING_TXT>"),
            ],
        }, {
            insta::assert_display_snapshot!(errors, @r###"
            Error parsing included file in `<REQUIREMENTS_TXT>` at position 0
            failed to open file `<MISSING_TXT>`
            "###);
        });

        Ok(())
    }

    #[test]
    fn invalid_requirement() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            numpy[ö]==1.29
        "})?;

        let error = RequirementsTxt::parse(requirements_txt.path(), temp_dir.path()).unwrap_err();
        let errors = anyhow::Error::new(error)
            .chain()
            .map(|err| err.to_string().replace('\\', "/"))
            .join("\n");

        insta::with_settings!({
            filters => vec![(requirements_txt.path().to_str().unwrap(), "<REQUIREMENTS_TXT>")]
        }, {
            insta::assert_display_snapshot!(errors, @r###"
            Couldn't parse requirement in `<REQUIREMENTS_TXT>` at position 0
            Expected an alphanumeric character starting the extra name, found 'ö'
            numpy[ö]==1.29
                  ^
            "###);
        });

        Ok(())
    }

    #[test]
    fn unsupported_editable() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            -e http://localhost:8080/
        "})?;

        let error = RequirementsTxt::parse(requirements_txt.path(), temp_dir.path()).unwrap_err();
        let errors = anyhow::Error::new(error)
            .chain()
            .map(|err| err.to_string().replace('\\', "/"))
            .join("\n");

        insta::with_settings!({
            filters => vec![(requirements_txt.path().to_str().unwrap(), "<REQUIREMENTS_TXT>")]
        }, {
            insta::assert_display_snapshot!(errors, @"Unsupported URL (expected a `file://` scheme) in `<REQUIREMENTS_TXT>`: http://localhost:8080/");
        });

        Ok(())
    }

    #[test]
    fn invalid_editable_extra() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            -e black[,abcdef]
        "})?;

        let error = RequirementsTxt::parse(requirements_txt.path(), temp_dir.path()).unwrap_err();
        let errors = anyhow::Error::new(error)
            .chain()
            .map(|err| err.to_string().replace('\\', "/"))
            .join("\n");

        insta::with_settings!({
            filters => vec![(requirements_txt.path().to_str().unwrap(), "<REQUIREMENTS_TXT>")]
        }, {
            insta::assert_display_snapshot!(errors, @r###"
            Couldn't parse requirement in `<REQUIREMENTS_TXT>` at position 6
            Expected an alphanumeric character starting the extra name, found ','
            black[,abcdef]
                  ^
            "###);
        });

        Ok(())
    }

    #[test]
    fn editable_extra() {
        assert_eq!(
            EditableRequirement::split_extras("../editable[dev]"),
            Some(("../editable", "[dev]"))
        );
        assert_eq!(
            EditableRequirement::split_extras("../editable[dev]more[extra]"),
            Some(("../editable[dev]more", "[extra]"))
        );
        assert_eq!(
            EditableRequirement::split_extras("../editable[[dev]]"),
            None
        );
        assert_eq!(
            EditableRequirement::split_extras("../editable[[dev]"),
            Some(("../editable[", "[dev]"))
        );
    }
}

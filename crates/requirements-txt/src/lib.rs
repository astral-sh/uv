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
use std::io::Error;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use fs_err as fs;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tracing::warn;
use unscanny::{Pattern, Scanner};
use url::Url;

use pep508_rs::{Pep508Error, Pep508ErrorSource, Requirement, VerbatimUrl};

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
    EditableRequirement(ParsedEditableRequirement),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EditableRequirement {
    pub url: VerbatimUrl,
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

impl FromStr for EditableRequirement {
    type Err = RequirementsTxtParserError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        static CWD: Lazy<PathBuf> = Lazy::new(|| std::env::current_dir().unwrap());

        let editable_requirement = ParsedEditableRequirement::from(s.to_string());
        editable_requirement.with_working_dir(&*CWD)
    }
}

/// A raw string for an editable requirement (`pip install -e <editable>`), which could be a URL or
/// a local path, and could contain unexpanded environment variables.
///
/// For example:
/// - `file:///home/ferris/project/scripts/...`
/// - `file:../editable/`
/// - `../editable/`
///
/// We disallow URLs with schemes other than `file://` (e.g., `https://...`).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ParsedEditableRequirement(String);

/// Like [`Url::parse`], but only splits the scheme. Derived from the `url` crate.
fn split_scheme(s: &str) -> Option<(&str, &str)> {
    /// <https://url.spec.whatwg.org/#c0-controls-and-space>
    #[inline]
    fn c0_control_or_space(ch: char) -> bool {
        ch <= ' ' // U+0000 to U+0020
    }

    /// <https://url.spec.whatwg.org/#ascii-alpha>
    #[inline]
    fn ascii_alpha(ch: char) -> bool {
        ch.is_ascii_alphabetic()
    }

    // Trim control characters and spaces from the start and end.
    let s = s.trim_matches(c0_control_or_space);
    if s.is_empty() || !s.starts_with(ascii_alpha) {
        return None;
    }

    // Find the `:` following any alpha characters.
    let mut iter = s.char_indices();
    let end = loop {
        match iter.next() {
            Some((_i, 'a'..='z' | 'A'..='Z' | '0'..='9' | '+' | '-' | '.')) => {}
            Some((i, ':')) => break i,
            _ => return None,
        }
    };

    let scheme = &s[..end];
    let rest = &s[end + 1..];
    Some((scheme, rest))
}

impl ParsedEditableRequirement {
    pub fn with_working_dir(
        self,
        working_dir: impl AsRef<Path>,
    ) -> Result<EditableRequirement, RequirementsTxtParserError> {
        let given = self.0;

        // Create a `VerbatimUrl` to represent the editable requirement.
        let url = if let Some((scheme, path)) = split_scheme(&given) {
            if scheme == "file" {
                if let Some(path) = path.strip_prefix("//") {
                    // Ex) `file:///home/ferris/project/scripts/...`
                    VerbatimUrl::from_path(path, working_dir)
                } else {
                    // Ex) `file:../editable/`
                    VerbatimUrl::from_path(path, working_dir)
                }
            } else {
                // Ex) `https://...`
                return Err(RequirementsTxtParserError::UnsupportedUrl(
                    given.to_string(),
                ));
            }
        } else {
            // Ex) `../editable/`
            VerbatimUrl::from_path(&given, working_dir)
        };

        // Create a `PathBuf`.
        let path = url
            .to_file_path()
            .map_err(|()| RequirementsTxtParserError::InvalidEditablePath(given.clone()))?;

        // Add the verbatim representation of the URL to the `VerbatimUrl`.
        let url = url.with_given(given);

        Ok(EditableRequirement { url, path })
    }
}

impl From<String> for ParsedEditableRequirement {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl Display for ParsedEditableRequirement {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
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
        let data =
            Self::parse_inner(&content, working_dir).map_err(|err| RequirementsTxtFileError {
                file: requirements_txt.as_ref().to_path_buf(),
                error: err,
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
        working_dir: impl AsRef<Path>,
    ) -> Result<Self, RequirementsTxtParserError> {
        let mut s = Scanner::new(content);

        let mut data = Self::default();
        while let Some(statement) = parse_entry(&mut s, content)? {
            match statement {
                RequirementsTxtStatement::Requirements {
                    filename,
                    start,
                    end,
                } => {
                    let sub_file = working_dir.as_ref().join(filename);
                    let sub_requirements =
                        Self::parse(&sub_file, working_dir.as_ref()).map_err(|err| {
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
                    let sub_file = working_dir.as_ref().join(filename);
                    let sub_constraints =
                        Self::parse(&sub_file, working_dir.as_ref()).map_err(|err| {
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
                    data.editables
                        .push(editable.with_working_dir(&working_dir)?);
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
        let editable_requirement = ParsedEditableRequirement::from(path_or_url.to_string());
        RequirementsTxtStatement::EditableRequirement(editable_requirement)
    } else if s.at(char::is_ascii_alphanumeric) {
        let (requirement, hashes) = parse_requirement_and_hashes(s, content)?;
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
        Requirement::from_str(&content[start..end]).map_err(|err| match err.message {
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
            RequirementsTxtParserError::Pep508 { start, end, .. } => {
                write!(f, "Couldn't parse requirement in position {start} to {end}")
            }
            RequirementsTxtParserError::Subfile { start, end, .. } => {
                write!(
                    f,
                    "Error parsing file included at position {start} to {end}"
                )
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
                    "Invalid editable path in {}: {given}",
                    self.file.display()
                )
            }
            RequirementsTxtParserError::UnsupportedUrl(url) => {
                write!(
                    f,
                    "Unsupported URL (expected a `file://` scheme) in {}: {}",
                    self.file.display(),
                    url
                )
            }
            RequirementsTxtParserError::Parser { message, location } => {
                write!(
                    f,
                    "{} in {} position {}",
                    message,
                    self.file.display(),
                    location
                )
            }
            RequirementsTxtParserError::UnsupportedRequirement { start, end, .. } => {
                write!(
                    f,
                    "Unsupported requirement in {} position {} to {}",
                    self.file.display(),
                    start,
                    end,
                )
            }
            RequirementsTxtParserError::Pep508 { start, end, .. } => {
                write!(
                    f,
                    "Couldn't parse requirement in {} position {} to {}",
                    self.file.display(),
                    start,
                    end,
                )
            }
            RequirementsTxtParserError::Subfile { start, end, .. } => {
                write!(
                    f,
                    "Error parsing file included into {} at position {} to {}",
                    self.file.display(),
                    start,
                    end
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
    fn from(err: Error) -> Self {
        Self::IO(err)
    }
}

#[cfg(test)]
mod test {
    use std::path::{Path, PathBuf};

    use fs_err as fs;
    use indoc::indoc;
    use tempfile::tempdir;
    use test_case::test_case;

    use crate::{split_scheme, RequirementsTxt};

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
    fn invalid_include_missing_file() {
        let working_dir = workspace_test_data_dir().join("requirements-txt");
        let basic = working_dir.join("invalid-include.txt");
        let missing = working_dir.join("missing.txt");
        let err = RequirementsTxt::parse(&basic, &working_dir).unwrap_err();
        let errors = anyhow::Error::new(err)
            .chain()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        assert_eq!(errors.len(), 3);
        assert_eq!(
            errors[0],
            format!(
                "Error parsing file included into {} at position 0 to 14",
                basic.display()
            )
        );
        assert_eq!(
            errors[1],
            format!("failed to open file `{}`", missing.display()),
        );
        // The last error message is os specific
    }

    #[test]
    fn invalid_requirement() {
        let working_dir = workspace_test_data_dir().join("requirements-txt");
        let basic = working_dir.join("invalid-requirement.txt");
        let err = RequirementsTxt::parse(&basic, &working_dir).unwrap_err();
        let errors = anyhow::Error::new(err)
            .chain()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let expected = &[
            format!(
                "Couldn't parse requirement in {} position 0 to 15",
                basic.display()
            ),
            indoc! {"
                Expected an alphanumeric character starting the extra name, found 'ö'
                numpy[ö]==1.29
                      ^"
            }
            .to_string(),
        ];
        assert_eq!(errors, expected);
    }

    fn workspace_test_data_dir() -> PathBuf {
        PathBuf::from("./test-data")
    }

    #[test]
    fn unsupported_editable() {
        let working_dir = workspace_test_data_dir().join("requirements-txt");
        let basic = working_dir.join("unsupported-editable.txt");
        let err = RequirementsTxt::parse(basic, &working_dir).unwrap_err();
        let errors = anyhow::Error::new(err)
            .chain()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let expected = &[
            "Unsupported URL (expected a `file://` scheme) in ./test-data/requirements-txt/unsupported-editable.txt: http://localhost:8080/".to_string()
        ];
        assert_eq!(errors, expected);
    }

    #[test]
    fn scheme() {
        assert_eq!(
            split_scheme("file:///home/ferris/project/scripts"),
            Some(("file", "///home/ferris/project/scripts"))
        );
        assert_eq!(
            split_scheme("file:home/ferris/project/scripts"),
            Some(("file", "home/ferris/project/scripts"))
        );
        assert_eq!(
            split_scheme("https://example.com"),
            Some(("https", "//example.com"))
        );
        assert_eq!(split_scheme("https:"), Some(("https", "")));
    }
}

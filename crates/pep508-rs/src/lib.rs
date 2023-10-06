//! A library for python [dependency specifiers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
//! better known as [PEP 508](https://peps.python.org/pep-0508/)
//!
//! ## Usage
//!
//! ```
//! use std::str::FromStr;
//! use pep508_rs::Requirement;
//!
//! let marker = r#"requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8""#;
//! let dependency_specification = Requirement::from_str(marker).unwrap();
//! assert_eq!(dependency_specification.name, "requests");
//! assert_eq!(dependency_specification.extras, Some(vec!["security".to_string(), "tests".to_string()]));
//! ```

#![deny(missing_docs)]

mod marker;
#[cfg(feature = "modern")]
pub mod modern;

pub use marker::{
    MarkerEnvironment, MarkerExpression, MarkerOperator, MarkerTree, MarkerValue,
    MarkerValueString, MarkerValueVersion, MarkerWarningKind, StringVersion,
};
#[cfg(feature = "pyo3")]
use pep440_rs::PyVersion;
use pep440_rs::{Version, VersionSpecifier, VersionSpecifiers};
#[cfg(feature = "pyo3")]
use pyo3::{
    basic::CompareOp, create_exception, exceptions::PyNotImplementedError, pyclass, pymethods,
    pymodule, types::PyModule, IntoPy, PyObject, PyResult, Python,
};
#[cfg(feature = "serde")]
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
#[cfg(feature = "pyo3")]
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
#[cfg(feature = "pyo3")]
use std::hash::{Hash, Hasher};
use std::str::{Chars, FromStr};
use thiserror::Error;
use unicode_width::UnicodeWidthStr;
use url::Url;

/// Error with a span attached. Not that those aren't `String` but `Vec<char>` indices.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Pep508Error {
    /// Either we have an error string from our parser or an upstream error from `url`
    pub message: Pep508ErrorSource,
    /// Span start index
    pub start: usize,
    /// Span length
    pub len: usize,
    /// The input string so we can print it underlined
    pub input: String,
}

/// Either we have an error string from our parser or an upstream error from `url`
#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum Pep508ErrorSource {
    /// An error from our parser
    String(String),
    /// A url parsing error
    #[error(transparent)]
    UrlError(#[from] url::ParseError),
}

impl Display for Pep508ErrorSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Pep508ErrorSource::String(string) => string.fmt(f),
            Pep508ErrorSource::UrlError(parse_err) => parse_err.fmt(f),
        }
    }
}

impl Display for Pep508Error {
    /// Pretty formatting with underline
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // We can use char indices here since it's a Vec<char>
        let start_offset = self
            .input
            .chars()
            .take(self.start)
            .collect::<String>()
            .width();
        let underline_len = if self.start == self.input.len() {
            // We also allow 0 here for convenience
            assert!(
                self.len <= 1,
                "Can only go one past the input not {}",
                self.len
            );
            1
        } else {
            self.input
                .chars()
                .skip(self.start)
                .take(self.len)
                .collect::<String>()
                .width()
        };
        write!(
            f,
            "{}\n{}\n{}{}",
            self.message,
            self.input,
            " ".repeat(start_offset),
            "^".repeat(underline_len)
        )
    }
}

/// We need this to allow e.g. anyhow's `.context()`
impl std::error::Error for Pep508Error {}

#[cfg(feature = "pyo3")]
create_exception!(
    pep508,
    PyPep508Error,
    pyo3::exceptions::PyValueError,
    "A PEP 508 parser error with span information"
);

/// A PEP 508 dependency specification
#[cfg_attr(feature = "pyo3", pyclass(module = "pep508"))]
#[derive(Hash, Debug, Clone, Eq, PartialEq)]
pub struct Requirement {
    /// The distribution name such as `numpy` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`
    pub name: String,
    /// The list of extras such as `security`, `tests` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`
    pub extras: Option<Vec<String>>,
    /// The version specifier such as `>= 2.8.1`, `== 2.8.*` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`
    /// or a url
    pub version_or_url: Option<VersionOrUrl>,
    /// The markers such as `python_version > "3.8"` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`.
    /// Those are a nested and/or tree
    pub marker: Option<MarkerTree>,
}

impl Display for Requirement {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(extras) = &self.extras {
            write!(f, "[{}]", extras.join(","))?;
        }
        if let Some(version_or_url) = &self.version_or_url {
            match version_or_url {
                VersionOrUrl::VersionSpecifier(version_specifier) => {
                    let version_specifier: Vec<String> =
                        version_specifier.iter().map(ToString::to_string).collect();
                    write!(f, " {}", version_specifier.join(", "))?;
                }
                VersionOrUrl::Url(url) => {
                    // We add the space for markers later if necessary
                    write!(f, " @ {}", url)?;
                }
            }
        }
        if let Some(marker) = &self.marker {
            write!(f, " ; {}", marker)?;
        }
        Ok(())
    }
}

/// https://github.com/serde-rs/serde/issues/908#issuecomment-298027413
#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Requirement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        FromStr::from_str(&s).map_err(de::Error::custom)
    }
}

/// https://github.com/serde-rs/serde/issues/1316#issue-332908452
#[cfg(feature = "serde")]
impl Serialize for Requirement {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl Requirement {
    /// The distribution name such as `numpy` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`
    #[getter]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// The list of extras such as `security`, `tests` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`
    #[getter]
    pub fn extras(&self) -> Option<Vec<String>> {
        self.extras.clone()
    }

    /// The marker expression such as  `python_version > "3.8"` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`
    #[getter]
    pub fn marker(&self) -> Option<String> {
        self.marker.as_ref().map(|m| m.to_string())
    }

    /// Parses a PEP 440 string
    #[new]
    pub fn py_new(requirement: &str) -> PyResult<Self> {
        Self::from_str(requirement).map_err(|err| PyPep508Error::new_err(err.to_string()))
    }

    #[getter]
    fn version_or_url(&self, py: Python<'_>) -> PyObject {
        match &self.version_or_url {
            None => py.None(),
            Some(VersionOrUrl::VersionSpecifier(version_specifier)) => version_specifier
                .iter()
                .map(|x| x.clone().into_py(py))
                .collect::<Vec<PyObject>>()
                .into_py(py),
            Some(VersionOrUrl::Url(url)) => url.to_string().into_py(py),
        }
    }

    fn __str__(&self) -> String {
        self.to_string()
    }

    fn __repr__(&self) -> String {
        format!(r#""{}""#, self)
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp) -> PyResult<bool> {
        let err = PyNotImplementedError::new_err("Requirement only supports equality comparisons");
        match op {
            CompareOp::Lt => Err(err),
            CompareOp::Le => Err(err),
            CompareOp::Eq => Ok(self == other),
            CompareOp::Ne => Ok(self != other),
            CompareOp::Gt => Err(err),
            CompareOp::Ge => Err(err),
        }
    }

    fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    /// Returns whether the markers apply for the given environment
    #[pyo3(name = "evaluate_markers")]
    pub fn py_evaluate_markers(&self, env: &MarkerEnvironment, extras: Vec<String>) -> bool {
        self.evaluate_markers(env, extras)
    }

    /// Returns whether the requirement would be satisfied, independent of environment markers, i.e.
    /// if there is potentially an environment that could activate this requirement.
    ///
    /// Note that unlike [Self::evaluate_markers] this does not perform any checks for bogus
    /// expressions but will simply return true. As caller you should separately perform a check
    /// with an environment and forward all warnings.
    #[pyo3(name = "evaluate_extras_and_python_version")]
    pub fn py_evaluate_extras_and_python_version(
        &self,
        extras: HashSet<String>,
        python_versions: Vec<PyVersion>,
    ) -> bool {
        self.evaluate_extras_and_python_version(
            extras,
            python_versions
                .into_iter()
                .map(|py_version| py_version.0)
                .collect(),
        )
    }

    /// Returns whether the markers apply for the given environment
    #[pyo3(name = "evaluate_markers_and_report")]
    pub fn py_evaluate_markers_and_report(
        &self,
        env: &MarkerEnvironment,
        extras: Vec<String>,
    ) -> (bool, Vec<(MarkerWarningKind, String, String)>) {
        self.evaluate_markers_and_report(env, extras)
    }
}

impl Requirement {
    /// Returns whether the markers apply for the given environment
    pub fn evaluate_markers(&self, env: &MarkerEnvironment, extras: Vec<String>) -> bool {
        if let Some(marker) = &self.marker {
            marker.evaluate(
                env,
                &extras.iter().map(String::as_str).collect::<Vec<&str>>(),
            )
        } else {
            true
        }
    }

    /// Returns whether the requirement would be satisfied, independent of environment markers, i.e.
    /// if there is potentially an environment that could activate this requirement.
    ///
    /// Note that unlike [Self::evaluate_markers] this does not perform any checks for bogus
    /// expressions but will simply return true. As caller you should separately perform a check
    /// with an environment and forward all warnings.
    pub fn evaluate_extras_and_python_version(
        &self,
        extras: HashSet<String>,
        python_versions: Vec<Version>,
    ) -> bool {
        if let Some(marker) = &self.marker {
            marker.evaluate_extras_and_python_version(&extras, &python_versions)
        } else {
            true
        }
    }

    /// Returns whether the markers apply for the given environment
    pub fn evaluate_markers_and_report(
        &self,
        env: &MarkerEnvironment,
        extras: Vec<String>,
    ) -> (bool, Vec<(MarkerWarningKind, String, String)>) {
        if let Some(marker) = &self.marker {
            marker.evaluate_collect_warnings(
                env,
                &extras.iter().map(|x| x.as_str()).collect::<Vec<&str>>(),
            )
        } else {
            (true, Vec::new())
        }
    }
}

impl FromStr for Requirement {
    type Err = Pep508Error;

    /// Parse a [Dependency Specifier](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parse(&mut CharIter::new(input))
    }
}

impl Requirement {
    /// Parse a [Dependency Specifier](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
    pub fn parse(input: &mut CharIter) -> Result<Self, Pep508Error> {
        parse(input)
    }
}

/// The actual version specifier or url to install
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub enum VersionOrUrl {
    /// A PEP 440 version specifier set
    VersionSpecifier(VersionSpecifiers),
    /// A installable URL
    Url(Url),
}

/// A `Vec<char>` and an index inside of it. Like [String], but with utf-8 aware indexing
pub struct CharIter<'a> {
    input: &'a str,
    chars: Chars<'a>,
    /// char-based (not byte-based) position
    pos: usize,
}

impl<'a> CharIter<'a> {
    /// Convert from `&str`
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.chars(),
            pos: 0,
        }
    }

    fn copy_chars(&self) -> String {
        self.input.to_string()
    }

    fn peek(&self) -> Option<(usize, char)> {
        self.chars.clone().next().map(|char| (self.pos, char))
    }

    fn eat(&mut self, token: char) -> Option<usize> {
        let (start_pos, peek_char) = self.peek()?;
        if peek_char == token {
            self.next();
            Some(start_pos)
        } else {
            None
        }
    }

    fn next(&mut self) -> Option<(usize, char)> {
        let next = (self.pos, self.chars.next()?);
        self.pos += 1;
        Some(next)
    }

    fn peek_char(&self) -> Option<char> {
        self.chars.clone().next()
    }

    fn get_pos(&self) -> usize {
        self.pos
    }

    fn peek_while(&mut self, condition: impl Fn(char) -> bool) -> (String, usize, usize) {
        let peeker = self.chars.clone();
        let start = self.get_pos();
        let mut len = 0;
        let substring = peeker
            .take_while(|c| {
                if condition(*c) {
                    len += 1;
                    true
                } else {
                    false
                }
            })
            .collect::<String>();
        (substring, start, len)
    }

    fn take_while(&mut self, condition: impl Fn(char) -> bool) -> (String, usize, usize) {
        // no pretty, but works
        let mut substring = String::new();
        let start = self.get_pos();
        let mut len = 0;
        while let Some(char) = self.peek_char() {
            if !condition(char) {
                break;
            } else {
                substring.push(char);
                self.next();
                len += 1;
            }
        }
        (substring, start, len)
    }

    fn next_expect_char(&mut self, expected: char, span_start: usize) -> Result<(), Pep508Error> {
        match self.next() {
            None => Err(Pep508Error {
                message: Pep508ErrorSource::String(format!(
                    "Expected '{}', found end of dependency specification",
                    expected
                )),
                start: span_start,
                len: 1,
                input: self.copy_chars(),
            }),
            Some((_, value)) if value == expected => Ok(()),
            Some((pos, other)) => Err(Pep508Error {
                message: Pep508ErrorSource::String(format!(
                    "Expected '{}', found '{}'",
                    expected, other
                )),
                start: pos,
                len: 1,
                input: self.copy_chars(),
            }),
        }
    }

    fn eat_whitespace(&mut self) {
        while let Some(char) = self.peek_char() {
            if char.is_whitespace() {
                self.next();
            } else {
                return;
            }
        }
    }
}

fn parse_name(chars: &mut CharIter) -> Result<String, Pep508Error> {
    // https://peps.python.org/pep-0508/#names
    // ^([A-Z0-9]|[A-Z0-9][A-Z0-9._-]*[A-Z0-9])$ with re.IGNORECASE
    let mut name = String::new();
    if let Some((index, char)) = chars.next() {
        if matches!(char, 'A'..='Z' | 'a'..='z' | '0'..='9') {
            name.push(char);
        } else {
            return Err(Pep508Error {
                message: Pep508ErrorSource::String(format!(
                    "Expected package name starting with an alphanumeric character, found '{}'",
                    char
                )),
                start: index,
                len: 1,
                input: chars.copy_chars(),
            });
        }
    } else {
        return Err(Pep508Error {
            message: Pep508ErrorSource::String("Empty field is not allowed for PEP508".to_string()),
            start: 0,
            len: 1,
            input: chars.copy_chars(),
        });
    }

    loop {
        match chars.peek() {
            Some((index, char @ ('A'..='Z' | 'a'..='z' | '0'..='9' | '.' | '-' | '_'))) => {
                name.push(char);
                chars.next();
                // [.-_] can't be the final character
                if chars.peek().is_none() && matches!(char, '.' | '-' | '_') {
                    return Err(Pep508Error {
                        message: Pep508ErrorSource::String(format!(
                            "Package name must end with an alphanumeric character, not '{}'",
                            char
                        )),
                        start: index,
                        len: 1,
                        input: chars.copy_chars(),
                    });
                }
            }
            Some(_) | None => return Ok(name),
        }
    }
}

/// parses extras in the `[extra1,extra2] format`
fn parse_extras(chars: &mut CharIter) -> Result<Option<Vec<String>>, Pep508Error> {
    let bracket_pos = match chars.eat('[') {
        Some(pos) => pos,
        None => return Ok(None),
    };
    let mut extras = Vec::new();

    loop {
        // wsp* before the identifier
        chars.eat_whitespace();
        let mut buffer = String::new();
        let early_eof_error = Pep508Error {
            message: Pep508ErrorSource::String(
                "Missing closing bracket (expected ']', found end of dependency specification)"
                    .to_string(),
            ),
            start: bracket_pos,
            len: 1,
            input: chars.copy_chars(),
        };

        // First char of the identifier
        match chars.next() {
            // letterOrDigit
            Some((_, alphanumeric @ ('a'..='z' | 'A'..='Z' | '0'..='9'))) => {
                buffer.push(alphanumeric)
            }
            Some((pos, other)) => {
                return Err(Pep508Error {
                    message: Pep508ErrorSource::String(format!(
                        "Expected an alphanumeric character starting the extra name, found '{}'",
                        other
                    )),
                    start: pos,
                    len: 1,
                    input: chars.copy_chars(),
                })
            }
            None => return Err(early_eof_error),
        }
        // Parse from the second char of the identifier
        // We handle the illegal character case below
        // identifier_end = letterOrDigit | (('-' | '_' | '.' )* letterOrDigit)
        // identifier_end*
        buffer.push_str(
            &chars
                .take_while(
                    |char| matches!(char, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.'),
                )
                .0,
        );
        match chars.peek() {
            Some((pos, char)) if char != ',' && char != ']' && !char.is_whitespace() => {
                return Err(Pep508Error {
                    message: Pep508ErrorSource::String(format!(
                        "Invalid character in extras name, expected an alphanumeric character, '-', '_', '.', ',' or ']', found '{}'", char
                    )),
                    start: pos,
                    len: 1,
                    input: chars.copy_chars(),
                })

            }
            _=>{}
        };
        // wsp* after the identifier
        chars.eat_whitespace();
        // end or next identifier?
        match chars.next() {
            Some((_, ',')) => {
                extras.push(buffer);
            }
            Some((_, ']')) => {
                extras.push(buffer);
                break;
            }
            Some((pos, other)) => {
                return Err(Pep508Error {
                    message: Pep508ErrorSource::String(format!(
                        "Expected either ',' (separating extras) or ']' (ending the extras section), found '{other}'"
                    )),
                    start: pos,
                    len: 1,
                    input: chars.copy_chars(),
                })
            }
            None => return Err(early_eof_error),
        }
    }

    Ok(Some(extras))
}

fn parse_url(chars: &mut CharIter) -> Result<VersionOrUrl, Pep508Error> {
    // wsp*
    chars.eat_whitespace();
    // <URI_reference>
    let (url, start, len) = chars.take_while(|char| !char.is_whitespace());
    if url.is_empty() {
        return Err(Pep508Error {
            message: Pep508ErrorSource::String("Expected URL".to_string()),
            start,
            len,
            input: chars.copy_chars(),
        });
    }
    let url = Url::parse(&url).map_err(|err| Pep508Error {
        message: Pep508ErrorSource::UrlError(err),
        start,
        len,
        input: chars.copy_chars(),
    })?;
    Ok(VersionOrUrl::Url(url))
}

/// PEP 440 wrapper
fn parse_specifier(
    chars: &mut CharIter,
    buffer: &str,
    start: usize,
    end: usize,
) -> Result<VersionSpecifier, Pep508Error> {
    VersionSpecifier::from_str(buffer).map_err(|err| Pep508Error {
        message: Pep508ErrorSource::String(err),
        start,
        len: end - start,
        input: chars.copy_chars(),
    })
}

/// Such as `>=1.19,<2.0`, either delimited by the end of the specifier or a `;` for the marker part
///
/// ```text
/// version_one (wsp* ',' version_one)*
/// ```
fn parse_version_specifier(chars: &mut CharIter) -> Result<Option<VersionOrUrl>, Pep508Error> {
    let mut start = chars.get_pos();
    let mut specifiers = Vec::new();
    let mut buffer = String::new();
    let requirement_kind = loop {
        match chars.peek() {
            Some((end, ',')) => {
                let specifier = parse_specifier(chars, &buffer, start, end)?;
                specifiers.push(specifier);
                buffer.clear();
                chars.next();
                start = end + 1;
            }
            Some((_, ';')) | None => {
                let end = chars.get_pos();
                let specifier = parse_specifier(chars, &buffer, start, end)?;
                specifiers.push(specifier);
                break Some(VersionOrUrl::VersionSpecifier(
                    specifiers.into_iter().collect(),
                ));
            }
            Some((_, char)) => {
                buffer.push(char);
                chars.next();
            }
        }
    };
    Ok(requirement_kind)
}

/// Such as `(>=1.19,<2.0)`
///
/// ```text
/// '(' version_one (wsp* ',' version_one)* ')'
/// ```
fn parse_version_specifier_parentheses(
    chars: &mut CharIter,
) -> Result<Option<VersionOrUrl>, Pep508Error> {
    let brace_pos = chars.get_pos();
    chars.next();
    // Makes for slightly better error underline
    chars.eat_whitespace();
    let mut start = chars.get_pos();
    let mut specifiers = Vec::new();
    let mut buffer = String::new();
    let requirement_kind = loop {
        match chars.next() {
            Some((end, ',')) => {
                let specifier =
                    parse_specifier(chars, &buffer, start, end)?;
                specifiers.push(specifier);
                buffer.clear();
                start = end + 1;
            }
            Some((end, ')')) => {
                let specifier = parse_specifier(chars, &buffer, start, end)?;
                specifiers.push(specifier);
                break Some(VersionOrUrl::VersionSpecifier(specifiers.into_iter().collect()));
            },
            Some((_, char)) => buffer.push(char),
            None => return Err(Pep508Error {
                message: Pep508ErrorSource::String("Missing closing parenthesis (expected ')', found end of dependency specification)".to_string()),
                start: brace_pos,
                len: 1,
                input: chars.copy_chars(),
            }),
        }
    };
    Ok(requirement_kind)
}

/// Parse a [dependency specifier](https://packaging.python.org/en/latest/specifications/dependency-specifiers)
fn parse(chars: &mut CharIter) -> Result<Requirement, Pep508Error> {
    // Technically, the grammar is:
    // ```text
    // name_req      = name wsp* extras? wsp* versionspec? wsp* quoted_marker?
    // url_req       = name wsp* extras? wsp* urlspec wsp+ quoted_marker?
    // specification = wsp* ( url_req | name_req ) wsp*
    // ```
    // So we can merge this into:
    // ```text
    // specification = wsp* name wsp* extras? wsp* (('@' wsp* url_req) | ('(' versionspec ')') | (versionspec)) wsp* (';' wsp* marker)? wsp*
    // ```
    // Where the extras start with '[' if any, then we have '@', '(' or one of the version comparison
    // operators. Markers start with ';' if any
    // wsp*
    chars.eat_whitespace();
    // name
    let name = parse_name(chars)?;
    // wsp*
    chars.eat_whitespace();
    // extras?
    let extras = parse_extras(chars)?;
    // wsp*
    chars.eat_whitespace();

    // ( url_req | name_req )?
    let requirement_kind = match chars.peek_char() {
        Some('@') => {
            chars.next();
            Some(parse_url(chars)?)
        }
        Some('(') => parse_version_specifier_parentheses(chars)?,
        Some('<' | '=' | '>' | '~' | '!') => parse_version_specifier(chars)?,
        Some(';') | None => None,
        Some(other) => {
            return Err(Pep508Error {
                message: Pep508ErrorSource::String(format!(
                    "Expected one of `@`, `(`, `<`, `=`, `>`, `~`, `!`, `;`, found `{}`",
                    other
                )),
                start: chars.get_pos(),
                len: 1,
                input: chars.copy_chars(),
            })
        }
    };

    // wsp*
    chars.eat_whitespace();
    // quoted_marker?
    let marker = if chars.peek_char() == Some(';') {
        // Skip past the semicolon
        chars.next();
        Some(marker::parse_markers_impl(chars)?)
    } else {
        None
    };
    // wsp*
    chars.eat_whitespace();
    if let Some((pos, char)) = chars.next() {
        return Err(Pep508Error {
            message: Pep508ErrorSource::String(if marker.is_none() {
                format!(r#"Expected end of input or ';', found '{}'"#, char)
            } else {
                format!(r#"Expected end of input, found '{}'"#, char)
            }),
            start: pos,
            len: 1,
            input: chars.copy_chars(),
        });
    }

    Ok(Requirement {
        name,
        extras,
        version_or_url: requirement_kind,
        marker,
    })
}

/// A library for [dependency specifiers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
/// as originally specified in [PEP 508](https://peps.python.org/pep-0508/)
///
/// This has `Version` and `VersionSpecifier` included. That is because
/// `pep440_rs.Version("1.2.3") != pep508_rs.Requirement("numpy==1.2.3").version_or_url` as the
/// `Version`s come from two different binaries and can therefore never be equal.
#[cfg(feature = "pyo3")]
#[pymodule]
#[pyo3(name = "pep508_rs")]
pub fn python_module(py: Python<'_>, m: &PyModule) -> PyResult<()> {
    // Allowed to fail if we embed this module in another
    #[allow(unused_must_use)]
    {
        pyo3_log::try_init();
    }

    m.add_class::<PyVersion>()?;
    m.add_class::<VersionSpecifier>()?;

    m.add_class::<Requirement>()?;
    m.add_class::<MarkerEnvironment>()?;
    m.add("Pep508Error", py.get_type::<PyPep508Error>())?;
    Ok(())
}

/// Half of these tests are copied from https://github.com/pypa/packaging/pull/624
#[cfg(test)]
mod tests {
    use crate::marker::{
        parse_markers_impl, MarkerExpression, MarkerOperator, MarkerTree, MarkerValue,
        MarkerValueString, MarkerValueVersion,
    };
    use crate::{CharIter, Requirement, VersionOrUrl};
    use indoc::indoc;
    use pep440_rs::{Operator, Version, VersionSpecifier};
    use std::str::FromStr;
    use url::Url;

    fn assert_err(input: &str, error: &str) {
        assert_eq!(Requirement::from_str(input).unwrap_err().to_string(), error);
    }

    #[test]
    fn error_empty() {
        assert_err(
            "",
            indoc! {"
            Empty field is not allowed for PEP508
            
            ^"
            },
        );
    }

    #[test]
    fn error_start() {
        assert_err(
            "_name",
            indoc! {"
                Expected package name starting with an alphanumeric character, found '_'
                _name
                ^"
            },
        );
    }

    #[test]
    fn error_end() {
        assert_err(
            "name_",
            indoc! {"
                Package name must end with an alphanumeric character, not '_'
                name_
                    ^"
            },
        );
    }

    #[test]
    fn basic_examples() {
        let input = r#"requests[security,tests] >=2.8.1, ==2.8.* ; python_version < '2.7'"#;
        let requests = Requirement::from_str(input).unwrap();
        assert_eq!(input, requests.to_string());
        let expected = Requirement {
            name: "requests".to_string(),
            extras: Some(vec!["security".to_string(), "tests".to_string()]),
            version_or_url: Some(VersionOrUrl::VersionSpecifier(
                [
                    VersionSpecifier::new(
                        Operator::GreaterThanEqual,
                        Version {
                            epoch: 0,
                            release: vec![2, 8, 1],
                            pre: None,
                            post: None,
                            dev: None,
                            local: None,
                        },
                        false,
                    )
                    .unwrap(),
                    VersionSpecifier::new(
                        Operator::Equal,
                        Version {
                            epoch: 0,
                            release: vec![2, 8],
                            pre: None,
                            post: None,
                            dev: None,
                            local: None,
                        },
                        true,
                    )
                    .unwrap(),
                ]
                .into_iter()
                .collect(),
            )),
            marker: Some(MarkerTree::Expression(MarkerExpression {
                l_value: MarkerValue::MarkerEnvVersion(MarkerValueVersion::PythonVersion),
                operator: MarkerOperator::LessThan,
                r_value: MarkerValue::QuotedString("2.7".to_string()),
            })),
        };
        assert_eq!(requests, expected);
    }

    #[test]
    fn parenthesized_single() {
        let numpy = Requirement::from_str("numpy ( >=1.19 )").unwrap();
        assert_eq!(numpy.name, "numpy");
    }

    #[test]
    fn parenthesized_double() {
        let numpy = Requirement::from_str("numpy ( >=1.19, <2.0 )").unwrap();
        assert_eq!(numpy.name, "numpy");
    }

    #[test]
    fn versions_single() {
        let numpy = Requirement::from_str("numpy >=1.19 ").unwrap();
        assert_eq!(numpy.name, "numpy");
    }

    #[test]
    fn versions_double() {
        let numpy = Requirement::from_str("numpy >=1.19, <2.0 ").unwrap();
        assert_eq!(numpy.name, "numpy");
    }

    #[test]
    fn error_extras_eof1() {
        assert_err(
            "black[",
            indoc! {"
                Missing closing bracket (expected ']', found end of dependency specification)
                black[
                     ^"
            },
        );
    }

    #[test]
    fn error_extras_eof2() {
        assert_err(
            "black[d",
            indoc! {"
                Missing closing bracket (expected ']', found end of dependency specification)
                black[d
                     ^"
            },
        );
    }

    #[test]
    fn error_extras_eof3() {
        assert_err(
            "black[d,",
            indoc! {"
                Missing closing bracket (expected ']', found end of dependency specification)
                black[d,
                     ^"
            },
        );
    }

    #[test]
    fn error_extras_illegal_start1() {
        assert_err(
            "black[ö]",
            indoc! {"
                Expected an alphanumeric character starting the extra name, found 'ö'
                black[ö]
                      ^"
            },
        );
    }

    #[test]
    fn error_extras_illegal_start2() {
        assert_err(
            "black[_d]",
            indoc! {"
                Expected an alphanumeric character starting the extra name, found '_'
                black[_d]
                      ^"
            },
        );
    }
    #[test]
    fn error_extras_illegal_character() {
        assert_err(
            "black[jüpyter]",
            indoc! {"
                Invalid character in extras name, expected an alphanumeric character, '-', '_', '.', ',' or ']', found 'ü'
                black[jüpyter]
                       ^"
            },
        );
    }

    #[test]
    fn error_extras1() {
        let numpy = Requirement::from_str("black[d]").unwrap();
        assert_eq!(numpy.extras, Some(vec!["d".to_string()]));
    }

    #[test]
    fn error_extras2() {
        let numpy = Requirement::from_str("black[d,jupyter]").unwrap();
        assert_eq!(
            numpy.extras,
            Some(vec!["d".to_string(), "jupyter".to_string()])
        );
    }

    #[test]
    fn error_parenthesized_pep440() {
        assert_err(
            "numpy ( ><1.19 )",
            indoc! {"
                Version specifier `><1.19 ` doesn't match PEP 440 rules
                numpy ( ><1.19 )
                        ^^^^^^^"
            },
        );
    }

    #[test]
    fn error_parenthesized_parenthesis() {
        assert_err(
            "numpy ( >=1.19 ",
            indoc! {"
                Missing closing parenthesis (expected ')', found end of dependency specification)
                numpy ( >=1.19 
                      ^"
            },
        );
    }

    #[test]
    fn error_whats_that() {
        assert_err(
            "numpy % 1.16",
            indoc! {"
                Expected one of `@`, `(`, `<`, `=`, `>`, `~`, `!`, `;`, found `%`
                numpy % 1.16
                      ^"
            },
        );
    }

    #[test]
    fn url() {
        let pip_url =
            Requirement::from_str("pip @ https://github.com/pypa/pip/archive/1.3.1.zip#sha1=da9234ee9982d4bbb3c72346a6de940a148ea686")
                .unwrap();
        let url = "https://github.com/pypa/pip/archive/1.3.1.zip#sha1=da9234ee9982d4bbb3c72346a6de940a148ea686";
        let expected = Requirement {
            name: "pip".to_string(),
            extras: None,
            marker: None,
            version_or_url: Some(VersionOrUrl::Url(Url::parse(url).unwrap())),
        };
        assert_eq!(pip_url, expected);
    }

    #[test]
    fn test_marker_parsing() {
        let marker = r#"python_version == "2.7" and (sys_platform == "win32" or (os_name == "linux" and implementation_name == 'cpython'))"#;
        let actual = parse_markers_impl(&mut CharIter::new(marker)).unwrap();
        let expected = MarkerTree::And(vec![
            MarkerTree::Expression(MarkerExpression {
                l_value: MarkerValue::MarkerEnvVersion(MarkerValueVersion::PythonVersion),
                operator: MarkerOperator::Equal,
                r_value: MarkerValue::QuotedString("2.7".to_string()),
            }),
            MarkerTree::Or(vec![
                MarkerTree::Expression(MarkerExpression {
                    l_value: MarkerValue::MarkerEnvString(MarkerValueString::SysPlatform),
                    operator: MarkerOperator::Equal,
                    r_value: MarkerValue::QuotedString("win32".to_string()),
                }),
                MarkerTree::And(vec![
                    MarkerTree::Expression(MarkerExpression {
                        l_value: MarkerValue::MarkerEnvString(MarkerValueString::OsName),
                        operator: MarkerOperator::Equal,
                        r_value: MarkerValue::QuotedString("linux".to_string()),
                    }),
                    MarkerTree::Expression(MarkerExpression {
                        l_value: MarkerValue::MarkerEnvString(
                            MarkerValueString::ImplementationName,
                        ),
                        operator: MarkerOperator::Equal,
                        r_value: MarkerValue::QuotedString("cpython".to_string()),
                    }),
                ]),
            ]),
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn name_and_marker() {
        Requirement::from_str(r#"numpy; sys_platform == "win32" or (os_name == "linux" and implementation_name == 'cpython')"#).unwrap();
    }

    #[test]
    fn error_marker_incomplete1() {
        assert_err(
            r#"numpy; sys_platform"#,
            indoc! {"
                Expected a valid marker operator (such as '>=' or 'not in'), found ''
                numpy; sys_platform
                                   ^"
            },
        );
    }

    #[test]
    fn error_marker_incomplete2() {
        assert_err(
            r#"numpy; sys_platform == "#,
            indoc! {"
                Expected marker value, found end of dependency specification
                numpy; sys_platform == 
                                       ^"
            },
        );
    }

    #[test]
    fn error_marker_incomplete3() {
        assert_err(
            r#"numpy; sys_platform == "win32" or "#,
            indoc! {r#"
                Expected marker value, found end of dependency specification
                numpy; sys_platform == "win32" or 
                                                  ^"#},
        );
    }

    #[test]
    fn error_marker_incomplete4() {
        assert_err(
            r#"numpy; sys_platform == "win32" or (os_name == "linux""#,
            indoc! {r#"
                Expected ')', found end of dependency specification
                numpy; sys_platform == "win32" or (os_name == "linux"
                                                  ^"#},
        );
    }

    #[test]
    fn error_marker_incomplete5() {
        assert_err(
            r#"numpy; sys_platform == "win32" or (os_name == "linux" and "#,
            indoc! {r#"
                Expected marker value, found end of dependency specification
                numpy; sys_platform == "win32" or (os_name == "linux" and 
                                                                          ^"#},
        );
    }

    #[test]
    fn error_pep440() {
        assert_err(
            r#"numpy >=1.1.*"#,
            indoc! {"
                Operator >= must not be used in version ending with a star
                numpy >=1.1.*
                      ^^^^^^^"
            },
        );
    }

    #[test]
    fn error_no_name() {
        assert_err(
            r#"==0.0"#,
            indoc! {"
                Expected package name starting with an alphanumeric character, found '='
                ==0.0
                ^"
            },
        );
    }

    #[test]
    fn error_no_comma_between_extras() {
        assert_err(
            r#"name[bar baz]"#,
            indoc! {"
                Expected either ',' (separating extras) or ']' (ending the extras section), found 'b'
                name[bar baz]
                         ^"
            },
        );
    }

    #[test]
    fn error_extra_comma_after_extras() {
        assert_err(
            r#"name[bar, baz,]"#,
            indoc! {"
                Expected an alphanumeric character starting the extra name, found ']'
                name[bar, baz,]
                              ^"
            },
        );
    }

    #[test]
    fn error_extras_not_closed() {
        assert_err(
            r#"name[bar, baz >= 1.0"#,
            indoc! {"
                Expected either ',' (separating extras) or ']' (ending the extras section), found '>'
                name[bar, baz >= 1.0
                              ^"
            },
        );
    }

    #[test]
    fn error_no_space_after_url() {
        assert_err(
            r#"name @ https://example.com/; extra == 'example'"#,
            indoc! {"
                Expected end of input or ';', found 'e'
                name @ https://example.com/; extra == 'example'
                                             ^"
            },
        );
    }

    #[test]
    fn error_name_at_nothing() {
        assert_err(
            r#"name @ "#,
            indoc! {"
                Expected URL
                name @ 
                       ^"
            },
        );
    }

    #[test]
    fn test_error_invalid_marker_key() {
        assert_err(
            r#"name; invalid_name"#,
            indoc! {"
                Expected a valid marker name, found 'invalid_name'
                name; invalid_name
                      ^^^^^^^^^^^^"
            },
        );
    }

    #[test]
    fn error_markers_invalid_order() {
        assert_err(
            "name; '3.7' <= invalid_name",
            indoc! {"
                Expected a valid marker name, found 'invalid_name'
                name; '3.7' <= invalid_name
                               ^^^^^^^^^^^^"
            },
        );
    }

    #[test]
    fn error_markers_notin() {
        assert_err(
            "name; '3.7' notin python_version",
            indoc! {"
                Expected a valid marker operator (such as '>=' or 'not in'), found 'notin'
                name; '3.7' notin python_version
                            ^^^^^"
            },
        );
    }

    #[test]
    fn error_markers_inpython_version() {
        assert_err(
            "name; '3.6'inpython_version",
            indoc! {"
                Expected a valid marker operator (such as '>=' or 'not in'), found 'inpython_version'
                name; '3.6'inpython_version
                           ^^^^^^^^^^^^^^^^"
            },
        );
    }

    #[test]
    fn error_markers_not_python_version() {
        assert_err(
            "name; '3.7' not python_version",
            indoc! {"
                Expected 'i', found 'p'
                name; '3.7' not python_version
                                ^"
            },
        );
    }

    #[test]
    fn error_markers_invalid_operator() {
        assert_err(
            "name; '3.7' ~ python_version",
            indoc! {"
                Expected a valid marker operator (such as '>=' or 'not in'), found '~'
                name; '3.7' ~ python_version
                            ^"
            },
        );
    }

    #[test]
    fn error_invalid_prerelease() {
        assert_err(
            "name==1.0.org1",
            indoc! {"
                Version specifier `==1.0.org1` doesn't match PEP 440 rules
                name==1.0.org1
                    ^^^^^^^^^^"
            },
        );
    }

    #[test]
    fn error_no_version_value() {
        assert_err(
            "name==",
            indoc! {"
                Version specifier `==` doesn't match PEP 440 rules
                name==
                    ^^"
            },
        );
    }

    #[test]
    fn error_no_version_operator() {
        assert_err(
            "name 1.0",
            indoc! {"
                Expected one of `@`, `(`, `<`, `=`, `>`, `~`, `!`, `;`, found `1`
                name 1.0
                     ^"
            },
        );
    }

    #[test]
    fn error_random_char() {
        assert_err(
            "name >= 1.0 #",
            indoc! {"
                Version specifier `>= 1.0 #` doesn't match PEP 440 rules
                name >= 1.0 #
                     ^^^^^^^^"
            },
        );
    }
}

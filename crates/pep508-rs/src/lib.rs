//! A library for python [dependency specifiers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
//! better known as [PEP 508](https://peps.python.org/pep-0508/)
//!
//! ## Usage
//!
//! ```
//! use std::str::FromStr;
//! use pep508_rs::Requirement;
//! use uv_normalize::ExtraName;
//!
//! let marker = r#"requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8""#;
//! let dependency_specification = Requirement::from_str(marker).unwrap();
//! assert_eq!(dependency_specification.name.as_ref(), "requests");
//! assert_eq!(dependency_specification.extras, vec![ExtraName::from_str("security").unwrap(), ExtraName::from_str("tests").unwrap()]);
//! ```

#![deny(missing_docs)]

#[cfg(feature = "pyo3")]
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
#[cfg(feature = "pyo3")]
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::str::{Chars, FromStr};

#[cfg(feature = "pyo3")]
use pep440_rs::PyVersion;
#[cfg(feature = "pyo3")]
use pyo3::{
    create_exception, exceptions::PyNotImplementedError, pyclass, pyclass::CompareOp, pymethods,
    pymodule, types::PyModule, IntoPy, PyObject, PyResult, Python,
};
#[cfg(feature = "serde")]
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use unicode_width::UnicodeWidthChar;

pub use marker::{
    MarkerEnvironment, MarkerExpression, MarkerOperator, MarkerTree, MarkerValue,
    MarkerValueString, MarkerValueVersion, MarkerWarningKind, StringVersion,
};
use pep440_rs::{Version, VersionSpecifier, VersionSpecifiers};
use uv_fs::normalize_url_path;
#[cfg(feature = "pyo3")]
use uv_normalize::InvalidNameError;
use uv_normalize::{ExtraName, PackageName};
pub use verbatim_url::{split_scheme, VerbatimUrl};

mod marker;
mod verbatim_url;

/// Error with a span attached. Not that those aren't `String` but `Vec<char>` indices.
#[derive(Debug)]
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
#[derive(Debug, Error)]
pub enum Pep508ErrorSource {
    /// An error from our parser.
    #[error("{0}")]
    String(String),
    /// A URL parsing error.
    #[error(transparent)]
    UrlError(#[from] verbatim_url::VerbatimUrlError),
    /// The version requirement is not supported.
    #[error("{0}")]
    UnsupportedRequirement(String),
}

impl Display for Pep508Error {
    /// Pretty formatting with underline.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // We can use char indices here since it's a Vec<char>
        let start_offset = self.input[..self.start]
            .chars()
            .flat_map(|c| c.width())
            .sum::<usize>();
        let underline_len = if self.start == self.input.len() {
            // We also allow 0 here for convenience
            assert!(
                self.len <= 1,
                "Can only go one past the input not {}",
                self.len
            );
            1
        } else {
            self.input[self.start..self.start + self.len]
                .chars()
                .flat_map(|c| c.width())
                .sum::<usize>()
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
#[derive(Hash, Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "pyo3", pyclass(module = "pep508"))]
pub struct Requirement {
    /// The distribution name such as `numpy` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`
    pub name: PackageName,
    /// The list of extras such as `security`, `tests` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`
    pub extras: Vec<ExtraName>,
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
        if !self.extras.is_empty() {
            write!(
                f,
                "[{}]",
                self.extras
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )?;
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
                    write!(f, " @ {url}")?;
                }
            }
        }
        if let Some(marker) = &self.marker {
            write!(f, " ; {marker}")?;
        }
        Ok(())
    }
}

/// <https://github.com/serde-rs/serde/issues/908#issuecomment-298027413>
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

/// <https://github.com/serde-rs/serde/issues/1316#issue-332908452>
#[cfg(feature = "serde")]
impl Serialize for Requirement {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

type MarkerWarning = (MarkerWarningKind, String, String);

#[cfg(feature = "pyo3")]
#[pymethods]
impl Requirement {
    /// The distribution name such as `numpy` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`
    #[getter]
    pub fn name(&self) -> String {
        self.name.to_string()
    }

    /// The list of extras such as `security`, `tests` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`
    #[getter]
    pub fn extras(&self) -> Vec<String> {
        self.extras.iter().map(ToString::to_string).collect()
    }

    /// The marker expression such as  `python_version > "3.8"` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`
    #[getter]
    pub fn marker(&self) -> Option<String> {
        self.marker.as_ref().map(std::string::ToString::to_string)
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
        format!(r#""{self}""#)
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
    #[allow(clippy::needless_pass_by_value)]
    #[pyo3(name = "evaluate_markers")]
    pub fn py_evaluate_markers(
        &self,
        env: &MarkerEnvironment,
        extras: Vec<String>,
    ) -> PyResult<bool> {
        let extras = extras
            .into_iter()
            .map(|extra| ExtraName::from_str(&extra))
            .collect::<Result<Vec<_>, InvalidNameError>>()
            .map_err(|err| PyPep508Error::new_err(err.to_string()))?;

        Ok(self.evaluate_markers(env, &extras))
    }

    /// Returns whether the requirement would be satisfied, independent of environment markers, i.e.
    /// if there is potentially an environment that could activate this requirement.
    ///
    /// Note that unlike [Self::evaluate_markers] this does not perform any checks for bogus
    /// expressions but will simply return true. As caller you should separately perform a check
    /// with an environment and forward all warnings.
    #[allow(clippy::needless_pass_by_value)]
    #[pyo3(name = "evaluate_extras_and_python_version")]
    pub fn py_evaluate_extras_and_python_version(
        &self,
        extras: HashSet<String>,
        python_versions: Vec<PyVersion>,
    ) -> PyResult<bool> {
        let extras = extras
            .into_iter()
            .map(|extra| ExtraName::from_str(&extra))
            .collect::<Result<HashSet<_>, InvalidNameError>>()
            .map_err(|err| PyPep508Error::new_err(err.to_string()))?;

        let python_versions = python_versions
            .into_iter()
            .map(|py_version| py_version.0)
            .collect::<Vec<_>>();

        Ok(self.evaluate_extras_and_python_version(&extras, &python_versions))
    }

    /// Returns whether the markers apply for the given environment
    #[allow(clippy::needless_pass_by_value)]
    #[pyo3(name = "evaluate_markers_and_report")]
    pub fn py_evaluate_markers_and_report(
        &self,
        env: &MarkerEnvironment,
        extras: Vec<String>,
    ) -> PyResult<(bool, Vec<MarkerWarning>)> {
        let extras = extras
            .into_iter()
            .map(|extra| ExtraName::from_str(&extra))
            .collect::<Result<Vec<_>, InvalidNameError>>()
            .map_err(|err| PyPep508Error::new_err(err.to_string()))?;

        Ok(self.evaluate_markers_and_report(env, &extras))
    }
}

impl Requirement {
    /// Returns `true` if the [`Version`] satisfies the [`Requirement`].
    pub fn is_satisfied_by(&self, version: &Version) -> bool {
        let Some(version_or_url) = self.version_or_url.as_ref() else {
            return true;
        };

        let specifiers = match version_or_url {
            VersionOrUrl::VersionSpecifier(specifiers) => specifiers,
            // TODO(charlie): Support URL dependencies.
            VersionOrUrl::Url(_) => return false,
        };

        specifiers
            .iter()
            .all(|specifier| specifier.contains(version))
    }

    /// Returns whether the markers apply for the given environment
    pub fn evaluate_markers(&self, env: &MarkerEnvironment, extras: &[ExtraName]) -> bool {
        if let Some(marker) = &self.marker {
            marker.evaluate(env, extras)
        } else {
            true
        }
    }

    /// Returns whether the requirement would be satisfied, independent of environment markers, i.e.
    /// if there is potentially an environment that could activate this requirement.
    ///
    /// Note that unlike [`Self::evaluate_markers`] this does not perform any checks for bogus
    /// expressions but will simply return true. As caller you should separately perform a check
    /// with an environment and forward all warnings.
    pub fn evaluate_extras_and_python_version(
        &self,
        extras: &HashSet<ExtraName>,
        python_versions: &[Version],
    ) -> bool {
        if let Some(marker) = &self.marker {
            marker.evaluate_extras_and_python_version(extras, python_versions)
        } else {
            true
        }
    }

    /// Returns whether the markers apply for the given environment
    pub fn evaluate_markers_and_report(
        &self,
        env: &MarkerEnvironment,
        extras: &[ExtraName],
    ) -> (bool, Vec<MarkerWarning>) {
        if let Some(marker) = &self.marker {
            marker.evaluate_collect_warnings(env, extras)
        } else {
            (true, Vec::new())
        }
    }
}

impl FromStr for Requirement {
    type Err = Pep508Error;

    /// Parse a [Dependency Specifier](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parse(&mut Cursor::new(input), None)
    }
}

impl Requirement {
    /// Parse a [Dependency Specifier](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
    pub fn parse(input: &str, working_dir: impl AsRef<Path>) -> Result<Self, Pep508Error> {
        parse(&mut Cursor::new(input), Some(working_dir.as_ref()))
    }
}

/// A list of [`ExtraName`] that can be attached to a [`Requirement`].
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct Extras(Vec<ExtraName>);

impl Extras {
    /// Parse a list of extras.
    pub fn parse(input: &str) -> Result<Self, Pep508Error> {
        Ok(Self(parse_extras(&mut Cursor::new(input))?))
    }

    /// Convert the [`Extras`] into a [`Vec`] of [`ExtraName`].
    pub fn into_vec(self) -> Vec<ExtraName> {
        self.0
    }
}

/// The actual version specifier or url to install
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub enum VersionOrUrl {
    /// A PEP 440 version specifier set
    VersionSpecifier(VersionSpecifiers),
    /// A installable URL
    Url(VerbatimUrl),
}

/// A [`Cursor`] over a string.
#[derive(Debug, Clone)]
pub struct Cursor<'a> {
    input: &'a str,
    chars: Chars<'a>,
    pos: usize,
}

impl<'a> Cursor<'a> {
    /// Convert from `&str`.
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.chars(),
            pos: 0,
        }
    }

    /// Returns a new cursor starting at the given position.
    pub fn at(self, pos: usize) -> Self {
        Self {
            input: self.input,
            chars: self.input[pos..].chars(),
            pos,
        }
    }

    /// Returns the current byte position of the cursor.
    fn pos(&self) -> usize {
        self.pos
    }

    /// Returns a slice over the input string.
    fn slice(&self, start: usize, len: usize) -> &str {
        &self.input[start..start + len]
    }

    /// Peeks the next character and position from the input stream without consuming it.
    fn peek(&self) -> Option<(usize, char)> {
        self.chars.clone().next().map(|char| (self.pos, char))
    }

    /// Peeks the next character from the input stream without consuming it.
    fn peek_char(&self) -> Option<char> {
        self.chars.clone().next()
    }

    /// Eats the next character from the input stream if it matches the given token.
    fn eat_char(&mut self, token: char) -> Option<usize> {
        let (start_pos, peek_char) = self.peek()?;
        if peek_char == token {
            self.next();
            Some(start_pos)
        } else {
            None
        }
    }

    /// Consumes whitespace from the cursor.
    fn eat_whitespace(&mut self) {
        while let Some(char) = self.peek_char() {
            if char.is_whitespace() {
                self.next();
            } else {
                return;
            }
        }
    }

    /// Returns the next character and position from the input stream and consumes it.
    fn next(&mut self) -> Option<(usize, char)> {
        let pos = self.pos;
        let char = self.chars.next()?;
        self.pos += char.len_utf8();
        Some((pos, char))
    }

    /// Peeks over the cursor as long as the condition is met, without consuming it.
    fn peek_while(&mut self, condition: impl Fn(char) -> bool) -> (usize, usize) {
        let peeker = self.chars.clone();
        let start = self.pos();
        let len = peeker.take_while(|c| condition(*c)).count();
        (start, len)
    }

    /// Consumes characters from the cursor as long as the condition is met.
    fn take_while(&mut self, condition: impl Fn(char) -> bool) -> (usize, usize) {
        let start = self.pos();
        let mut len = 0;
        while let Some(char) = self.peek_char() {
            if !condition(char) {
                break;
            }

            self.next();
            len += char.len_utf8();
        }
        (start, len)
    }

    /// Consumes characters from the cursor, raising an error if it doesn't match the given token.
    fn next_expect_char(&mut self, expected: char, span_start: usize) -> Result<(), Pep508Error> {
        match self.next() {
            None => Err(Pep508Error {
                message: Pep508ErrorSource::String(format!(
                    "Expected '{expected}', found end of dependency specification"
                )),
                start: span_start,
                len: 1,
                input: self.to_string(),
            }),
            Some((_, value)) if value == expected => Ok(()),
            Some((pos, other)) => Err(Pep508Error {
                message: Pep508ErrorSource::String(format!(
                    "Expected '{expected}', found '{other}'"
                )),
                start: pos,
                len: other.len_utf8(),
                input: self.to_string(),
            }),
        }
    }
}

impl Display for Cursor<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.input)
    }
}

fn parse_name(cursor: &mut Cursor) -> Result<PackageName, Pep508Error> {
    // https://peps.python.org/pep-0508/#names
    // ^([A-Z0-9]|[A-Z0-9][A-Z0-9._-]*[A-Z0-9])$ with re.IGNORECASE
    let mut name = String::new();
    if let Some((index, char)) = cursor.next() {
        if matches!(char, 'A'..='Z' | 'a'..='z' | '0'..='9') {
            name.push(char);
        } else {
            return Err(Pep508Error {
                message: Pep508ErrorSource::String(format!(
                    "Expected package name starting with an alphanumeric character, found '{char}'"
                )),
                start: index,
                len: char.len_utf8(),
                input: cursor.to_string(),
            });
        }
    } else {
        return Err(Pep508Error {
            message: Pep508ErrorSource::String("Empty field is not allowed for PEP508".to_string()),
            start: 0,
            len: 1,
            input: cursor.to_string(),
        });
    }

    loop {
        match cursor.peek() {
            Some((index, char @ ('A'..='Z' | 'a'..='z' | '0'..='9' | '.' | '-' | '_'))) => {
                name.push(char);
                cursor.next();
                // [.-_] can't be the final character
                if cursor.peek().is_none() && matches!(char, '.' | '-' | '_') {
                    return Err(Pep508Error {
                        message: Pep508ErrorSource::String(format!(
                            "Package name must end with an alphanumeric character, not '{char}'"
                        )),
                        start: index,
                        len: char.len_utf8(),
                        input: cursor.to_string(),
                    });
                }
            }
            Some(_) | None => {
                return Ok(PackageName::new(name)
                    .expect("`PackageName` validation should match PEP 508 parsing"));
            }
        }
    }
}

/// parses extras in the `[extra1,extra2] format`
fn parse_extras(cursor: &mut Cursor) -> Result<Vec<ExtraName>, Pep508Error> {
    let Some(bracket_pos) = cursor.eat_char('[') else {
        return Ok(vec![]);
    };
    let mut extras = Vec::new();

    loop {
        // wsp* before the identifier
        cursor.eat_whitespace();
        let mut buffer = String::new();
        let early_eof_error = Pep508Error {
            message: Pep508ErrorSource::String(
                "Missing closing bracket (expected ']', found end of dependency specification)"
                    .to_string(),
            ),
            start: bracket_pos,
            len: 1,
            input: cursor.to_string(),
        };

        // First char of the identifier
        match cursor.next() {
            // letterOrDigit
            Some((_, alphanumeric @ ('a'..='z' | 'A'..='Z' | '0'..='9'))) => {
                buffer.push(alphanumeric);
            }
            Some((pos, other)) => {
                return Err(Pep508Error {
                    message: Pep508ErrorSource::String(format!(
                        "Expected an alphanumeric character starting the extra name, found '{other}'"
                    )),
                    start: pos,
                    len: other.len_utf8(),
                    input: cursor.to_string(),
                });
            }
            None => return Err(early_eof_error),
        }
        // Parse from the second char of the identifier
        // We handle the illegal character case below
        // identifier_end = letterOrDigit | (('-' | '_' | '.' )* letterOrDigit)
        // identifier_end*
        let (start, len) = cursor
            .take_while(|char| matches!(char, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.'));
        buffer.push_str(cursor.slice(start, len));
        match cursor.peek() {
            Some((pos, char)) if char != ',' && char != ']' && !char.is_whitespace() => {
                return Err(Pep508Error {
                    message: Pep508ErrorSource::String(format!(
                        "Invalid character in extras name, expected an alphanumeric character, '-', '_', '.', ',' or ']', found '{char}'"
                    )),
                    start: pos,
                    len: char.len_utf8(),
                    input: cursor.to_string(),
                });
            }
            _ => {}
        };
        // wsp* after the identifier
        cursor.eat_whitespace();
        // end or next identifier?
        match cursor.next() {
            Some((_, ',')) => {
                extras.push(
                    ExtraName::new(buffer)
                        .expect("`ExtraName` validation should match PEP 508 parsing"),
                );
            }
            Some((_, ']')) => {
                extras.push(
                    ExtraName::new(buffer)
                        .expect("`ExtraName` validation should match PEP 508 parsing"),
                );
                break;
            }
            Some((pos, other)) => {
                return Err(Pep508Error {
                    message: Pep508ErrorSource::String(format!(
                        "Expected either ',' (separating extras) or ']' (ending the extras section), found '{other}'"
                    )),
                    start: pos,
                    len: other.len_utf8(),
                    input: cursor.to_string(),
                });
            }
            None => return Err(early_eof_error),
        }
    }

    Ok(extras)
}

/// Parse a raw string for a URL requirement, which could be either a URL or a local path, and which
/// could contain unexpanded environment variables.
///
/// For example:
/// - `https://pypi.org/project/requests/...`
/// - `file:///home/ferris/project/scripts/...`
/// - `file:../editable/`
/// - `../editable/`
/// - `https://download.pytorch.org/whl/torch_stable.html`
fn parse_url(cursor: &mut Cursor, working_dir: Option<&Path>) -> Result<VerbatimUrl, Pep508Error> {
    // wsp*
    cursor.eat_whitespace();
    // <URI_reference>
    let (start, len) = cursor.take_while(|char| !char.is_whitespace());
    let url = cursor.slice(start, len);
    if url.is_empty() {
        return Err(Pep508Error {
            message: Pep508ErrorSource::String("Expected URL".to_string()),
            start,
            len,
            input: cursor.to_string(),
        });
    }

    let url = preprocess_url(url, working_dir, cursor, start, len)?;

    Ok(url)
}

/// Create a `VerbatimUrl` to represent the requirement.
fn preprocess_url(
    url: &str,
    working_dir: Option<&Path>,
    cursor: &Cursor,
    start: usize,
    len: usize,
) -> Result<VerbatimUrl, Pep508Error> {
    let url = if let Some((scheme, path)) = split_scheme(url) {
        if scheme == "file" {
            // Ex) `file:///home/ferris/project/scripts/...` or `file:../editable/`.
            let path = path.strip_prefix("//").unwrap_or(path);

            // Transform, e.g., `/C:/Users/ferris/wheel-0.42.0.tar.gz` to `C:\Users\ferris\wheel-0.42.0.tar.gz`.
            let path = normalize_url_path(path);

            if let Some(working_dir) = working_dir {
                VerbatimUrl::from_path(path, working_dir).with_given(url.to_string())
            } else {
                VerbatimUrl::from_absolute_path(path)
                    .map_err(|err| Pep508Error {
                        message: Pep508ErrorSource::UrlError(err),
                        start,
                        len,
                        input: cursor.to_string(),
                    })?
                    .with_given(url.to_string())
            }
        } else {
            // Ex) `https://download.pytorch.org/whl/torch_stable.html`
            VerbatimUrl::from_str(url).map_err(|err| Pep508Error {
                message: Pep508ErrorSource::UrlError(err),
                start,
                len,
                input: cursor.to_string(),
            })?
        }
    } else {
        // Ex) `../editable/`
        if let Some(working_dir) = working_dir {
            VerbatimUrl::from_path(url, working_dir).with_given(url.to_string())
        } else {
            VerbatimUrl::from_absolute_path(url)
                .map_err(|err| Pep508Error {
                    message: Pep508ErrorSource::UrlError(err),
                    start,
                    len,
                    input: cursor.to_string(),
                })?
                .with_given(url.to_string())
        }
    };
    Ok(url)
}

/// PEP 440 wrapper
fn parse_specifier(
    cursor: &mut Cursor,
    buffer: &str,
    start: usize,
    end: usize,
) -> Result<VersionSpecifier, Pep508Error> {
    VersionSpecifier::from_str(buffer).map_err(|err| Pep508Error {
        message: Pep508ErrorSource::String(err.to_string()),
        start,
        len: end - start,
        input: cursor.to_string(),
    })
}

/// Such as `>=1.19,<2.0`, either delimited by the end of the specifier or a `;` for the marker part
///
/// ```text
/// version_one (wsp* ',' version_one)*
/// ```
fn parse_version_specifier(cursor: &mut Cursor) -> Result<Option<VersionOrUrl>, Pep508Error> {
    let mut start = cursor.pos();
    let mut specifiers = Vec::new();
    let mut buffer = String::new();
    let requirement_kind = loop {
        match cursor.peek() {
            Some((end, ',')) => {
                let specifier = parse_specifier(cursor, &buffer, start, end)?;
                specifiers.push(specifier);
                buffer.clear();
                cursor.next();
                start = end + 1;
            }
            Some((_, ';')) | None => {
                let end = cursor.pos();
                let specifier = parse_specifier(cursor, &buffer, start, end)?;
                specifiers.push(specifier);
                break Some(VersionOrUrl::VersionSpecifier(
                    specifiers.into_iter().collect(),
                ));
            }
            Some((_, char)) => {
                buffer.push(char);
                cursor.next();
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
    cursor: &mut Cursor,
) -> Result<Option<VersionOrUrl>, Pep508Error> {
    let brace_pos = cursor.pos();
    cursor.next();
    // Makes for slightly better error underline
    cursor.eat_whitespace();
    let mut start = cursor.pos();
    let mut specifiers = Vec::new();
    let mut buffer = String::new();
    let requirement_kind = loop {
        match cursor.next() {
            Some((end, ',')) => {
                let specifier =
                    parse_specifier(cursor, &buffer, start, end)?;
                specifiers.push(specifier);
                buffer.clear();
                start = end + 1;
            }
            Some((end, ')')) => {
                let specifier = parse_specifier(cursor, &buffer, start, end)?;
                specifiers.push(specifier);
                break Some(VersionOrUrl::VersionSpecifier(specifiers.into_iter().collect()));
            }
            Some((_, char)) => buffer.push(char),
            None => return Err(Pep508Error {
                message: Pep508ErrorSource::String("Missing closing parenthesis (expected ')', found end of dependency specification)".to_string()),
                start: brace_pos,
                len: 1,
                input: cursor.to_string(),
            }),
        }
    };
    Ok(requirement_kind)
}

/// Parse a [dependency specifier](https://packaging.python.org/en/latest/specifications/dependency-specifiers)
fn parse(cursor: &mut Cursor, working_dir: Option<&Path>) -> Result<Requirement, Pep508Error> {
    let start = cursor.pos();

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
    cursor.eat_whitespace();
    // name
    let name = parse_name(cursor)?;
    // wsp*
    cursor.eat_whitespace();
    // extras?
    let extras = parse_extras(cursor)?;
    // wsp*
    cursor.eat_whitespace();

    // ( url_req | name_req )?
    let requirement_kind = match cursor.peek_char() {
        Some('@') => {
            cursor.next();
            Some(VersionOrUrl::Url(parse_url(cursor, working_dir)?))
        }
        Some('(') => parse_version_specifier_parentheses(cursor)?,
        Some('<' | '=' | '>' | '~' | '!') => parse_version_specifier(cursor)?,
        Some(';') | None => None,
        Some(other) => {
            // Rewind to the start of the version specifier, to see if the user added a URL without
            // a package name. pip supports this in `requirements.txt`, but it doesn't adhere to
            // the PEP 508 grammar.
            let mut clone = cursor.clone().at(start);
            return if parse_url(&mut clone, working_dir).is_ok() {
                Err(Pep508Error {
                    message: Pep508ErrorSource::UnsupportedRequirement("URL requirement must be preceded by a package name. Add the name of the package before the URL (e.g., `package_name @ https://...`).".to_string()),
                    start,
                    len: clone.pos() - start,
                    input: clone.to_string(),
                })
            } else {
                Err(Pep508Error {
                    message: Pep508ErrorSource::String(format!(
                        "Expected one of `@`, `(`, `<`, `=`, `>`, `~`, `!`, `;`, found `{other}`"
                    )),
                    start: cursor.pos(),
                    len: other.len_utf8(),
                    input: cursor.to_string(),
                })
            };
        }
    };

    // wsp*
    cursor.eat_whitespace();
    // quoted_marker?
    let marker = if cursor.peek_char() == Some(';') {
        // Skip past the semicolon
        cursor.next();
        Some(marker::parse_markers_impl(cursor)?)
    } else {
        None
    };
    // wsp*
    cursor.eat_whitespace();
    if let Some((pos, char)) = cursor.next() {
        return Err(Pep508Error {
            message: Pep508ErrorSource::String(if marker.is_none() {
                format!(r#"Expected end of input or ';', found '{char}'"#)
            } else {
                format!(r#"Expected end of input, found '{char}'"#)
            }),
            start: pos,
            len: char.len_utf8(),
            input: cursor.to_string(),
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

/// Half of these tests are copied from <https://github.com/pypa/packaging/pull/624>
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use indoc::indoc;

    use pep440_rs::{Operator, Version, VersionPattern, VersionSpecifier};
    use uv_normalize::{ExtraName, PackageName};

    use crate::marker::{
        parse_markers_impl, MarkerExpression, MarkerOperator, MarkerTree, MarkerValue,
        MarkerValueString, MarkerValueVersion,
    };
    use crate::{Cursor, Requirement, VerbatimUrl, VersionOrUrl};

    fn assert_err(input: &str, error: &str) {
        assert_eq!(Requirement::from_str(input).unwrap_err().to_string(), error);
    }

    #[cfg(windows)]
    #[test]
    fn test_preprocess_url_windows() {
        use std::path::PathBuf;

        let actual = crate::preprocess_url(
            "file:///C:/Users/ferris/wheel-0.42.0.tar.gz",
            None,
            &Cursor::new(""),
            0,
            0,
        )
        .unwrap()
        .to_file_path();
        let expected = PathBuf::from(r"C:\Users\ferris\wheel-0.42.0.tar.gz");
        assert_eq!(actual, Ok(expected));
    }

    #[test]
    fn error_empty() {
        assert_err(
            "",
            indoc! {"\
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
            name: PackageName::from_str("requests").unwrap(),
            extras: vec![
                ExtraName::from_str("security").unwrap(),
                ExtraName::from_str("tests").unwrap(),
            ],
            version_or_url: Some(VersionOrUrl::VersionSpecifier(
                [
                    VersionSpecifier::new(
                        Operator::GreaterThanEqual,
                        VersionPattern::verbatim(Version::new([2, 8, 1])),
                    )
                    .unwrap(),
                    VersionSpecifier::new(
                        Operator::Equal,
                        VersionPattern::wildcard(Version::new([2, 8])),
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
        assert_eq!(numpy.name.as_ref(), "numpy");
    }

    #[test]
    fn parenthesized_double() {
        let numpy = Requirement::from_str("numpy ( >=1.19, <2.0 )").unwrap();
        assert_eq!(numpy.name.as_ref(), "numpy");
    }

    #[test]
    fn versions_single() {
        let numpy = Requirement::from_str("numpy >=1.19 ").unwrap();
        assert_eq!(numpy.name.as_ref(), "numpy");
    }

    #[test]
    fn versions_double() {
        let numpy = Requirement::from_str("numpy >=1.19, <2.0 ").unwrap();
        assert_eq!(numpy.name.as_ref(), "numpy");
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
        assert_eq!(numpy.extras, vec![ExtraName::from_str("d").unwrap()]);
    }

    #[test]
    fn error_extras2() {
        let numpy = Requirement::from_str("black[d,jupyter]").unwrap();
        assert_eq!(
            numpy.extras,
            vec![
                ExtraName::from_str("d").unwrap(),
                ExtraName::from_str("jupyter").unwrap(),
            ]
        );
    }

    #[test]
    fn error_parenthesized_pep440() {
        assert_err(
            "numpy ( ><1.19 )",
            indoc! {"
                no such comparison operator \"><\", must be one of ~= == != <= >= < > ===
                numpy ( ><1.19 )
                        ^^^^^^^"
            },
        );
    }

    #[test]
    fn error_parenthesized_parenthesis() {
        assert_err(
            "numpy ( >=1.19",
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
            name: PackageName::from_str("pip").unwrap(),
            extras: vec![],
            marker: None,
            version_or_url: Some(VersionOrUrl::Url(VerbatimUrl::from_str(url).unwrap())),
        };
        assert_eq!(pip_url, expected);
    }

    #[test]
    fn test_marker_parsing() {
        let marker = r#"python_version == "2.7" and (sys_platform == "win32" or (os_name == "linux" and implementation_name == 'cpython'))"#;
        let actual = parse_markers_impl(&mut Cursor::new(marker)).unwrap();
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
            r#"numpy; sys_platform =="#,
            indoc! {"\
                Expected marker value, found end of dependency specification
                numpy; sys_platform ==
                                      ^"
            },
        );
    }

    #[test]
    fn error_marker_incomplete3() {
        assert_err(
            r#"numpy; sys_platform == "win32" or"#,
            indoc! {"
                Expected marker value, found end of dependency specification
                numpy; sys_platform == \"win32\" or
                                                 ^"},
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
            r#"numpy; sys_platform == "win32" or (os_name == "linux" and"#,
            indoc! {"
                Expected marker value, found end of dependency specification
                numpy; sys_platform == \"win32\" or (os_name == \"linux\" and
                                                                         ^"},
        );
    }

    #[test]
    fn error_pep440() {
        assert_err(
            r#"numpy >=1.1.*"#,
            indoc! {"
                Operator >= cannot be used with a wildcard version specifier
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
    fn error_bare_url() {
        assert_err(
            r#"git+https://github.com/pallets/flask.git"#,
            indoc! {"
                URL requirement must be preceded by a package name. Add the name of the package before the URL (e.g., `package_name @ https://...`).
                git+https://github.com/pallets/flask.git
                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^"
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
            r#"name @"#,
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
                after parsing 1.0, found \".org1\" after it, which is not part of a valid version
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
                Unexpected end of version specifier, expected version
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
                Trailing `#` is not allowed
                name >= 1.0 #
                     ^^^^^^^^"
            },
        );
    }
}

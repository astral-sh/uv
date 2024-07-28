//! A library for python [dependency specifiers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
//! better known as [PEP 508](https://peps.python.org/pep-0508/)
//!
//! ## Usage
//!
//! ```
//! use std::str::FromStr;
//! use pep508_rs::{Requirement, VerbatimUrl};
//! use uv_normalize::ExtraName;
//!
//! let marker = r#"requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8""#;
//! let dependency_specification = Requirement::<VerbatimUrl>::from_str(marker).unwrap();
//! assert_eq!(dependency_specification.name.as_ref(), "requests");
//! assert_eq!(dependency_specification.extras, vec![ExtraName::from_str("security").unwrap(), ExtraName::from_str("tests").unwrap()]);
//! ```

#![warn(missing_docs)]

#[cfg(feature = "pyo3")]
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
#[cfg(feature = "pyo3")]
use std::hash::{Hash, Hasher};
#[cfg(feature = "pyo3")]
use std::ops::Deref;
use std::path::Path;
use std::str::FromStr;

#[cfg(feature = "pyo3")]
use pyo3::{
    create_exception, exceptions::PyNotImplementedError, pyclass, pyclass::CompareOp, pymethods,
    pymodule, types::PyModule, IntoPy, PyObject, PyResult, Python,
};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use url::Url;

use cursor::Cursor;
pub use marker::{
    ExtraOperator, MarkerEnvironment, MarkerEnvironmentBuilder, MarkerExpression, MarkerOperator,
    MarkerTree, MarkerValue, MarkerValueString, MarkerValueVersion, MarkerWarningKind,
    StringVersion,
};
pub use origin::RequirementOrigin;
#[cfg(feature = "pyo3")]
use pep440_rs::PyVersion;
use pep440_rs::{Version, VersionSpecifier, VersionSpecifiers};
#[cfg(feature = "non-pep508-extensions")]
pub use unnamed::{UnnamedRequirement, UnnamedRequirementUrl};
pub use uv_normalize::{ExtraName, InvalidNameError, PackageName};
pub use verbatim_url::{
    expand_env_vars, split_scheme, strip_host, Scheme, VerbatimUrl, VerbatimUrlError,
};

mod cursor;
mod marker;
mod origin;
#[cfg(feature = "non-pep508-extensions")]
mod unnamed;
mod verbatim_url;

/// Error with a span attached. Not that those aren't `String` but `Vec<char>` indices.
#[derive(Debug)]
pub struct Pep508Error<T: Pep508Url = VerbatimUrl> {
    /// Either we have an error string from our parser or an upstream error from `url`
    pub message: Pep508ErrorSource<T>,
    /// Span start index
    pub start: usize,
    /// Span length
    pub len: usize,
    /// The input string so we can print it underlined
    pub input: String,
}

/// Either we have an error string from our parser or an upstream error from `url`
#[derive(Debug, Error)]
pub enum Pep508ErrorSource<T: Pep508Url = VerbatimUrl> {
    /// An error from our parser.
    #[error("{0}")]
    String(String),
    /// A URL parsing error.
    #[error(transparent)]
    UrlError(T::Err),
    /// The version requirement is not supported.
    #[error("{0}")]
    UnsupportedRequirement(String),
}

impl<T: Pep508Url> Display for Pep508Error<T> {
    /// Pretty formatting with underline.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // We can use char indices here since it's a Vec<char>
        let start_offset = self.input[..self.start]
            .chars()
            .filter_map(unicode_width::UnicodeWidthChar::width)
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
                .filter_map(unicode_width::UnicodeWidthChar::width)
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

/// We need this to allow anyhow's `.context()` and `AsDynError`.
impl<E: Error + Debug, T: Pep508Url<Err = E>> std::error::Error for Pep508Error<T> {}

#[cfg(feature = "pyo3")]
create_exception!(
    pep508,
    PyPep508Error,
    pyo3::exceptions::PyValueError,
    "A PEP 508 parser error with span information"
);

/// A PEP 508 dependency specifier.
#[derive(Hash, Debug, Clone, Eq, PartialEq)]
pub struct Requirement<T: Pep508Url = VerbatimUrl> {
    /// The distribution name such as `requests` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`.
    pub name: PackageName,
    /// The list of extras such as `security`, `tests` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`.
    pub extras: Vec<ExtraName>,
    /// The version specifier such as `>= 2.8.1`, `== 2.8.*` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`.
    /// or a URL.
    pub version_or_url: Option<VersionOrUrl<T>>,
    /// The markers such as `python_version > "3.8"` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`.
    /// Those are a nested and/or tree.
    pub marker: Option<MarkerTree>,
    /// The source file containing the requirement.
    pub origin: Option<RequirementOrigin>,
}

impl<T: Pep508Url> Requirement<T> {
    /// Removes the URL specifier from this requirement.
    pub fn clear_url(&mut self) {
        if matches!(self.version_or_url, Some(VersionOrUrl::Url(_))) {
            self.version_or_url = None;
        }
    }
}

impl<T: Pep508Url + Display> Display for Requirement<T> {
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
                    write!(f, "{}", version_specifier.join(","))?;
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
impl<'de, T: Pep508Url> Deserialize<'de> for Requirement<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        FromStr::from_str(&s).map_err(de::Error::custom)
    }
}

/// <https://github.com/serde-rs/serde/issues/1316#issue-332908452>
impl<T: Pep508Url> Serialize for Requirement<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

type MarkerWarning = (MarkerWarningKind, String);

#[cfg(feature = "pyo3")]
#[pyclass(module = "pep508", name = "Requirement")]
#[derive(Hash, Debug, Clone, Eq, PartialEq)]
/// A PEP 508 dependency specifier.
pub struct PyRequirement(Requirement);

#[cfg(feature = "pyo3")]
impl Deref for PyRequirement {
    type Target = Requirement;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl PyRequirement {
    /// The distribution name such as `requests` in
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
        self.marker.as_ref().map(ToString::to_string)
    }

    /// Parses a PEP 440 string
    #[new]
    pub fn py_new(requirement: &str) -> PyResult<Self> {
        Ok(Self(
            Requirement::from_str(requirement)
                .map_err(|err| PyPep508Error::new_err(err.to_string()))?,
        ))
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
        self.to_string()
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

impl<T: Pep508Url> Requirement<T> {
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

    /// Returns whether the markers apply for the given environment.
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

    /// Return the requirement with an additional marker added, to require the given extra.
    ///
    /// For example, given `flask >= 2.0.2`, calling `with_extra_marker("dotenv")` would return
    /// `flask >= 2.0.2 ; extra == "dotenv"`.
    #[must_use]
    pub fn with_extra_marker(self, extra: &ExtraName) -> Self {
        let marker = match self.marker {
            Some(expression) => MarkerTree::And(vec![
                expression,
                MarkerTree::Expression(MarkerExpression::Extra {
                    operator: ExtraOperator::Equal,
                    name: extra.clone(),
                }),
            ]),
            None => MarkerTree::Expression(MarkerExpression::Extra {
                operator: ExtraOperator::Equal,
                name: extra.clone(),
            }),
        };
        Self {
            marker: Some(marker),
            ..self
        }
    }

    /// Set the source file containing the requirement.
    #[must_use]
    pub fn with_origin(self, origin: RequirementOrigin) -> Self {
        Self {
            origin: Some(origin),
            ..self
        }
    }
}

/// Type to parse URLs from `name @ <url>` into. Defaults to [`url::Url`].
pub trait Pep508Url: Display + Debug + Sized {
    /// String to URL parsing error
    type Err: Error + Debug;

    /// Parse a url from `name @ <url>`. Defaults to [`url::Url::parse_url`].
    fn parse_url(url: &str, working_dir: Option<&Path>) -> Result<Self, Self::Err>;
}

impl Pep508Url for Url {
    type Err = url::ParseError;

    fn parse_url(url: &str, _working_dir: Option<&Path>) -> Result<Self, Self::Err> {
        Url::parse(url)
    }
}

/// A reporter for warnings that occur during marker parsing or evaluation.
pub trait Reporter {
    /// Report a warning.
    fn report(&mut self, kind: MarkerWarningKind, warning: String);
}

impl<F> Reporter for F
where
    F: FnMut(MarkerWarningKind, String),
{
    fn report(&mut self, kind: MarkerWarningKind, warning: String) {
        (self)(kind, warning);
    }
}

/// A simple [`Reporter`] that logs to tracing when the `tracing` feature is enabled.
pub struct TracingReporter;

impl Reporter for TracingReporter {
    #[allow(unused_variables)]
    fn report(&mut self, _kind: MarkerWarningKind, message: String) {
        #[cfg(feature = "tracing")]
        {
            tracing::warn!("{message}");
        }
    }
}

#[cfg(feature = "schemars")]
impl<T: Pep508Url> schemars::JsonSchema for Requirement<T> {
    fn schema_name() -> String {
        "Requirement".to_string()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            metadata: Some(Box::new(schemars::schema::Metadata {
                description: Some("A PEP 508 dependency specifier".to_string()),
                ..schemars::schema::Metadata::default()
            })),
            ..schemars::schema::SchemaObject::default()
        }
        .into()
    }
}

impl<T: Pep508Url> FromStr for Requirement<T> {
    type Err = Pep508Error<T>;

    /// Parse a [Dependency Specifier](https://packaging.python.org/en/latest/specifications/dependency-specifiers/).
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parse_pep508_requirement::<T>(&mut Cursor::new(input), None, &mut TracingReporter)
    }
}

impl<T: Pep508Url> Requirement<T> {
    /// Parse a [Dependency Specifier](https://packaging.python.org/en/latest/specifications/dependency-specifiers/).
    pub fn parse(input: &str, working_dir: impl AsRef<Path>) -> Result<Self, Pep508Error<T>> {
        parse_pep508_requirement(
            &mut Cursor::new(input),
            Some(working_dir.as_ref()),
            &mut TracingReporter,
        )
    }

    /// Parse a [Dependency Specifier](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
    /// with the given reporter for warnings.
    pub fn parse_reporter(
        input: &str,
        working_dir: impl AsRef<Path>,
        reporter: &mut impl Reporter,
    ) -> Result<Self, Pep508Error<T>> {
        parse_pep508_requirement(
            &mut Cursor::new(input),
            Some(working_dir.as_ref()),
            reporter,
        )
    }
}

/// A list of [`ExtraName`] that can be attached to a [`Requirement`].
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct Extras(Vec<ExtraName>);

impl Extras {
    /// Parse a list of extras.
    pub fn parse<T: Pep508Url>(input: &str) -> Result<Self, Pep508Error<T>> {
        Ok(Self(parse_extras_cursor(&mut Cursor::new(input))?))
    }
}

/// The actual version specifier or URL to install.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub enum VersionOrUrl<T: Pep508Url = VerbatimUrl> {
    /// A PEP 440 version specifier set
    VersionSpecifier(VersionSpecifiers),
    /// A installable URL
    Url(T),
}

impl<T: Pep508Url> Display for VersionOrUrl<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VersionSpecifier(version_specifier) => Display::fmt(version_specifier, f),
            Self::Url(url) => Display::fmt(url, f),
        }
    }
}

/// Unowned version specifier or URL to install.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub enum VersionOrUrlRef<'a, T: Pep508Url = VerbatimUrl> {
    /// A PEP 440 version specifier set
    VersionSpecifier(&'a VersionSpecifiers),
    /// A installable URL
    Url(&'a T),
}

impl<T: Pep508Url> Display for VersionOrUrlRef<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VersionSpecifier(version_specifier) => Display::fmt(version_specifier, f),
            Self::Url(url) => Display::fmt(url, f),
        }
    }
}

impl<'a> From<&'a VersionOrUrl> for VersionOrUrlRef<'a> {
    fn from(value: &'a VersionOrUrl) -> Self {
        match value {
            VersionOrUrl::VersionSpecifier(version_specifier) => {
                VersionOrUrlRef::VersionSpecifier(version_specifier)
            }
            VersionOrUrl::Url(url) => VersionOrUrlRef::Url(url),
        }
    }
}

fn parse_name<T: Pep508Url>(cursor: &mut Cursor) -> Result<PackageName, Pep508Error<T>> {
    // https://peps.python.org/pep-0508/#names
    // ^([A-Z0-9]|[A-Z0-9][A-Z0-9._-]*[A-Z0-9])$ with re.IGNORECASE
    let start = cursor.pos();
    let mut name = String::new();

    if let Some((index, char)) = cursor.next() {
        if matches!(char, 'A'..='Z' | 'a'..='z' | '0'..='9') {
            name.push(char);
        } else {
            // Check if the user added a filesystem path without a package name. pip supports this
            // in `requirements.txt`, but it doesn't adhere to the PEP 508 grammar.
            let mut clone = cursor.clone().at(start);
            return if looks_like_unnamed_requirement(&mut clone) {
                Err(Pep508Error {
                    message: Pep508ErrorSource::UnsupportedRequirement("URL requirement must be preceded by a package name. Add the name of the package before the URL (e.g., `package_name @ /path/to/file`).".to_string()),
                    start,
                    len: clone.pos() - start,
                    input: clone.to_string(),
                })
            } else {
                Err(Pep508Error {
                    message: Pep508ErrorSource::String(format!(
                        "Expected package name starting with an alphanumeric character, found '{char}'"
                    )),
                    start: index,
                    len: char.len_utf8(),
                    input: cursor.to_string(),
                })
            };
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

/// Parse a potential URL from the [`Cursor`], advancing the [`Cursor`] to the end of the URL.
///
/// Returns `true` if the URL appears to be a viable unnamed requirement, and `false` otherwise.
fn looks_like_unnamed_requirement(cursor: &mut Cursor) -> bool {
    // Read the entire path.
    let (start, len) = cursor.take_while(|char| !char.is_whitespace());
    let url = cursor.slice(start, len);

    // Expand any environment variables in the path.
    let expanded = expand_env_vars(url);

    // Analyze the path.
    let mut chars = expanded.chars();

    let Some(first_char) = chars.next() else {
        return false;
    };

    // Ex) `/bin/ls`
    if first_char == '\\' || first_char == '/' || first_char == '.' {
        return true;
    }

    // Ex) `https://` or `C:`
    if split_scheme(&expanded).is_some() {
        return true;
    }

    false
}

/// parses extras in the `[extra1,extra2] format`
fn parse_extras_cursor<T: Pep508Url>(
    cursor: &mut Cursor,
) -> Result<Vec<ExtraName>, Pep508Error<T>> {
    let Some(bracket_pos) = cursor.eat_char('[') else {
        return Ok(vec![]);
    };
    cursor.eat_whitespace();

    let mut extras = Vec::new();
    let mut is_first_iteration = true;

    loop {
        // End of the extras section. (Empty extras are allowed.)
        if let Some(']') = cursor.peek_char() {
            cursor.next();
            break;
        }

        // Comma separator
        match (cursor.peek(), is_first_iteration) {
            // For the first iteration, we don't expect a comma.
            (Some((pos, ',')), true) => {
                return Err(Pep508Error {
                    message: Pep508ErrorSource::String(
                        "Expected either alphanumerical character (starting the extra name) or ']' (ending the extras section), found ','".to_string()
                    ),
                    start: pos,
                    len: 1,
                    input: cursor.to_string(),
                });
            }
            // For the other iterations, the comma is required.
            (Some((_, ',')), false) => {
                cursor.next();
            }
            (Some((pos, other)), false) => {
                return Err(Pep508Error {
                    message: Pep508ErrorSource::String(
                        format!("Expected either ',' (separating extras) or ']' (ending the extras section), found '{other}'")
                    ),
                    start: pos,
                    len: 1,
                    input: cursor.to_string(),
                });
            }
            _ => {}
        }

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

        // First char of the identifier.
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

        // Add the parsed extra
        extras.push(
            ExtraName::new(buffer).expect("`ExtraName` validation should match PEP 508 parsing"),
        );
        is_first_iteration = false;
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
fn parse_url<T: Pep508Url>(
    cursor: &mut Cursor,
    working_dir: Option<&Path>,
) -> Result<T, Pep508Error<T>> {
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

    let url = T::parse_url(url, working_dir).map_err(|err| Pep508Error {
        message: Pep508ErrorSource::UrlError(err),
        start,
        len,
        input: cursor.to_string(),
    })?;

    Ok(url)
}

/// Identify the extras in a relative URL (e.g., `../editable[dev]`).
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

/// PEP 440 wrapper
fn parse_specifier<T: Pep508Url>(
    cursor: &mut Cursor,
    buffer: &str,
    start: usize,
    end: usize,
) -> Result<VersionSpecifier, Pep508Error<T>> {
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
fn parse_version_specifier<T: Pep508Url>(
    cursor: &mut Cursor,
) -> Result<Option<VersionOrUrl<T>>, Pep508Error<T>> {
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
fn parse_version_specifier_parentheses<T: Pep508Url>(
    cursor: &mut Cursor,
) -> Result<Option<VersionOrUrl<T>>, Pep508Error<T>> {
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

/// Parse a PEP 508-compliant [dependency specifier](https://packaging.python.org/en/latest/specifications/dependency-specifiers).
fn parse_pep508_requirement<T: Pep508Url>(
    cursor: &mut Cursor,
    working_dir: Option<&Path>,
    reporter: &mut impl Reporter,
) -> Result<Requirement<T>, Pep508Error<T>> {
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
    let extras = parse_extras_cursor(cursor)?;
    // wsp*
    cursor.eat_whitespace();

    // ( url_req | name_req )?
    let requirement_kind = match cursor.peek_char() {
        // url_req
        Some('@') => {
            cursor.next();
            Some(VersionOrUrl::Url(parse_url(cursor, working_dir)?))
        }
        // name_req
        Some('(') => parse_version_specifier_parentheses(cursor)?,
        // name_req
        Some('<' | '=' | '>' | '~' | '!') => parse_version_specifier(cursor)?,
        // No requirements / any version
        Some(';') | None => None,
        Some(other) => {
            // Rewind to the start of the version specifier, to see if the user added a URL without
            // a package name. pip supports this in `requirements.txt`, but it doesn't adhere to
            // the PEP 508 grammar.
            let mut clone = cursor.clone().at(start);
            return if parse_url::<T>(&mut clone, working_dir).is_ok() {
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

    let requirement_end = cursor.pos();

    // wsp*
    cursor.eat_whitespace();
    // quoted_marker?
    let marker = if cursor.peek_char() == Some(';') {
        // Skip past the semicolon
        cursor.next();
        Some(marker::parse::parse_markers_cursor(cursor, reporter)?)
    } else {
        None
    };
    // wsp*
    cursor.eat_whitespace();
    if let Some((pos, char)) = cursor.next() {
        if let Some(VersionOrUrl::Url(url)) = requirement_kind {
            if marker.is_none() && url.to_string().ends_with(';') {
                return Err(Pep508Error {
                    message: Pep508ErrorSource::String(
                        "Missing space before ';', the end of the URL is ambiguous".to_string(),
                    ),
                    start: requirement_end - ';'.len_utf8(),
                    len: ';'.len_utf8(),
                    input: cursor.to_string(),
                });
            }
        }
        let message = if marker.is_none() {
            format!(r#"Expected end of input or ';', found '{char}'"#)
        } else {
            format!(r#"Expected end of input, found '{char}'"#)
        };
        return Err(Pep508Error {
            message: Pep508ErrorSource::String(message),
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
        origin: None,
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
pub fn python_module(py: Python<'_>, m: &pyo3::Bound<'_, PyModule>) -> PyResult<()> {
    // Allowed to fail if we embed this module in another

    #[allow(unused_must_use)]
    {
        pyo3_log::try_init();
    }

    m.add_class::<PyVersion>()?;
    m.add_class::<VersionSpecifier>()?;

    m.add_class::<PyRequirement>()?;
    m.add_class::<MarkerEnvironment>()?;
    m.add("Pep508Error", py.get_type_bound::<PyPep508Error>())?;
    Ok(())
}

/// Half of these tests are copied from <https://github.com/pypa/packaging/pull/624>
#[cfg(test)]
mod tests {
    use std::env;
    use std::str::FromStr;

    use insta::assert_snapshot;
    use url::Url;

    use pep440_rs::{Operator, Version, VersionPattern, VersionSpecifier};
    use uv_normalize::{ExtraName, InvalidNameError, PackageName};

    use crate::cursor::Cursor;
    use crate::marker::{
        parse, MarkerExpression, MarkerOperator, MarkerTree, MarkerValueString, MarkerValueVersion,
    };
    use crate::{Requirement, TracingReporter, VerbatimUrl, VersionOrUrl};

    fn parse_pep508_err(input: &str) -> String {
        Requirement::<VerbatimUrl>::from_str(input)
            .unwrap_err()
            .to_string()
    }

    #[cfg(feature = "non-pep508-extensions")]
    fn parse_unnamed_err(input: &str) -> String {
        crate::UnnamedRequirement::<VerbatimUrl>::from_str(input)
            .unwrap_err()
            .to_string()
    }

    #[cfg(windows)]
    #[test]
    fn test_preprocess_url_windows() {
        use std::path::PathBuf;

        let actual = crate::parse_url::<VerbatimUrl>(
            &mut Cursor::new("file:///C:/Users/ferris/wheel-0.42.0.tar.gz"),
            None,
        )
        .unwrap()
        .to_file_path();
        let expected = PathBuf::from(r"C:\Users\ferris\wheel-0.42.0.tar.gz");
        assert_eq!(actual, Ok(expected));
    }

    #[test]
    fn error_empty() {
        assert_snapshot!(
            parse_pep508_err(""),
            @r"
        Empty field is not allowed for PEP508

        ^"
        );
    }

    #[test]
    fn error_start() {
        assert_snapshot!(
            parse_pep508_err("_name"),
            @"
            Expected package name starting with an alphanumeric character, found '_'
            _name
            ^"
        );
    }

    #[test]
    fn error_end() {
        assert_snapshot!(
            parse_pep508_err("name_"),
            @"
            Package name must end with an alphanumeric character, not '_'
            name_
                ^"
        );
    }

    #[test]
    fn basic_examples() {
        let input = r"requests[security,tests]>=2.8.1,==2.8.* ; python_version < '2.7'";
        let requests = Requirement::<Url>::from_str(input).unwrap();
        assert_eq!(input, requests.to_string());
        let expected = Requirement {
            name: PackageName::from_str("requests").unwrap(),
            extras: vec![
                ExtraName::from_str("security").unwrap(),
                ExtraName::from_str("tests").unwrap(),
            ],
            version_or_url: Some(VersionOrUrl::VersionSpecifier(
                [
                    VersionSpecifier::from_pattern(
                        Operator::GreaterThanEqual,
                        VersionPattern::verbatim(Version::new([2, 8, 1])),
                    )
                    .unwrap(),
                    VersionSpecifier::from_pattern(
                        Operator::Equal,
                        VersionPattern::wildcard(Version::new([2, 8])),
                    )
                    .unwrap(),
                ]
                .into_iter()
                .collect(),
            )),
            marker: Some(MarkerTree::Expression(MarkerExpression::Version {
                key: MarkerValueVersion::PythonVersion,
                specifier: VersionSpecifier::from_pattern(
                    pep440_rs::Operator::LessThan,
                    "2.7".parse().unwrap(),
                )
                .unwrap(),
            })),
            origin: None,
        };
        assert_eq!(requests, expected);
    }

    #[test]
    fn parenthesized_single() {
        let numpy = Requirement::<Url>::from_str("numpy ( >=1.19 )").unwrap();
        assert_eq!(numpy.name.as_ref(), "numpy");
    }

    #[test]
    fn parenthesized_double() {
        let numpy = Requirement::<Url>::from_str("numpy ( >=1.19, <2.0 )").unwrap();
        assert_eq!(numpy.name.as_ref(), "numpy");
    }

    #[test]
    fn versions_single() {
        let numpy = Requirement::<Url>::from_str("numpy >=1.19 ").unwrap();
        assert_eq!(numpy.name.as_ref(), "numpy");
    }

    #[test]
    fn versions_double() {
        let numpy = Requirement::<Url>::from_str("numpy >=1.19, <2.0 ").unwrap();
        assert_eq!(numpy.name.as_ref(), "numpy");
    }

    #[test]
    #[cfg(feature = "non-pep508-extensions")]
    fn direct_url_no_extras() {
        let numpy = crate::UnnamedRequirement::<VerbatimUrl>::from_str("https://files.pythonhosted.org/packages/28/4a/46d9e65106879492374999e76eb85f87b15328e06bd1550668f79f7b18c6/numpy-1.26.4-cp312-cp312-win32.whl").unwrap();
        assert_eq!(numpy.url.to_string(), "https://files.pythonhosted.org/packages/28/4a/46d9e65106879492374999e76eb85f87b15328e06bd1550668f79f7b18c6/numpy-1.26.4-cp312-cp312-win32.whl");
        assert_eq!(numpy.extras, vec![]);
    }

    #[test]
    #[cfg(all(unix, feature = "non-pep508-extensions"))]
    fn direct_url_extras() {
        let numpy = crate::UnnamedRequirement::<VerbatimUrl>::from_str(
            "/path/to/numpy-1.26.4-cp312-cp312-win32.whl[dev]",
        )
        .unwrap();
        assert_eq!(
            numpy.url.to_string(),
            "file:///path/to/numpy-1.26.4-cp312-cp312-win32.whl"
        );
        assert_eq!(numpy.extras, vec![ExtraName::from_str("dev").unwrap()]);
    }

    #[test]
    #[cfg(all(windows, feature = "non-pep508-extensions"))]
    fn direct_url_extras() {
        let numpy = crate::UnnamedRequirement::<VerbatimUrl>::from_str(
            "C:\\path\\to\\numpy-1.26.4-cp312-cp312-win32.whl[dev]",
        )
        .unwrap();
        assert_eq!(
            numpy.url.to_string(),
            "file:///C:/path/to/numpy-1.26.4-cp312-cp312-win32.whl"
        );
        assert_eq!(numpy.extras, vec![ExtraName::from_str("dev").unwrap()]);
    }

    #[test]
    fn error_extras_eof1() {
        assert_snapshot!(
            parse_pep508_err("black["),
            @"
            Missing closing bracket (expected ']', found end of dependency specification)
            black[
                 ^"
        );
    }

    #[test]
    fn error_extras_eof2() {
        assert_snapshot!(
            parse_pep508_err("black[d"),
            @"
            Missing closing bracket (expected ']', found end of dependency specification)
            black[d
                 ^"
        );
    }

    #[test]
    fn error_extras_eof3() {
        assert_snapshot!(
            parse_pep508_err("black[d,"),
            @"
            Missing closing bracket (expected ']', found end of dependency specification)
            black[d,
                 ^"
        );
    }

    #[test]
    fn error_extras_illegal_start1() {
        assert_snapshot!(
            parse_pep508_err("black[ö]"),
            @"
            Expected an alphanumeric character starting the extra name, found 'ö'
            black[ö]
                  ^"
        );
    }

    #[test]
    fn error_extras_illegal_start2() {
        assert_snapshot!(
            parse_pep508_err("black[_d]"),
            @"
            Expected an alphanumeric character starting the extra name, found '_'
            black[_d]
                  ^"
        );
    }

    #[test]
    fn error_extras_illegal_start3() {
        assert_snapshot!(
            parse_pep508_err("black[,]"),
            @"
            Expected either alphanumerical character (starting the extra name) or ']' (ending the extras section), found ','
            black[,]
                  ^"
        );
    }

    #[test]
    fn error_extras_illegal_character() {
        assert_snapshot!(
            parse_pep508_err("black[jüpyter]"),
            @"
            Invalid character in extras name, expected an alphanumeric character, '-', '_', '.', ',' or ']', found 'ü'
            black[jüpyter]
                   ^"
        );
    }

    #[test]
    fn error_extras1() {
        let numpy = Requirement::<Url>::from_str("black[d]").unwrap();
        assert_eq!(numpy.extras, vec![ExtraName::from_str("d").unwrap()]);
    }

    #[test]
    fn error_extras2() {
        let numpy = Requirement::<Url>::from_str("black[d,jupyter]").unwrap();
        assert_eq!(
            numpy.extras,
            vec![
                ExtraName::from_str("d").unwrap(),
                ExtraName::from_str("jupyter").unwrap(),
            ]
        );
    }

    #[test]
    fn empty_extras() {
        let black = Requirement::<Url>::from_str("black[]").unwrap();
        assert_eq!(black.extras, vec![]);
    }

    #[test]
    fn empty_extras_with_spaces() {
        let black = Requirement::<Url>::from_str("black[  ]").unwrap();
        assert_eq!(black.extras, vec![]);
    }

    #[test]
    fn error_extra_with_trailing_comma() {
        assert_snapshot!(
            parse_pep508_err("black[d,]"),
            @"
            Expected an alphanumeric character starting the extra name, found ']'
            black[d,]
                    ^"
        );
    }

    #[test]
    fn error_parenthesized_pep440() {
        assert_snapshot!(
            parse_pep508_err("numpy ( ><1.19 )"),
            @"
            no such comparison operator \"><\", must be one of ~= == != <= >= < > ===
            numpy ( ><1.19 )
                    ^^^^^^^"
        );
    }

    #[test]
    fn error_parenthesized_parenthesis() {
        assert_snapshot!(
            parse_pep508_err("numpy ( >=1.19"),
            @"
            Missing closing parenthesis (expected ')', found end of dependency specification)
            numpy ( >=1.19
                  ^"
        );
    }

    #[test]
    fn error_whats_that() {
        assert_snapshot!(
            parse_pep508_err("numpy % 1.16"),
            @"
            Expected one of `@`, `(`, `<`, `=`, `>`, `~`, `!`, `;`, found `%`
            numpy % 1.16
                  ^"
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
            version_or_url: Some(VersionOrUrl::Url(Url::parse(url).unwrap())),
            origin: None,
        };
        assert_eq!(pip_url, expected);
    }

    #[test]
    fn test_marker_parsing() {
        let marker = r#"python_version == "2.7" and (sys_platform == "win32" or (os_name == "linux" and implementation_name == 'cpython'))"#;
        let actual = parse::parse_markers_cursor::<VerbatimUrl>(
            &mut Cursor::new(marker),
            &mut TracingReporter,
        )
        .unwrap();
        let expected = MarkerTree::And(vec![
            MarkerTree::Expression(MarkerExpression::Version {
                key: MarkerValueVersion::PythonVersion,
                specifier: VersionSpecifier::from_pattern(
                    pep440_rs::Operator::Equal,
                    "2.7".parse().unwrap(),
                )
                .unwrap(),
            }),
            MarkerTree::Or(vec![
                MarkerTree::Expression(MarkerExpression::String {
                    key: MarkerValueString::SysPlatform,
                    operator: MarkerOperator::Equal,
                    value: "win32".to_string(),
                }),
                MarkerTree::And(vec![
                    MarkerTree::Expression(MarkerExpression::String {
                        key: MarkerValueString::OsName,
                        operator: MarkerOperator::Equal,
                        value: "linux".to_string(),
                    }),
                    MarkerTree::Expression(MarkerExpression::String {
                        key: MarkerValueString::ImplementationName,
                        operator: MarkerOperator::Equal,
                        value: "cpython".to_string(),
                    }),
                ]),
            ]),
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn name_and_marker() {
        Requirement::<Url>::from_str(r#"numpy; sys_platform == "win32" or (os_name == "linux" and implementation_name == 'cpython')"#).unwrap();
    }

    #[test]
    fn error_marker_incomplete1() {
        assert_snapshot!(
            parse_pep508_err(r"numpy; sys_platform"),
            @"
                Expected a valid marker operator (such as '>=' or 'not in'), found ''
                numpy; sys_platform
                                   ^"
        );
    }

    #[test]
    fn error_marker_incomplete2() {
        assert_snapshot!(
            parse_pep508_err(r"numpy; sys_platform =="),
            @r"
            Expected marker value, found end of dependency specification
            numpy; sys_platform ==
                                  ^"
        );
    }

    #[test]
    fn error_marker_incomplete3() {
        assert_snapshot!(
            parse_pep508_err(r#"numpy; sys_platform == "win32" or"#),
            @r#"
            Expected marker value, found end of dependency specification
            numpy; sys_platform == "win32" or
                                             ^"#
        );
    }

    #[test]
    fn error_marker_incomplete4() {
        assert_snapshot!(
            parse_pep508_err(r#"numpy; sys_platform == "win32" or (os_name == "linux""#),
            @r#"
            Expected ')', found end of dependency specification
            numpy; sys_platform == "win32" or (os_name == "linux"
                                              ^"#
        );
    }

    #[test]
    fn error_marker_incomplete5() {
        assert_snapshot!(
            parse_pep508_err(r#"numpy; sys_platform == "win32" or (os_name == "linux" and"#),
            @r#"
            Expected marker value, found end of dependency specification
            numpy; sys_platform == "win32" or (os_name == "linux" and
                                                                     ^"#
        );
    }

    #[test]
    fn error_pep440() {
        assert_snapshot!(
            parse_pep508_err(r"numpy >=1.1.*"),
            @r"
            Operator >= cannot be used with a wildcard version specifier
            numpy >=1.1.*
                  ^^^^^^^"
        );
    }

    #[test]
    fn error_no_name() {
        assert_snapshot!(
            parse_pep508_err(r"==0.0"),
            @r"
        Expected package name starting with an alphanumeric character, found '='
        ==0.0
        ^
        "
        );
    }

    #[test]
    fn error_unnamedunnamed_url() {
        assert_snapshot!(
            parse_pep508_err(r"git+https://github.com/pallets/flask.git"),
            @"
            URL requirement must be preceded by a package name. Add the name of the package before the URL (e.g., `package_name @ https://...`).
            git+https://github.com/pallets/flask.git
            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^"
        );
    }

    #[test]
    fn error_unnamed_file_path() {
        assert_snapshot!(
            parse_pep508_err(r"/path/to/flask.tar.gz"),
            @r###"
        URL requirement must be preceded by a package name. Add the name of the package before the URL (e.g., `package_name @ /path/to/file`).
        /path/to/flask.tar.gz
        ^^^^^^^^^^^^^^^^^^^^^
        "###
        );
    }

    #[test]
    fn error_no_comma_between_extras() {
        assert_snapshot!(
            parse_pep508_err(r"name[bar baz]"),
            @"
            Expected either ',' (separating extras) or ']' (ending the extras section), found 'b'
            name[bar baz]
                     ^"
        );
    }

    #[test]
    fn error_extra_comma_after_extras() {
        assert_snapshot!(
            parse_pep508_err(r"name[bar, baz,]"),
            @"
            Expected an alphanumeric character starting the extra name, found ']'
            name[bar, baz,]
                          ^"
        );
    }

    #[test]
    fn error_extras_not_closed() {
        assert_snapshot!(
            parse_pep508_err(r"name[bar, baz >= 1.0"),
            @"
            Expected either ',' (separating extras) or ']' (ending the extras section), found '>'
            name[bar, baz >= 1.0
                          ^"
        );
    }

    #[test]
    fn error_no_space_after_url() {
        assert_snapshot!(
            parse_pep508_err(r"name @ https://example.com/; extra == 'example'"),
            @"
            Missing space before ';', the end of the URL is ambiguous
            name @ https://example.com/; extra == 'example'
                                       ^"
        );
    }

    #[test]
    fn error_name_at_nothing() {
        assert_snapshot!(
            parse_pep508_err(r"name @"),
            @"
            Expected URL
            name @
                  ^"
        );
    }

    #[test]
    fn test_error_invalid_marker_key() {
        assert_snapshot!(
            parse_pep508_err(r"name; invalid_name"),
            @"
            Expected a valid marker name, found 'invalid_name'
            name; invalid_name
                  ^^^^^^^^^^^^"
        );
    }

    #[test]
    fn error_markers_invalid_order() {
        assert_snapshot!(
            parse_pep508_err("name; '3.7' <= invalid_name"),
            @"
            Expected a valid marker name, found 'invalid_name'
            name; '3.7' <= invalid_name
                           ^^^^^^^^^^^^"
        );
    }

    #[test]
    fn error_markers_notin() {
        assert_snapshot!(
            parse_pep508_err("name; '3.7' notin python_version"),
            @"
            Expected a valid marker operator (such as '>=' or 'not in'), found 'notin'
            name; '3.7' notin python_version
                        ^^^^^"
        );
    }

    #[test]
    fn error_markers_inpython_version() {
        assert_snapshot!(
            parse_pep508_err("name; '3.6'inpython_version"),
            @"
            Expected a valid marker operator (such as '>=' or 'not in'), found 'inpython_version'
            name; '3.6'inpython_version
                       ^^^^^^^^^^^^^^^^"
        );
    }

    #[test]
    fn error_markers_not_python_version() {
        assert_snapshot!(
            parse_pep508_err("name; '3.7' not python_version"),
            @"
            Expected 'i', found 'p'
            name; '3.7' not python_version
                            ^"
        );
    }

    #[test]
    fn error_markers_invalid_operator() {
        assert_snapshot!(
            parse_pep508_err("name; '3.7' ~ python_version"),
            @"
            Expected a valid marker operator (such as '>=' or 'not in'), found '~'
            name; '3.7' ~ python_version
                        ^"
        );
    }

    #[test]
    fn error_invalid_prerelease() {
        assert_snapshot!(
            parse_pep508_err("name==1.0.org1"),
            @r###"
        after parsing '1.0', found '.org1', which is not part of a valid version
        name==1.0.org1
            ^^^^^^^^^^
        "###
        );
    }

    #[test]
    fn error_no_version_value() {
        assert_snapshot!(
            parse_pep508_err("name=="),
            @"
            Unexpected end of version specifier, expected version
            name==
                ^^"
        );
    }

    #[test]
    fn error_no_version_operator() {
        assert_snapshot!(
            parse_pep508_err("name 1.0"),
            @"
            Expected one of `@`, `(`, `<`, `=`, `>`, `~`, `!`, `;`, found `1`
            name 1.0
                 ^"
        );
    }

    #[test]
    fn error_random_char() {
        assert_snapshot!(
            parse_pep508_err("name >= 1.0 #"),
            @"
            Trailing `#` is not allowed
            name >= 1.0 #
                 ^^^^^^^^"
        );
    }

    #[test]
    #[cfg(feature = "non-pep508-extensions")]
    fn error_invalid_extra_unnamed_url() {
        assert_snapshot!(
            parse_unnamed_err("/foo-3.0.0-py3-none-any.whl[d,]"),
            @r###"
        Expected an alphanumeric character starting the extra name, found ']'
        /foo-3.0.0-py3-none-any.whl[d,]
                                      ^
        "###
        );
    }

    /// Check that the relative path support feature toggle works.
    #[test]
    fn non_pep508_paths() {
        let requirements = &[
            "foo @ file://./foo",
            "foo @ file://foo-3.0.0-py3-none-any.whl",
            "foo @ file:foo-3.0.0-py3-none-any.whl",
            "foo @ ./foo-3.0.0-py3-none-any.whl",
        ];
        let cwd = env::current_dir().unwrap();

        for requirement in requirements {
            assert_eq!(
                Requirement::<VerbatimUrl>::parse(requirement, &cwd).is_ok(),
                cfg!(feature = "non-pep508-extensions"),
                "{}: {:?}",
                requirement,
                Requirement::<VerbatimUrl>::parse(requirement, &cwd)
            );
        }
    }

    #[test]
    fn no_space_after_operator() {
        let requirement = Requirement::<Url>::from_str("pytest;python_version<='4.0'").unwrap();
        assert_eq!(requirement.to_string(), "pytest ; python_version <= '4.0'");

        let requirement = Requirement::<Url>::from_str("pytest;'4.0'>=python_version").unwrap();
        assert_eq!(requirement.to_string(), "pytest ; python_version <= '4.0'");
    }

    #[test]
    fn path_with_fragment() {
        let requirements = if cfg!(windows) {
            &[
                "wheel @ file:///C:/Users/ferris/wheel-0.42.0.whl#hash=somehash",
                "wheel @ C:/Users/ferris/wheel-0.42.0.whl#hash=somehash",
            ]
        } else {
            &[
                "wheel @ file:///Users/ferris/wheel-0.42.0.whl#hash=somehash",
                "wheel @ /Users/ferris/wheel-0.42.0.whl#hash=somehash",
            ]
        };

        for requirement in requirements {
            // Extract the URL.
            let Some(VersionOrUrl::Url(url)) = Requirement::<VerbatimUrl>::from_str(requirement)
                .unwrap()
                .version_or_url
            else {
                unreachable!("Expected a URL")
            };

            // Assert that the fragment and path have been separated correctly.
            assert_eq!(url.fragment(), Some("hash=somehash"));
            assert!(
                url.path().ends_with("/Users/ferris/wheel-0.42.0.whl"),
                "Expected the path to end with '/Users/ferris/wheel-0.42.0.whl', found '{}'",
                url.path()
            );
        }
    }

    #[test]
    fn add_extra_marker() -> Result<(), InvalidNameError> {
        let requirement = Requirement::<Url>::from_str("pytest").unwrap();
        let expected = Requirement::<Url>::from_str("pytest; extra == 'dotenv'").unwrap();
        let actual = requirement.with_extra_marker(&ExtraName::from_str("dotenv")?);
        assert_eq!(actual, expected);

        let requirement = Requirement::<Url>::from_str("pytest; '4.0' >= python_version").unwrap();
        let expected =
            Requirement::from_str("pytest; '4.0' >= python_version and extra == 'dotenv'").unwrap();
        let actual = requirement.with_extra_marker(&ExtraName::from_str("dotenv")?);
        assert_eq!(actual, expected);

        let requirement = Requirement::<Url>::from_str(
            "pytest; '4.0' >= python_version or sys_platform == 'win32'",
        )
        .unwrap();
        let expected = Requirement::from_str(
            "pytest; ('4.0' >= python_version or sys_platform == 'win32') and extra == 'dotenv'",
        )
        .unwrap();
        let actual = requirement.with_extra_marker(&ExtraName::from_str("dotenv")?);
        assert_eq!(actual, expected);

        Ok(())
    }
}

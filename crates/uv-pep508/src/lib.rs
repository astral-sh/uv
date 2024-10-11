//! A library for python [dependency specifiers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
//! better known as [PEP 508](https://peps.python.org/pep-0508/)
//!
//! ## Usage
//!
//! ```
//! use std::str::FromStr;
//! use uv_pep508::{Requirement, VerbatimUrl};
//! use uv_normalize::ExtraName;
//!
//! let marker = r#"requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8""#;
//! let dependency_specification = Requirement::<VerbatimUrl>::from_str(marker).unwrap();
//! assert_eq!(dependency_specification.name.as_ref(), "requests");
//! assert_eq!(dependency_specification.extras, vec![ExtraName::from_str("security").unwrap(), ExtraName::from_str("tests").unwrap()]);
//! ```

#![warn(missing_docs)]

use std::collections::HashSet;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::path::Path;
use std::str::FromStr;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use url::Url;

use crate::marker::MarkerValueExtra;
use cursor::Cursor;
pub use marker::{
    ContainsMarkerTree, ExtraMarkerTree, ExtraOperator, InMarkerTree, MarkerEnvironment,
    MarkerEnvironmentBuilder, MarkerExpression, MarkerOperator, MarkerTree, MarkerTreeContents,
    MarkerTreeKind, MarkerValue, MarkerValueString, MarkerValueVersion, MarkerWarningKind,
    StringMarkerTree, StringVersion, VersionMarkerTree,
};
pub use origin::RequirementOrigin;
#[cfg(feature = "non-pep508-extensions")]
pub use unnamed::{UnnamedRequirement, UnnamedRequirementUrl};
pub use uv_normalize::{ExtraName, InvalidNameError, PackageName};
use uv_pep440::{Version, VersionSpecifier, VersionSpecifiers};
pub use verbatim_url::{
    expand_env_vars, split_scheme, strip_host, Scheme, VerbatimUrl, VerbatimUrlError,
};

mod cursor;
pub mod marker;
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

/// A PEP 508 dependency specifier.
#[derive(Hash, Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
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
    pub marker: MarkerTree,
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
        if let Some(marker) = self.marker.contents() {
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

impl<T: Pep508Url> Requirement<T> {
    /// Returns whether the markers apply for the given environment
    pub fn evaluate_markers(&self, env: &MarkerEnvironment, extras: &[ExtraName]) -> bool {
        self.marker.evaluate(env, extras)
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
        self.marker
            .evaluate_extras_and_python_version(extras, python_versions)
    }

    /// Returns whether the markers apply for the given environment.
    pub fn evaluate_markers_and_report(
        &self,
        env: &MarkerEnvironment,
        extras: &[ExtraName],
    ) -> (bool, Vec<MarkerWarning>) {
        self.marker.evaluate_collect_warnings(env, extras)
    }

    /// Return the requirement with an additional marker added, to require the given extra.
    ///
    /// For example, given `flask >= 2.0.2`, calling `with_extra_marker("dotenv")` would return
    /// `flask >= 2.0.2 ; extra == "dotenv"`.
    #[must_use]
    pub fn with_extra_marker(mut self, extra: &ExtraName) -> Self {
        self.marker
            .and(MarkerTree::expression(MarkerExpression::Extra {
                operator: ExtraOperator::Equal,
                name: MarkerValueExtra::Extra(extra.clone()),
            }));

        self
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
                description: Some(
                    "A PEP 508 dependency specifier, e.g., `ruff >= 0.6.0`".to_string(),
                ),
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
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
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
                        "Expected package name starting with an alphanumeric character, found `{char}`"
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

    // Strip extras.
    let url = split_extras(&expanded)
        .map(|(url, _)| url)
        .unwrap_or(&expanded);

    // Analyze the path.
    let mut chars = url.chars();

    let Some(first_char) = chars.next() else {
        return false;
    };

    // Ex) `/bin/ls`
    if first_char == '\\' || first_char == '/' || first_char == '.' {
        return true;
    }

    // Ex) `https://` or `C:`
    if split_scheme(url).is_some() {
        return true;
    }

    // Ex) `foo/bar`
    if url.contains('/') || url.contains('\\') {
        return true;
    }

    // Ex) `foo.tar.gz`
    if looks_like_archive(url) {
        return true;
    }

    false
}

/// Returns `true` if a file looks like an archive.
///
/// See <https://github.com/pypa/pip/blob/111eed14b6e9fba7c78a5ec2b7594812d17b5d2b/src/pip/_internal/utils/filetypes.py#L8>
/// for the list of supported archive extensions.
fn looks_like_archive(file: impl AsRef<Path>) -> bool {
    let file = file.as_ref();

    // E.g., `gz` in `foo.tar.gz`
    let Some(extension) = file.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };

    // E.g., `tar` in `foo.tar.gz`
    let pre_extension = file
        .file_stem()
        .and_then(|stem| Path::new(stem).extension().and_then(|ext| ext.to_str()));

    matches!(
        (pre_extension, extension),
        (_, "whl" | "tbz" | "txz" | "tlz" | "zip" | "tgz" | "tar")
            | (Some("tar"), "bz2" | "xz" | "lz" | "lzma" | "gz")
    )
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
                        "Expected either alphanumerical character (starting the extra name) or `]` (ending the extras section), found `,`".to_string()
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
                        format!("Expected either `,` (separating extras) or `]` (ending the extras section), found `{other}`")
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
                        "Expected an alphanumeric character starting the extra name, found `{other}`"
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
                        "Invalid character in extras name, expected an alphanumeric character, `-`, `_`, `.`, `,` or `]`, found `{char}`"
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
/// When parsing, we eat characters until we see any of the following:
/// - A newline.
/// - A semicolon (marker) or hash (comment), _preceded_ by a space. We parse the URL until the last
///   non-whitespace character (inclusive).
/// - A semicolon (marker) or hash (comment) _followed_ by a space. We treat this as an error, since
///   the end of the URL is ambiguous.
///
/// For example:
/// - `https://pypi.org/project/requests/...`
/// - `file:///home/ferris/project/scripts/...`
/// - `file:../editable/`
/// - `../editable/`
/// - `../path to editable/`
/// - `https://download.pytorch.org/whl/torch_stable.html`
fn parse_url<T: Pep508Url>(
    cursor: &mut Cursor,
    working_dir: Option<&Path>,
) -> Result<T, Pep508Error<T>> {
    // wsp*
    cursor.eat_whitespace();
    // <URI_reference>
    let (start, len) = {
        let start = cursor.pos();
        let mut len = 0;
        while let Some((_, c)) = cursor.next() {
            // If we see a line break, we're done.
            if matches!(c, '\r' | '\n') {
                break;
            }

            // If we see top-level whitespace, check if it's followed by a semicolon or hash. If so,
            // end the URL at the last non-whitespace character.
            if c.is_whitespace() {
                let mut cursor = cursor.clone();
                cursor.eat_whitespace();
                if matches!(cursor.peek_char(), None | Some(';' | '#')) {
                    break;
                }
            }

            len += c.len_utf8();

            // If we see a top-level semicolon or hash followed by whitespace, we're done.
            match c {
                ';' if cursor.peek_char().is_some_and(char::is_whitespace) => {
                    break;
                }
                '#' if cursor.peek_char().is_some_and(char::is_whitespace) => {
                    break;
                }
                _ => {}
            }
        }
        (start, len)
    };
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
    let name_start = cursor.pos();
    let name = parse_name(cursor)?;
    let name_end = cursor.pos();
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
            return if looks_like_unnamed_requirement(&mut clone) {
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

    // If the requirement consists solely of a package name, and that name appears to be an archive,
    // treat it as a URL requirement, for consistency and security. (E.g., `requests-2.26.0.tar.gz`
    // is a valid Python package name, but we should treat it as a reference to a file.)
    //
    // See: https://github.com/pypa/pip/blob/111eed14b6e9fba7c78a5ec2b7594812d17b5d2b/src/pip/_internal/utils/filetypes.py#L8
    if requirement_kind.is_none() {
        if looks_like_archive(cursor.slice(name_start, name_end)) {
            let clone = cursor.clone().at(start);
            return Err(Pep508Error {
                message: Pep508ErrorSource::UnsupportedRequirement("URL requirement must be preceded by a package name. Add the name of the package before the URL (e.g., `package_name @ https://...`).".to_string()),
                start,
                len: clone.pos() - start,
                input: clone.to_string(),
            });
        }
    }

    // wsp*
    cursor.eat_whitespace();
    // quoted_marker?
    let marker = if cursor.peek_char() == Some(';') {
        // Skip past the semicolon
        cursor.next();
        marker::parse::parse_markers_cursor(cursor, reporter)?
    } else {
        None
    };

    // wsp*
    cursor.eat_whitespace();
    if let Some((pos, char)) = cursor.next() {
        if marker.is_none() {
            if let Some(VersionOrUrl::Url(url)) = requirement_kind {
                let url = url.to_string();
                for c in [';', '#'] {
                    if url.ends_with(c) {
                        return Err(Pep508Error {
                            message: Pep508ErrorSource::String(format!(
                                "Missing space before '{c}', the end of the URL is ambiguous"
                            )),
                            start: requirement_end - c.len_utf8(),
                            len: c.len_utf8(),
                            input: cursor.to_string(),
                        });
                    }
                }
            }
        }
        let message = if marker.is_none() {
            format!(r#"Expected end of input or `;`, found `{char}`"#)
        } else {
            format!(r#"Expected end of input, found `{char}`"#)
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
        marker: marker.unwrap_or_default(),
        origin: None,
    })
}

#[cfg(test)]
mod tests;

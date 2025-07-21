//! A library for [dependency specifiers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
//! previously known as [PEP 508](https://peps.python.org/pep-0508/)
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
//! assert_eq!(dependency_specification.extras, vec![ExtraName::from_str("security").unwrap(), ExtraName::from_str("tests").unwrap()].into());
//! ```

#![warn(missing_docs)]

#[cfg(feature = "schemars")]
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;
use url::Url;

use uv_cache_key::{CacheKey, CacheKeyHasher};
use uv_normalize::{ExtraName, PackageName};

use crate::cursor::Cursor;
pub use crate::marker::{
    CanonicalMarkerValueExtra, CanonicalMarkerValueString, CanonicalMarkerValueVersion,
    ContainsMarkerTree, ExtraMarkerTree, ExtraOperator, InMarkerTree, MarkerEnvironment,
    MarkerEnvironmentBuilder, MarkerExpression, MarkerOperator, MarkerTree, MarkerTreeContents,
    MarkerTreeKind, MarkerValue, MarkerValueExtra, MarkerValueList, MarkerValueString,
    MarkerValueVersion, MarkerVariantsEnvironment, MarkerVariantsUniversal, MarkerWarningKind,
    StringMarkerTree, StringVersion, VariantFeature, VariantNamespace, VariantValue,
    VersionMarkerTree,
};
pub use crate::origin::RequirementOrigin;
#[cfg(feature = "non-pep508-extensions")]
pub use crate::unnamed::{UnnamedRequirement, UnnamedRequirementUrl};
pub use crate::verbatim_url::{
    Scheme, VerbatimUrl, VerbatimUrlError, expand_env_vars, looks_like_git_repository,
    split_scheme, strip_host,
};
/// Version and version specifiers used in requirements (reexport).
// https://github.com/konstin/pep508_rs/issues/19
pub use uv_pep440;
use uv_pep440::{VersionSpecifier, VersionSpecifiers};

use crate::marker::VariantParseError;

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
    /// The operator is not supported with the variant marker.
    #[error(
        "The operator {0} is not supported with the marker {1}, only the `in` and `not in` operators are supported"
    )]
    ListOperator(MarkerOperator, MarkerValueList),
    /// The value is not a quoted string.
    #[error("Only quoted strings are supported with the variant marker {1}, not {0}")]
    ListValue(MarkerValue, MarkerValueList),
    /// The variant marker is on the left hand side of the expression.
    #[error("The marker {0} must be on the right hand side of the expression")]
    ListLValue(MarkerValueList),
    /// A variant segment uses invalid characters.
    #[error(transparent)]
    InvalidVariantSegment(VariantParseError),
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
    pub extras: Box<[ExtraName]>,
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

    /// Returns a [`Display`] implementation that doesn't mask credentials.
    pub fn displayable_with_credentials(&self) -> impl Display {
        RequirementDisplay {
            requirement: self,
            display_credentials: true,
        }
    }
}

impl<T: Pep508Url + Display> Display for Requirement<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        RequirementDisplay {
            requirement: self,
            display_credentials: false,
        }
        .fmt(f)
    }
}

struct RequirementDisplay<'a, T>
where
    T: Pep508Url + Display,
{
    requirement: &'a Requirement<T>,
    display_credentials: bool,
}

impl<T: Pep508Url + Display> Display for RequirementDisplay<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.requirement.name)?;
        if !self.requirement.extras.is_empty() {
            write!(
                f,
                "[{}]",
                self.requirement
                    .extras
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )?;
        }
        if let Some(version_or_url) = &self.requirement.version_or_url {
            match version_or_url {
                VersionOrUrl::VersionSpecifier(version_specifier) => {
                    let version_specifier: Vec<String> =
                        version_specifier.iter().map(ToString::to_string).collect();
                    write!(f, "{}", version_specifier.join(","))?;
                }
                VersionOrUrl::Url(url) => {
                    let url_string = if self.display_credentials {
                        url.displayable_with_credentials().to_string()
                    } else {
                        url.to_string()
                    };
                    // We add the space for markers later if necessary
                    write!(f, " @ {url_string}")?;
                }
            }
        }
        if let Some(marker) = self.requirement.marker.contents() {
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
        struct RequirementVisitor<T>(std::marker::PhantomData<T>);

        impl<T: Pep508Url> serde::de::Visitor<'_> for RequirementVisitor<T> {
            type Value = Requirement<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string containing a PEP 508 requirement")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                FromStr::from_str(v).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(RequirementVisitor(std::marker::PhantomData))
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

impl<T: Pep508Url> CacheKey for Requirement<T> {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.name.as_str().cache_key(state);

        self.extras.len().cache_key(state);
        for extra in &self.extras {
            extra.as_str().cache_key(state);
        }

        // TODO(zanieb): We inline cache key handling for the child types here, but we could
        // move the implementations to the children. The intent here was to limit the scope of
        // types exposing the `CacheKey` trait for now.
        if let Some(version_or_url) = &self.version_or_url {
            1u8.cache_key(state);
            match version_or_url {
                VersionOrUrl::VersionSpecifier(spec) => {
                    0u8.cache_key(state);
                    spec.len().cache_key(state);
                    for specifier in spec.iter() {
                        specifier.operator().as_str().cache_key(state);
                        specifier.version().cache_key(state);
                    }
                }
                VersionOrUrl::Url(url) => {
                    1u8.cache_key(state);
                    url.cache_key(state);
                }
            }
        } else {
            0u8.cache_key(state);
        }

        if let Some(marker) = self.marker.contents() {
            1u8.cache_key(state);
            marker.to_string().cache_key(state);
        } else {
            0u8.cache_key(state);
        }

        // `origin` is intentionally omitted
    }
}

impl<T: Pep508Url> Requirement<T> {
    /// Returns whether the markers apply for the given environment
    pub fn evaluate_markers(
        &self,
        env: &MarkerEnvironment,
        variants: impl MarkerVariantsEnvironment,
        extras: &[ExtraName],
    ) -> bool {
        self.marker.evaluate(env, variants, extras)
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

/// Type to parse URLs from `name @ <url>` into. Defaults to [`Url`].
pub trait Pep508Url: Display + Debug + Sized + CacheKey {
    /// String to URL parsing error
    type Err: Error + Debug;

    /// Parse a url from `name @ <url>`. Defaults to [`Url::parse_url`].
    fn parse_url(url: &str, working_dir: Option<&Path>) -> Result<Self, Self::Err>;

    /// Returns a [`Display`] implementation that doesn't mask credentials.
    fn displayable_with_credentials(&self) -> impl Display;
}

impl Pep508Url for Url {
    type Err = url::ParseError;

    fn parse_url(url: &str, _working_dir: Option<&Path>) -> Result<Self, Self::Err> {
        Self::parse(url)
    }

    fn displayable_with_credentials(&self) -> impl Display {
        self
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
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Requirement")
    }

    fn json_schema(_gen: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "A PEP 508 dependency specifier, e.g., `ruff >= 0.6.0`"
        })
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

    if let Some((index, char)) = cursor.next() {
        if !matches!(char, 'A'..='Z' | 'a'..='z' | '0'..='9') {
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

    cursor.take_while(|char| matches!(char, 'A'..='Z' | 'a'..='z' | '0'..='9' | '.' | '-' | '_'));
    let len = cursor.pos() - start;
    // Unwrap-safety: The block above ensures that there is at least one char in the buffer.
    let last = cursor.slice(start, len).chars().last().unwrap();
    // [.-_] can't be the final character
    if !matches!(last, 'A'..='Z' | 'a'..='z' | '0'..='9') {
        return Err(Pep508Error {
            message: Pep508ErrorSource::String(format!(
                "Package name must end with an alphanumeric character, not `{last}`"
            )),
            start: cursor.pos() - last.len_utf8(),
            len: last.len_utf8(),
            input: cursor.to_string(),
        });
    }
    Ok(PackageName::from_str(cursor.slice(start, len)).unwrap())
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
                    message: Pep508ErrorSource::String(format!(
                        "Expected either `,` (separating extras) or `]` (ending the extras section), found `{other}`"
                    )),
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
        }
        // wsp* after the identifier
        cursor.eat_whitespace();

        // Add the parsed extra
        extras.push(
            ExtraName::from_str(&buffer)
                .expect("`ExtraName` validation should match PEP 508 parsing"),
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
            if cursor.peek_char().is_some_and(|c| matches!(c, ';' | '#')) {
                let mut cursor = cursor.clone();
                cursor.next();
                if cursor.peek_char().is_some_and(char::is_whitespace) {
                    break;
                }
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

    // If the requirement consists solely of a package name, and that name appears to be an archive,
    // treat it as a URL requirement, for consistency and security. (E.g., `requests-2.26.0.tar.gz`
    // is a valid Python package name, but we should treat it as a reference to a file.)
    //
    // See: https://github.com/pypa/pip/blob/111eed14b6e9fba7c78a5ec2b7594812d17b5d2b/src/pip/_internal/utils/filetypes.py#L8
    if requirement_kind.is_none() {
        if looks_like_archive(cursor.slice(name_start, name_end - name_start)) {
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

    if let Some((pos, char)) = cursor.next().filter(|(_, c)| *c != '#') {
        let message = if char == '#' {
            format!(
                r"Expected end of input or `;`, found `{char}`; comments must be preceded by a leading space"
            )
        } else if marker.is_none() {
            format!(r"Expected end of input or `;`, found `{char}`")
        } else {
            format!(r"Expected end of input, found `{char}`")
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
        extras: extras.into_boxed_slice(),
        version_or_url: requirement_kind,
        marker: marker.unwrap_or_default(),
        origin: None,
    })
}

#[cfg(feature = "rkyv")]
/// An [`rkyv`] implementation for [`Requirement`].
impl<T: Pep508Url + Display> rkyv::Archive for Requirement<T> {
    type Archived = rkyv::string::ArchivedString;
    type Resolver = rkyv::string::StringResolver;

    #[inline]
    fn resolve(&self, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        let as_str = self.to_string();
        rkyv::string::ArchivedString::resolve_from_str(&as_str, resolver, out);
    }
}

#[cfg(feature = "rkyv")]
impl<T: Pep508Url + Display, S> rkyv::Serialize<S> for Requirement<T>
where
    S: rkyv::rancor::Fallible + rkyv::ser::Allocator + rkyv::ser::Writer + ?Sized,
    S::Error: rkyv::rancor::Source,
{
    fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        let as_str = self.to_string();
        rkyv::string::ArchivedString::serialize_from_str(&as_str, serializer)
    }
}

#[cfg(feature = "rkyv")]
impl<T: Pep508Url + Display, D: rkyv::rancor::Fallible + ?Sized>
    rkyv::Deserialize<Requirement<T>, D> for rkyv::string::ArchivedString
{
    fn deserialize(&self, _deserializer: &mut D) -> Result<Requirement<T>, D::Error> {
        // SAFETY: We only serialize valid requirements.
        Ok(Requirement::<T>::from_str(self.as_str()).unwrap())
    }
}

#[cfg(test)]
mod tests {
    //! Half of these tests are copied from <https://github.com/pypa/packaging/pull/624>

    use std::env;
    use std::str::FromStr;

    use insta::assert_snapshot;
    use url::Url;

    use uv_normalize::{ExtraName, InvalidNameError, PackageName};
    use uv_pep440::{Operator, Version, VersionPattern, VersionSpecifier};

    use crate::cursor::Cursor;
    use crate::marker::{MarkerExpression, MarkerTree, MarkerValueVersion, parse};
    use crate::{
        MarkerOperator, MarkerValueString, Requirement, TracingReporter, VerbatimUrl, VersionOrUrl,
    };

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
        Expected package name starting with an alphanumeric character, found `_`
        _name
        ^"
        );
    }

    #[test]
    fn error_end() {
        assert_snapshot!(
            parse_pep508_err("name_"),
            @"
        Package name must end with an alphanumeric character, not `_`
        name_
            ^"
        );
    }

    #[test]
    fn basic_examples() {
        let input = r"requests[security,tests]==2.8.*,>=2.8.1 ; python_full_version < '2.7'";
        let requests = Requirement::<Url>::from_str(input).unwrap();
        assert_eq!(input, requests.to_string());
        let expected = Requirement {
            name: PackageName::from_str("requests").unwrap(),
            extras: Box::new([
                ExtraName::from_str("security").unwrap(),
                ExtraName::from_str("tests").unwrap(),
            ]),
            version_or_url: Some(VersionOrUrl::VersionSpecifier(
                [
                    VersionSpecifier::from_pattern(
                        Operator::Equal,
                        VersionPattern::wildcard(Version::new([2, 8])),
                    )
                    .unwrap(),
                    VersionSpecifier::from_pattern(
                        Operator::GreaterThanEqual,
                        VersionPattern::verbatim(Version::new([2, 8, 1])),
                    )
                    .unwrap(),
                ]
                .into_iter()
                .collect(),
            )),
            marker: MarkerTree::expression(MarkerExpression::Version {
                key: MarkerValueVersion::PythonFullVersion,
                specifier: VersionSpecifier::from_pattern(
                    Operator::LessThan,
                    "2.7".parse().unwrap(),
                )
                .unwrap(),
            }),
            origin: None,
        };
        assert_eq!(requests, expected);
    }

    #[test]
    fn leading_whitespace() {
        let numpy = Requirement::<Url>::from_str(" numpy").unwrap();
        assert_eq!(numpy.name.as_ref(), "numpy");
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
        assert_eq!(
            numpy.url.to_string(),
            "https://files.pythonhosted.org/packages/28/4a/46d9e65106879492374999e76eb85f87b15328e06bd1550668f79f7b18c6/numpy-1.26.4-cp312-cp312-win32.whl"
        );
        assert_eq!(*numpy.extras, []);
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
        assert_eq!(*numpy.extras, [ExtraName::from_str("dev").unwrap()]);
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
        assert_eq!(*numpy.extras, [ExtraName::from_str("dev").unwrap()]);
    }

    #[test]
    fn error_extras_eof1() {
        assert_snapshot!(
            parse_pep508_err("black["),
            @r#"
    Missing closing bracket (expected ']', found end of dependency specification)
    black[
         ^
    "#
        );
    }

    #[test]
    fn error_extras_eof2() {
        assert_snapshot!(
            parse_pep508_err("black[d"),
            @r#"
    Missing closing bracket (expected ']', found end of dependency specification)
    black[d
         ^
    "#
        );
    }

    #[test]
    fn error_extras_eof3() {
        assert_snapshot!(
            parse_pep508_err("black[d,"),
            @r#"
    Missing closing bracket (expected ']', found end of dependency specification)
    black[d,
         ^
    "#
        );
    }

    #[test]
    fn error_extras_illegal_start1() {
        assert_snapshot!(
            parse_pep508_err("black[ö]"),
            @r#"
    Expected an alphanumeric character starting the extra name, found `ö`
    black[ö]
          ^
    "#
        );
    }

    #[test]
    fn error_extras_illegal_start2() {
        assert_snapshot!(
            parse_pep508_err("black[_d]"),
            @r#"
    Expected an alphanumeric character starting the extra name, found `_`
    black[_d]
          ^
    "#
        );
    }

    #[test]
    fn error_extras_illegal_start3() {
        assert_snapshot!(
            parse_pep508_err("black[,]"),
            @r#"
    Expected either alphanumerical character (starting the extra name) or `]` (ending the extras section), found `,`
    black[,]
          ^
    "#
        );
    }

    #[test]
    fn error_extras_illegal_character() {
        assert_snapshot!(
            parse_pep508_err("black[jüpyter]"),
            @r#"
    Invalid character in extras name, expected an alphanumeric character, `-`, `_`, `.`, `,` or `]`, found `ü`
    black[jüpyter]
           ^
    "#
        );
    }

    #[test]
    fn error_extras1() {
        let numpy = Requirement::<Url>::from_str("black[d]").unwrap();
        assert_eq!(*numpy.extras, [ExtraName::from_str("d").unwrap()]);
    }

    #[test]
    fn error_extras2() {
        let numpy = Requirement::<Url>::from_str("black[d,jupyter]").unwrap();
        assert_eq!(
            *numpy.extras,
            [
                ExtraName::from_str("d").unwrap(),
                ExtraName::from_str("jupyter").unwrap(),
            ]
        );
    }

    #[test]
    fn empty_extras() {
        let black = Requirement::<Url>::from_str("black[]").unwrap();
        assert_eq!(*black.extras, []);
    }

    #[test]
    fn empty_extras_with_spaces() {
        let black = Requirement::<Url>::from_str("black[  ]").unwrap();
        assert_eq!(*black.extras, []);
    }

    #[test]
    fn error_extra_with_trailing_comma() {
        assert_snapshot!(
            parse_pep508_err("black[d,]"),
            @"
        Expected an alphanumeric character starting the extra name, found `]`
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
            @r#"
    Missing closing parenthesis (expected ')', found end of dependency specification)
    numpy ( >=1.19
          ^
    "#
        );
    }

    #[test]
    fn error_whats_that() {
        assert_snapshot!(
            parse_pep508_err("numpy % 1.16"),
            @r#"
    Expected one of `@`, `(`, `<`, `=`, `>`, `~`, `!`, `;`, found `%`
    numpy % 1.16
          ^
    "#
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
            extras: Box::new([]),
            marker: MarkerTree::TRUE,
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
        .unwrap()
        .unwrap();

        let mut a = MarkerTree::expression(MarkerExpression::Version {
            key: MarkerValueVersion::PythonVersion,
            specifier: VersionSpecifier::from_pattern(Operator::Equal, "2.7".parse().unwrap())
                .unwrap(),
        });
        let mut b = MarkerTree::expression(MarkerExpression::String {
            key: MarkerValueString::SysPlatform,
            operator: MarkerOperator::Equal,
            value: arcstr::literal!("win32"),
        });
        let mut c = MarkerTree::expression(MarkerExpression::String {
            key: MarkerValueString::OsName,
            operator: MarkerOperator::Equal,
            value: arcstr::literal!("linux"),
        });
        let d = MarkerTree::expression(MarkerExpression::String {
            key: MarkerValueString::ImplementationName,
            operator: MarkerOperator::Equal,
            value: arcstr::literal!("cpython"),
        });

        c.and(d);
        b.or(c);
        a.and(b);

        assert_eq!(a, actual);
    }

    #[test]
    fn name_and_marker() {
        Requirement::<Url>::from_str(r#"numpy; sys_platform == "win32" or (os_name == "linux" and implementation_name == 'cpython')"#).unwrap();
    }

    #[test]
    fn error_marker_incomplete1() {
        assert_snapshot!(
            parse_pep508_err(r"numpy; sys_platform"),
            @r#"
    Expected a valid marker operator (such as `>=` or `not in`), found ``
    numpy; sys_platform
                       ^
    "#
        );
    }

    #[test]
    fn error_marker_incomplete2() {
        assert_snapshot!(
            parse_pep508_err(r"numpy; sys_platform =="),
            @r#"
    Expected marker value, found end of dependency specification
    numpy; sys_platform ==
                          ^
    "#
        );
    }

    #[test]
    fn error_marker_incomplete3() {
        assert_snapshot!(
            parse_pep508_err(r#"numpy; sys_platform == "win32" or"#),
            @r#"
    Expected marker value, found end of dependency specification
    numpy; sys_platform == "win32" or
                                     ^
    "#
        );
    }

    #[test]
    fn error_marker_incomplete4() {
        assert_snapshot!(
            parse_pep508_err(r#"numpy; sys_platform == "win32" or (os_name == "linux""#),
            @r#"
    Expected ')', found end of dependency specification
    numpy; sys_platform == "win32" or (os_name == "linux"
                                      ^
    "#
        );
    }

    #[test]
    fn error_marker_incomplete5() {
        assert_snapshot!(
            parse_pep508_err(r#"numpy; sys_platform == "win32" or (os_name == "linux" and"#),
            @r#"
    Expected marker value, found end of dependency specification
    numpy; sys_platform == "win32" or (os_name == "linux" and
                                                             ^
    "#
        );
    }

    #[test]
    fn error_pep440() {
        assert_snapshot!(
            parse_pep508_err(r"numpy >=1.1.*"),
            @r#"
    Operator >= cannot be used with a wildcard version specifier
    numpy >=1.1.*
          ^^^^^^^
    "#
        );
    }

    #[test]
    fn error_no_name() {
        assert_snapshot!(
            parse_pep508_err(r"==0.0"),
            @r"
    Expected package name starting with an alphanumeric character, found `=`
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
            @r#"
    Expected either `,` (separating extras) or `]` (ending the extras section), found `b`
    name[bar baz]
             ^
    "#
        );
    }

    #[test]
    fn error_extra_comma_after_extras() {
        assert_snapshot!(
            parse_pep508_err(r"name[bar, baz,]"),
            @r#"
    Expected an alphanumeric character starting the extra name, found `]`
    name[bar, baz,]
                  ^
    "#
        );
    }

    #[test]
    fn error_extras_not_closed() {
        assert_snapshot!(
            parse_pep508_err(r"name[bar, baz >= 1.0"),
            @r#"
    Expected either `,` (separating extras) or `]` (ending the extras section), found `>`
    name[bar, baz >= 1.0
                  ^
    "#
        );
    }

    #[test]
    fn error_name_at_nothing() {
        assert_snapshot!(
            parse_pep508_err(r"name @"),
            @r#"
    Expected URL
    name @
          ^
    "#
        );
    }

    #[test]
    fn parse_name_with_star() {
        assert_snapshot!(
            parse_pep508_err("wheel-*.whl"),
            @r"
        Package name must end with an alphanumeric character, not `-`
        wheel-*.whl
             ^
        ");
        assert_snapshot!(
            parse_pep508_err("wheelѦ"),
            @r"
        Expected one of `@`, `(`, `<`, `=`, `>`, `~`, `!`, `;`, found `Ѧ`
        wheelѦ
             ^
        ");
    }

    #[test]
    fn test_error_invalid_marker_key() {
        assert_snapshot!(
            parse_pep508_err(r"name; invalid_name"),
            @r#"
    Expected a quoted string or a valid marker name, found `invalid_name`
    name; invalid_name
          ^^^^^^^^^^^^
    "#
        );
    }

    #[test]
    fn error_markers_invalid_order() {
        assert_snapshot!(
            parse_pep508_err("name; '3.7' <= invalid_name"),
            @r#"
    Expected a quoted string or a valid marker name, found `invalid_name`
    name; '3.7' <= invalid_name
                   ^^^^^^^^^^^^
    "#
        );
    }

    #[test]
    fn error_markers_notin() {
        assert_snapshot!(
            parse_pep508_err("name; '3.7' notin python_version"),
            @"
        Expected a valid marker operator (such as `>=` or `not in`), found `notin`
        name; '3.7' notin python_version
                    ^^^^^"
        );
    }

    #[test]
    fn error_missing_quote() {
        assert_snapshot!(
            parse_pep508_err("name; python_version == 3.10"),
            @"
        Expected a quoted string or a valid marker name, found `3.10`
        name; python_version == 3.10
                                ^^^^
        "
        );
    }

    #[test]
    fn error_markers_inpython_version() {
        assert_snapshot!(
            parse_pep508_err("name; '3.6'inpython_version"),
            @r#"
    Expected a valid marker operator (such as `>=` or `not in`), found `inpython_version`
    name; '3.6'inpython_version
               ^^^^^^^^^^^^^^^^
    "#
        );
    }

    #[test]
    fn error_markers_not_python_version() {
        assert_snapshot!(
            parse_pep508_err("name; '3.7' not python_version"),
            @"
        Expected `i`, found `p`
        name; '3.7' not python_version
                        ^"
        );
    }

    #[test]
    fn error_markers_invalid_operator() {
        assert_snapshot!(
            parse_pep508_err("name; '3.7' ~ python_version"),
            @"
        Expected a valid marker operator (such as `>=` or `not in`), found `~`
        name; '3.7' ~ python_version
                    ^"
        );
    }

    #[test]
    fn error_invalid_prerelease() {
        assert_snapshot!(
            parse_pep508_err("name==1.0.org1"),
            @r###"
    after parsing `1.0`, found `.org1`, which is not part of a valid version
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
            @r#"
    Expected one of `@`, `(`, `<`, `=`, `>`, `~`, `!`, `;`, found `1`
    name 1.0
         ^
    "#
        );
    }

    #[test]
    fn error_random_char() {
        assert_snapshot!(
            parse_pep508_err("name >= 1.0 #"),
            @r##"
    Trailing `#` is not allowed
    name >= 1.0 #
         ^^^^^^^^
    "##
        );
    }

    #[test]
    #[cfg(feature = "non-pep508-extensions")]
    fn error_invalid_extra_unnamed_url() {
        assert_snapshot!(
            parse_unnamed_err("/foo-3.0.0-py3-none-any.whl[d,]"),
            @r#"
    Expected an alphanumeric character starting the extra name, found `]`
    /foo-3.0.0-py3-none-any.whl[d,]
                                  ^
    "#
        );
    }

    /// Check that the relative path support feature toggle works.
    #[test]
    #[cfg(feature = "non-pep508-extensions")]
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
        assert_eq!(
            requirement.to_string(),
            "pytest ; python_full_version < '4.1'"
        );

        let requirement = Requirement::<Url>::from_str("pytest;'4.0'>=python_version").unwrap();
        assert_eq!(
            requirement.to_string(),
            "pytest ; python_full_version < '4.1'"
        );
    }

    #[test]
    #[cfg(feature = "non-pep508-extensions")]
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
                "Expected the path to end with `/Users/ferris/wheel-0.42.0.whl`, found `{}`",
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

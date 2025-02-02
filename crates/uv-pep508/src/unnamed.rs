use std::fmt::{Debug, Display, Formatter};
use std::hash::Hash;
use std::path::Path;
use std::str::FromStr;

use uv_fs::normalize_url_path;
use uv_normalize::ExtraName;

use crate::marker::parse;
use crate::{
    expand_env_vars, parse_extras_cursor, split_extras, split_scheme, strip_host, Cursor,
    MarkerEnvironment, MarkerTree, Pep508Error, Pep508ErrorSource, Pep508Url, Reporter,
    RequirementOrigin, Scheme, TracingReporter, VerbatimUrl, VerbatimUrlError,
};

/// An extension over [`Pep508Url`] that also supports parsing unnamed requirements, namely paths.
///
/// The error type is fixed to the same as the [`Pep508Url`] impl error.
pub trait UnnamedRequirementUrl: Pep508Url {
    /// Parse a URL from a relative or absolute path.
    fn parse_path(path: impl AsRef<Path>, working_dir: impl AsRef<Path>)
        -> Result<Self, Self::Err>;

    /// Parse a URL from an absolute path.
    fn parse_absolute_path(path: impl AsRef<Path>) -> Result<Self, Self::Err>;

    /// Parse a URL from a string.
    fn parse_unnamed_url(given: impl AsRef<str>) -> Result<Self, Self::Err>;

    /// Set the verbatim representation of the URL.
    #[must_use]
    fn with_given(self, given: impl AsRef<str>) -> Self;

    /// Return the original string as given by the user, if available.
    fn given(&self) -> Option<&str>;
}

impl UnnamedRequirementUrl for VerbatimUrl {
    fn parse_path(
        path: impl AsRef<Path>,
        working_dir: impl AsRef<Path>,
    ) -> Result<Self, VerbatimUrlError> {
        Self::from_path(path, working_dir)
    }

    fn parse_absolute_path(path: impl AsRef<Path>) -> Result<Self, Self::Err> {
        Self::from_absolute_path(path)
    }

    fn parse_unnamed_url(given: impl AsRef<str>) -> Result<Self, Self::Err> {
        Ok(Self::parse_url(given)?)
    }

    fn with_given(self, given: impl AsRef<str>) -> Self {
        self.with_given(given)
    }

    fn given(&self) -> Option<&str> {
        self.given()
    }
}

/// A PEP 508-like, direct URL dependency specifier without a package name.
///
/// In a `requirements.txt` file, the name of the package is optional for direct URL
/// dependencies. This isn't compliant with PEP 508, but is common in `requirements.txt`, which
/// is implementation-defined.
#[derive(Hash, Debug, Clone, Eq, PartialEq)]
pub struct UnnamedRequirement<Url: UnnamedRequirementUrl = VerbatimUrl> {
    /// The direct URL that defines the version specifier.
    pub url: Url,
    /// The list of extras such as `security`, `tests` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`.
    pub extras: Vec<ExtraName>,
    /// The markers such as `python_version > "3.8"` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`.
    /// Those are a nested and/or tree.
    pub marker: MarkerTree,
    /// The source file containing the requirement.
    pub origin: Option<RequirementOrigin>,
}

impl<Url: UnnamedRequirementUrl> UnnamedRequirement<Url> {
    /// Returns whether the markers apply for the given environment
    pub fn evaluate_markers(&self, env: &MarkerEnvironment, extras: &[ExtraName]) -> bool {
        self.evaluate_optional_environment(Some(env), extras)
    }

    /// Returns whether the markers apply for the given environment
    pub fn evaluate_optional_environment(
        &self,
        env: Option<&MarkerEnvironment>,
        extras: &[ExtraName],
    ) -> bool {
        self.marker.evaluate_optional_environment(env, extras)
    }

    /// Set the source file containing the requirement.
    #[must_use]
    pub fn with_origin(self, origin: RequirementOrigin) -> Self {
        Self {
            origin: Some(origin),
            ..self
        }
    }

    /// Parse a PEP 508-like direct URL requirement without a package name.
    pub fn parse(
        input: &str,
        working_dir: impl AsRef<Path>,
        reporter: &mut impl Reporter,
    ) -> Result<Self, Pep508Error<Url>> {
        parse_unnamed_requirement(
            &mut Cursor::new(input),
            Some(working_dir.as_ref()),
            reporter,
        )
    }
}

impl<Url: UnnamedRequirementUrl> Display for UnnamedRequirement<Url> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)?;
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
        if let Some(marker) = self.marker.contents() {
            write!(f, " ; {marker}")?;
        }
        Ok(())
    }
}

impl<Url: UnnamedRequirementUrl> FromStr for UnnamedRequirement<Url> {
    type Err = Pep508Error<Url>;

    /// Parse a PEP 508-like direct URL requirement without a package name.
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parse_unnamed_requirement(&mut Cursor::new(input), None, &mut TracingReporter)
    }
}

/// Parse a PEP 508-like direct URL specifier without a package name.
///
/// Unlike pip, we allow extras on URLs and paths.
fn parse_unnamed_requirement<Url: UnnamedRequirementUrl>(
    cursor: &mut Cursor,
    working_dir: Option<&Path>,
    reporter: &mut impl Reporter,
) -> Result<UnnamedRequirement<Url>, Pep508Error<Url>> {
    cursor.eat_whitespace();

    // Parse the URL itself, along with any extras.
    let (url, extras) = parse_unnamed_url::<Url>(cursor, working_dir)?;

    // wsp*
    cursor.eat_whitespace();
    // quoted_marker?
    let marker = if cursor.peek_char() == Some(';') {
        // Skip past the semicolon
        cursor.next();
        parse::parse_markers_cursor(cursor, reporter)?
    } else {
        None
    };
    // wsp*
    cursor.eat_whitespace();
    if let Some((pos, char)) = cursor.next() {
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

    Ok(UnnamedRequirement {
        url,
        extras,
        marker: marker.unwrap_or_default(),
        origin: None,
    })
}

/// Create a `VerbatimUrl` to represent the requirement, and extracts any extras at the end of the
/// URL, to comply with the non-PEP 508 extensions.
///
/// For example:
/// - `file:///home/ferris/project/scripts/...`
/// - `file:../editable/`
/// - `../editable/`
/// - `https://download.pytorch.org/whl/torch_stable.html`
fn preprocess_unnamed_url<Url: UnnamedRequirementUrl>(
    url: &str,
    #[cfg_attr(not(feature = "non-pep508-extensions"), allow(unused))] working_dir: Option<&Path>,
    cursor: &Cursor,
    start: usize,
    len: usize,
) -> Result<(Url, Vec<ExtraName>), Pep508Error<Url>> {
    // Split extras _before_ expanding the URL. We assume that the extras are not environment
    // variables. If we parsed the extras after expanding the URL, then the verbatim representation
    // of the URL itself would be ambiguous, since it would consist of the environment variable,
    // which would expand to _more_ than the URL.
    let (url, extras) = if let Some((url, extras)) = split_extras(url) {
        (url, Some(extras))
    } else {
        (url, None)
    };

    // Parse the extras, if provided.
    let extras = if let Some(extras) = extras {
        parse_extras_cursor(&mut Cursor::new(extras)).map_err(|err| Pep508Error {
            message: err.message,
            start: start + url.len() + err.start,
            len: err.len,
            input: cursor.to_string(),
        })?
    } else {
        vec![]
    };

    // Expand environment variables in the URL.
    let expanded = expand_env_vars(url);

    if let Some((scheme, path)) = split_scheme(&expanded) {
        match Scheme::parse(scheme) {
            // Ex) `file:///home/ferris/project/scripts/...`, `file://localhost/home/ferris/project/scripts/...`, or `file:../ferris/`
            Some(Scheme::File) => {
                // Strip the leading slashes, along with the `localhost` host, if present.
                let path = strip_host(path);

                // Transform, e.g., `/C:/Users/ferris/wheel-0.42.0.tar.gz` to `C:\Users\ferris\wheel-0.42.0.tar.gz`.
                let path = normalize_url_path(path);

                #[cfg(feature = "non-pep508-extensions")]
                if let Some(working_dir) = working_dir {
                    let url = Url::parse_path(path.as_ref(), working_dir)
                        .map_err(|err| Pep508Error {
                            message: Pep508ErrorSource::UrlError(err),
                            start,
                            len,
                            input: cursor.to_string(),
                        })?
                        .with_given(url);
                    return Ok((url, extras));
                }

                let url = Url::parse_absolute_path(path.as_ref())
                    .map_err(|err| Pep508Error {
                        message: Pep508ErrorSource::UrlError(err),
                        start,
                        len,
                        input: cursor.to_string(),
                    })?
                    .with_given(url);
                Ok((url, extras))
            }
            // Ex) `https://download.pytorch.org/whl/torch_stable.html`
            Some(_) => {
                // Ex) `https://download.pytorch.org/whl/torch_stable.html`
                let url = Url::parse_unnamed_url(expanded.as_ref())
                    .map_err(|err| Pep508Error {
                        message: Pep508ErrorSource::UrlError(err),
                        start,
                        len,
                        input: cursor.to_string(),
                    })?
                    .with_given(url);
                Ok((url, extras))
            }

            // Ex) `C:\Users\ferris\wheel-0.42.0.tar.gz`
            _ => {
                if let Some(working_dir) = working_dir {
                    let url = Url::parse_path(expanded.as_ref(), working_dir)
                        .map_err(|err| Pep508Error {
                            message: Pep508ErrorSource::UrlError(err),
                            start,
                            len,
                            input: cursor.to_string(),
                        })?
                        .with_given(url);
                    return Ok((url, extras));
                }

                let url = Url::parse_absolute_path(expanded.as_ref())
                    .map_err(|err| Pep508Error {
                        message: Pep508ErrorSource::UrlError(err),
                        start,
                        len,
                        input: cursor.to_string(),
                    })?
                    .with_given(url);
                Ok((url, extras))
            }
        }
    } else {
        // Ex) `../editable/`
        if let Some(working_dir) = working_dir {
            let url = Url::parse_path(expanded.as_ref(), working_dir)
                .map_err(|err| Pep508Error {
                    message: Pep508ErrorSource::UrlError(err),
                    start,
                    len,
                    input: cursor.to_string(),
                })?
                .with_given(url);
            return Ok((url, extras));
        }

        let url = Url::parse_absolute_path(expanded.as_ref())
            .map_err(|err| Pep508Error {
                message: Pep508ErrorSource::UrlError(err),
                start,
                len,
                input: cursor.to_string(),
            })?
            .with_given(url);
        Ok((url, extras))
    }
}

/// Like [`crate::parse_url`], but allows for extras to be present at the end of the URL, to comply
/// with the non-PEP 508 extensions.
///
/// When parsing, we eat characters until we see any of the following:
/// - A newline.
/// - A semicolon (marker) or hash (comment), _preceded_ by a space. We parse the URL until the last
///   non-whitespace character (inclusive).
/// - A semicolon (marker) or hash (comment) _followed_ by a space. We treat this as an error, since
///   the end of the URL is ambiguous.
///
/// URLs can include extras at the end, enclosed in square brackets.
///
/// For example:
/// - `https://download.pytorch.org/whl/torch_stable.html[dev]`
/// - `../editable[dev]`
/// - `https://download.pytorch.org/whl/torch_stable.html ; python_version > "3.8"`
/// - `https://download.pytorch.org/whl/torch_stable.html # this is a comment`
fn parse_unnamed_url<Url: UnnamedRequirementUrl>(
    cursor: &mut Cursor,
    working_dir: Option<&Path>,
) -> Result<(Url, Vec<ExtraName>), Pep508Error<Url>> {
    // wsp*
    cursor.eat_whitespace();

    // <URI_reference>
    let (start, len) = {
        let start = cursor.pos();
        let mut len = 0;
        let mut depth = 0u32;
        while let Some((_, c)) = cursor.next() {
            // If we see a line break, we're done.
            if matches!(c, '\r' | '\n') {
                break;
            }

            // Track the depth of brackets.
            if c == '[' {
                depth = depth.saturating_add(1);
            } else if c == ']' {
                depth = depth.saturating_sub(1);
            }

            // If we see top-level whitespace, check if it's followed by a semicolon or hash. If so,
            // end the URL at the last non-whitespace character.
            if depth == 0 && c.is_whitespace() {
                let mut cursor = cursor.clone();
                cursor.eat_whitespace();
                if matches!(cursor.peek_char(), None | Some(';' | '#')) {
                    break;
                }
            }

            len += c.len_utf8();

            // If we see a top-level semicolon or hash followed by whitespace, we're done.
            if depth == 0 && cursor.peek_char().is_some_and(|c| matches!(c, ';' | '#')) {
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

    let url = preprocess_unnamed_url(url, working_dir, cursor, start, len)?;

    Ok(url)
}

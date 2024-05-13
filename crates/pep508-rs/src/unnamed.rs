use std::fmt::{Display, Formatter};
use std::path::Path;
use std::str::FromStr;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use uv_fs::normalize_url_path;
use uv_normalize::ExtraName;

use crate::marker::parse_markers_cursor;
use crate::{
    expand_env_vars, parse_extras_cursor, split_extras, split_scheme, strip_host, Cursor,
    MarkerEnvironment, MarkerTree, Pep508Error, Pep508ErrorSource, Reporter, RequirementOrigin,
    Scheme, TracingReporter, VerbatimUrl, VerbatimUrlError,
};

/// A PEP 508-like, direct URL dependency specifier without a package name.
///
/// In a `requirements.txt` file, the name of the package is optional for direct URL
/// dependencies. This isn't compliant with PEP 508, but is common in `requirements.txt`, which
/// is implementation-defined.
#[derive(Hash, Debug, Clone, Eq, PartialEq)]
pub struct UnnamedRequirement {
    /// The direct URL that defines the version specifier.
    pub url: VerbatimUrl,
    /// The list of extras such as `security`, `tests` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`.
    pub extras: Vec<ExtraName>,
    /// The markers such as `python_version > "3.8"` in
    /// `requests [security,tests] >= 2.8.1, == 2.8.* ; python_version > "3.8"`.
    /// Those are a nested and/or tree.
    pub marker: Option<MarkerTree>,
    /// The source file containing the requirement.
    pub origin: Option<RequirementOrigin>,
}

impl UnnamedRequirement {
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
        if let Some(marker) = &self.marker {
            marker.evaluate_optional_environment(env, extras)
        } else {
            true
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

impl Display for UnnamedRequirement {
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
        if let Some(marker) = &self.marker {
            write!(f, " ; {}", marker)?;
        }
        Ok(())
    }
}

/// <https://github.com/serde-rs/serde/issues/908#issuecomment-298027413>
impl<'de> Deserialize<'de> for UnnamedRequirement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        FromStr::from_str(&s).map_err(de::Error::custom)
    }
}

/// <https://github.com/serde-rs/serde/issues/1316#issue-332908452>
impl Serialize for UnnamedRequirement {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl FromStr for UnnamedRequirement {
    type Err = Pep508Error<VerbatimUrl>;

    /// Parse a PEP 508-like direct URL requirement without a package name.
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parse_unnamed_requirement(&mut Cursor::new(input), None, &mut TracingReporter)
    }
}

impl UnnamedRequirement {
    /// Parse a PEP 508-like direct URL requirement without a package name.
    pub fn parse(
        input: &str,
        working_dir: impl AsRef<Path>,
        reporter: &mut impl Reporter,
    ) -> Result<Self, Pep508Error<VerbatimUrl>> {
        parse_unnamed_requirement(
            &mut Cursor::new(input),
            Some(working_dir.as_ref()),
            reporter,
        )
    }
}

/// Parse a PEP 508-like direct URL specifier without a package name.
///
/// Unlike pip, we allow extras on URLs and paths.
fn parse_unnamed_requirement(
    cursor: &mut Cursor,
    working_dir: Option<&Path>,
    reporter: &mut impl Reporter,
) -> Result<UnnamedRequirement, Pep508Error<VerbatimUrl>> {
    cursor.eat_whitespace();

    // Parse the URL itself, along with any extras.
    let (url, extras) = parse_unnamed_url(cursor, working_dir)?;
    let requirement_end = cursor.pos();

    // wsp*
    cursor.eat_whitespace();
    // quoted_marker?
    let marker = if cursor.peek_char() == Some(';') {
        // Skip past the semicolon
        cursor.next();
        Some(parse_markers_cursor(cursor, reporter)?)
    } else {
        None
    };
    // wsp*
    cursor.eat_whitespace();
    if let Some((pos, char)) = cursor.next() {
        if let Some(given) = url.given() {
            if given.ends_with(';') && marker.is_none() {
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

    Ok(UnnamedRequirement {
        url,
        extras,
        marker,
        origin: None,
    })
}

/// Create a `VerbatimUrl` to represent the requirement, and extracts any extras at the end of the
/// URL, to comply with the non-PEP 508 extensions.
fn preprocess_unnamed_url(
    url: &str,
    #[cfg_attr(not(feature = "non-pep508-extensions"), allow(unused))] working_dir: Option<&Path>,
    cursor: &Cursor,
    start: usize,
    len: usize,
) -> Result<(VerbatimUrl, Vec<ExtraName>), Pep508Error<VerbatimUrl>> {
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
                    let url = VerbatimUrl::parse_path(path.as_ref(), working_dir)
                        .with_given(url.to_string());
                    return Ok((url, extras));
                }

                let url = VerbatimUrl::parse_absolute_path(path.as_ref())
                    .map_err(|err| Pep508Error {
                        message: Pep508ErrorSource::<VerbatimUrl>::UrlError(err),
                        start,
                        len,
                        input: cursor.to_string(),
                    })?
                    .with_given(url.to_string());
                Ok((url, extras))
            }
            // Ex) `https://download.pytorch.org/whl/torch_stable.html`
            Some(_) => {
                // Ex) `https://download.pytorch.org/whl/torch_stable.html`
                let url = VerbatimUrl::parse_url(expanded.as_ref())
                    .map_err(|err| Pep508Error {
                        message: Pep508ErrorSource::<VerbatimUrl>::UrlError(VerbatimUrlError::Url(
                            err,
                        )),
                        start,
                        len,
                        input: cursor.to_string(),
                    })?
                    .with_given(url.to_string());
                Ok((url, extras))
            }

            // Ex) `C:\Users\ferris\wheel-0.42.0.tar.gz`
            _ => {
                if let Some(working_dir) = working_dir {
                    let url = VerbatimUrl::parse_path(expanded.as_ref(), working_dir)
                        .with_given(url.to_string());
                    return Ok((url, extras));
                }

                let url = VerbatimUrl::parse_absolute_path(expanded.as_ref())
                    .map_err(|err| Pep508Error {
                        message: Pep508ErrorSource::UrlError(err),
                        start,
                        len,
                        input: cursor.to_string(),
                    })?
                    .with_given(url.to_string());
                Ok((url, extras))
            }
        }
    } else {
        // Ex) `../editable/`
        if let Some(working_dir) = working_dir {
            let url =
                VerbatimUrl::parse_path(expanded.as_ref(), working_dir).with_given(url.to_string());
            return Ok((url, extras));
        }

        let url = VerbatimUrl::parse_absolute_path(expanded.as_ref())
            .map_err(|err| Pep508Error {
                message: Pep508ErrorSource::UrlError(err),
                start,
                len,
                input: cursor.to_string(),
            })?
            .with_given(url.to_string());
        Ok((url, extras))
    }
}

/// Like [`crate::parse_url`], but allows for extras to be present at the end of the URL, to comply
/// with the non-PEP 508 extensions.
///
/// For example:
/// - `https://download.pytorch.org/whl/torch_stable.html[dev]`
/// - `../editable[dev]`
fn parse_unnamed_url(
    cursor: &mut Cursor,
    working_dir: Option<&Path>,
) -> Result<(VerbatimUrl, Vec<ExtraName>), Pep508Error<VerbatimUrl>> {
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

    let url = preprocess_unnamed_url(url, working_dir, cursor, start, len)?;

    Ok(url)
}

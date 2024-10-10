use regex::Regex;
use serde::{de, Deserialize, Deserializer, Serialize};
use std::borrow::Cow;
use std::str::FromStr;
use std::sync::LazyLock;
use tracing::warn;

use uv_pep440::{VersionSpecifiers, VersionSpecifiersParseError};
use uv_pep508::{Pep508Error, Pep508Url, Requirement};

use crate::VerbatimParsedUrl;

/// Ex) `>=7.2.0<8.0.0`
static MISSING_COMMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d)([<>=~^!])").unwrap());
/// Ex) `!=~5.0`
static NOT_EQUAL_TILDE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!=~((?:\d\.)*\d)").unwrap());
/// Ex) `>=1.9.*`, `<3.4.*`
static INVALID_TRAILING_DOT_STAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(<=|>=|<|>)(\d+(\.\d+)*)\.\*").unwrap());
/// Ex) `!=3.0*`
static MISSING_DOT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d\.\d)+\*").unwrap());
/// Ex) `>=3.6,`
static TRAILING_COMMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",\s*$").unwrap());
/// Ex) `>dev`
static GREATER_THAN_DEV: LazyLock<Regex> = LazyLock::new(|| Regex::new(r">dev").unwrap());
/// Ex) `>=9.0.0a1.0`
static TRAILING_ZERO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+(\.\d)*(a|b|rc|post|dev)\d+)\.0").unwrap());

// Search and replace functions that fix invalid specifiers.
type FixUp = for<'a> fn(&'a str) -> Cow<'a, str>;

/// A list of fixups with a corresponding message about what was fixed.
static FIXUPS: &[(FixUp, &str)] = &[
    // Given `>=7.2.0<8.0.0`, rewrite to `>=7.2.0,<8.0.0`.
    (
        |input| MISSING_COMMA.replace_all(input, r"$1,$2"),
        "inserting missing comma",
    ),
    // Given `!=~5.0,>=4.12`, rewrite to `!=5.0.*,>=4.12`.
    (
        |input| NOT_EQUAL_TILDE.replace_all(input, r"!=${1}.*"),
        "replacing invalid tilde with wildcard",
    ),
    // Given `>=1.9.*`, rewrite to `>=1.9`.
    (
        |input| INVALID_TRAILING_DOT_STAR.replace_all(input, r"${1}${2}"),
        "removing star after comparison operator other than equal and not equal",
    ),
    // Given `!=3.0*`, rewrite to `!=3.0.*`.
    (
        |input| MISSING_DOT.replace_all(input, r"${1}.*"),
        "inserting missing dot",
    ),
    // Given `>=3.6,`, rewrite to `>=3.6`
    (
        |input| TRAILING_COMMA.replace_all(input, r"${1}"),
        "removing trailing comma",
    ),
    // Given `>dev`, rewrite to `>0.0.0dev`
    (
        |input| GREATER_THAN_DEV.replace_all(input, r">0.0.0dev"),
        "assuming 0.0.0dev",
    ),
    // Given `>=9.0.0a1.0`, rewrite to `>=9.0.0a1`
    (
        |input| TRAILING_ZERO.replace_all(input, r"${1}"),
        "removing trailing zero",
    ),
    (remove_stray_quotes, "removing stray quotes"),
];

// Given `>= 2.7'`, rewrite to `>= 2.7`
fn remove_stray_quotes(input: &str) -> Cow<'_, str> {
    /// Ex) `'>= 2.7'`, `>=3.6'`
    static STRAY_QUOTES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"['"]"#).unwrap());

    // make sure not to touch markers, which can have quotes (e.g. `python_version >= '3.7'`)
    match input.find(';') {
        Some(markers) => {
            let requirement = STRAY_QUOTES.replace_all(&input[..markers], "");
            format!("{}{}", requirement, &input[markers..]).into()
        }
        None => STRAY_QUOTES.replace_all(input, ""),
    }
}

fn parse_with_fixups<Err, T: FromStr<Err = Err>>(input: &str, type_name: &str) -> Result<T, Err> {
    match T::from_str(input) {
        Ok(requirement) => Ok(requirement),
        Err(err) => {
            let mut patched_input = input.to_string();
            let mut messages = Vec::new();
            for (fixup, message) in FIXUPS {
                let patched = fixup(patched_input.as_ref());
                if patched != patched_input {
                    messages.push(*message);

                    if let Ok(requirement) = T::from_str(&patched) {
                        warn!(
                            "Fixing invalid {type_name} by {} (before: `{input}`; after: `{patched}`)",
                            messages.join(", ")
                        );
                        return Ok(requirement);
                    }

                    patched_input = patched.to_string();
                }
            }

            Err(err)
        }
    }
}

/// Like [`Requirement`], but attempts to correct some common errors in user-provided requirements.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct LenientRequirement<T: Pep508Url = VerbatimParsedUrl>(Requirement<T>);

impl<T: Pep508Url> FromStr for LenientRequirement<T> {
    type Err = Pep508Error<T>;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Ok(Self(parse_with_fixups(input, "requirement")?))
    }
}

impl<T: Pep508Url> From<LenientRequirement<T>> for Requirement<T> {
    fn from(requirement: LenientRequirement<T>) -> Self {
        requirement.0
    }
}

/// Like [`VersionSpecifiers`], but attempts to correct some common errors in user-provided requirements.
///
/// For example, we turn `>=3.x.*` into `>=3.x`.
#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
pub struct LenientVersionSpecifiers(VersionSpecifiers);

impl FromStr for LenientVersionSpecifiers {
    type Err = VersionSpecifiersParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Ok(Self(parse_with_fixups(input, "version specifier")?))
    }
}

impl From<LenientVersionSpecifiers> for VersionSpecifiers {
    fn from(specifiers: LenientVersionSpecifiers) -> Self {
        specifiers.0
    }
}

impl<'de> Deserialize<'de> for LenientVersionSpecifiers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests;

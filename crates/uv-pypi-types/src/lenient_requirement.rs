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
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = LenientVersionSpecifiers;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                LenientVersionSpecifiers::from_str(v).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_pep440::VersionSpecifiers;
    use uv_pep508::Requirement;

    use crate::LenientVersionSpecifiers;

    use super::LenientRequirement;

    #[test]
    fn requirement_missing_comma() {
        let actual: Requirement = LenientRequirement::from_str("elasticsearch-dsl (>=7.2.0<8.0.0)")
            .unwrap()
            .into();
        let expected: Requirement =
            Requirement::from_str("elasticsearch-dsl (>=7.2.0,<8.0.0)").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn requirement_not_equal_tile() {
        let actual: Requirement = LenientRequirement::from_str("jupyter-core (!=~5.0,>=4.12)")
            .unwrap()
            .into();
        let expected: Requirement = Requirement::from_str("jupyter-core (!=5.0.*,>=4.12)").unwrap();
        assert_eq!(actual, expected);

        let actual: Requirement = LenientRequirement::from_str("jupyter-core (!=~5,>=4.12)")
            .unwrap()
            .into();
        let expected: Requirement = Requirement::from_str("jupyter-core (!=5.*,>=4.12)").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn requirement_greater_than_star() {
        let actual: Requirement = LenientRequirement::from_str("torch (>=1.9.*)")
            .unwrap()
            .into();
        let expected: Requirement = Requirement::from_str("torch (>=1.9)").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn requirement_missing_dot() {
        let actual: Requirement =
            LenientRequirement::from_str("pyzmq (>=2.7,!=3.0*,!=3.1*,!=3.2*)")
                .unwrap()
                .into();
        let expected: Requirement =
            Requirement::from_str("pyzmq (>=2.7,!=3.0.*,!=3.1.*,!=3.2.*)").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn requirement_trailing_comma() {
        let actual: Requirement = LenientRequirement::from_str("pyzmq >=3.6,").unwrap().into();
        let expected: Requirement = Requirement::from_str("pyzmq >=3.6").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn specifier_missing_comma() {
        let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=7.2.0<8.0.0")
            .unwrap()
            .into();
        let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=7.2.0,<8.0.0").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn specifier_not_equal_tile() {
        let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str("!=~5.0,>=4.12")
            .unwrap()
            .into();
        let expected: VersionSpecifiers = VersionSpecifiers::from_str("!=5.0.*,>=4.12").unwrap();
        assert_eq!(actual, expected);

        let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str("!=~5,>=4.12")
            .unwrap()
            .into();
        let expected: VersionSpecifiers = VersionSpecifiers::from_str("!=5.*,>=4.12").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn specifier_greater_than_star() {
        let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=1.9.*")
            .unwrap()
            .into();
        let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=1.9").unwrap();
        assert_eq!(actual, expected);

        let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=1.*").unwrap().into();
        let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=1").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn specifier_missing_dot() {
        let actual: VersionSpecifiers =
            LenientVersionSpecifiers::from_str(">=2.7,!=3.0*,!=3.1*,!=3.2*")
                .unwrap()
                .into();
        let expected: VersionSpecifiers =
            VersionSpecifiers::from_str(">=2.7,!=3.0.*,!=3.1.*,!=3.2.*").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn specifier_trailing_comma() {
        let actual: VersionSpecifiers =
            LenientVersionSpecifiers::from_str(">=3.6,").unwrap().into();
        let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=3.6").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn specifier_trailing_comma_trailing_space() {
        let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=3.6, ")
            .unwrap()
            .into();
        let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=3.6").unwrap();
        assert_eq!(actual, expected);
    }

    /// <https://pypi.org/simple/shellingham/?format=application/vnd.pypi.simple.v1+json>
    #[test]
    fn specifier_invalid_single_quotes() {
        let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">= '2.7'")
            .unwrap()
            .into();
        let expected: VersionSpecifiers = VersionSpecifiers::from_str(">= 2.7").unwrap();
        assert_eq!(actual, expected);
    }

    /// <https://pypi.org/simple/tensorflowonspark/?format=application/vnd.pypi.simple.v1+json>
    #[test]
    fn specifier_invalid_double_quotes() {
        let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=\"3.6\"")
            .unwrap()
            .into();
        let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=3.6").unwrap();
        assert_eq!(actual, expected);
    }

    /// <https://pypi.org/simple/celery/?format=application/vnd.pypi.simple.v1+json>
    #[test]
    fn specifier_multi_fix() {
        let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(
            ">=2.7, !=3.0.*, !=3.1.*, !=3.2.*, !=3.3.*, !=3.4.*,",
        )
        .unwrap()
        .into();
        let expected: VersionSpecifiers =
            VersionSpecifiers::from_str(">=2.7, !=3.0.*, !=3.1.*, !=3.2.*, !=3.3.*, !=3.4.*")
                .unwrap();
        assert_eq!(actual, expected);
    }

    /// <https://pypi.org/simple/wincertstore/?format=application/vnd.pypi.simple.v1+json>
    #[test]
    fn smaller_than_star() {
        let actual: VersionSpecifiers =
            LenientVersionSpecifiers::from_str(">=2.7,!=3.0.*,!=3.1.*,<3.4.*")
                .unwrap()
                .into();
        let expected: VersionSpecifiers =
            VersionSpecifiers::from_str(">=2.7,!=3.0.*,!=3.1.*,<3.4").unwrap();
        assert_eq!(actual, expected);
    }

    /// <https://pypi.org/simple/algoliasearch/?format=application/vnd.pypi.simple.v1+json>
    /// <https://pypi.org/simple/okta/?format=application/vnd.pypi.simple.v1+json>
    #[test]
    fn stray_quote() {
        let actual: VersionSpecifiers =
            LenientVersionSpecifiers::from_str(">=2.7, !=3.0.*, !=3.1.*', !=3.2.*, !=3.3.*'")
                .unwrap()
                .into();
        let expected: VersionSpecifiers =
            VersionSpecifiers::from_str(">=2.7, !=3.0.*, !=3.1.*, !=3.2.*, !=3.3.*").unwrap();
        assert_eq!(actual, expected);
        let actual: VersionSpecifiers =
            LenientVersionSpecifiers::from_str(">=3.6'").unwrap().into();
        let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=3.6").unwrap();
        assert_eq!(actual, expected);
    }

    /// <https://files.pythonhosted.org/packages/74/49/7349527cea7f708e7d3253ab6b32c9b5bdf84a57dde8fc265a33e6a4e662/boto3-1.2.0-py2.py3-none-any.whl>
    #[test]
    fn trailing_comma_after_quote() {
        let actual: Requirement = LenientRequirement::from_str("botocore>=1.3.0,<1.4.0',")
            .unwrap()
            .into();
        let expected: Requirement = Requirement::from_str("botocore>=1.3.0,<1.4.0").unwrap();
        assert_eq!(actual, expected);
    }

    /// <https://github.com/celery/celery/blob/6215f34d2675441ef2177bd850bf5f4b442e944c/requirements/default.txt#L1>
    #[test]
    fn greater_than_dev() {
        let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">dev").unwrap().into();
        let expected: VersionSpecifiers = VersionSpecifiers::from_str(">0.0.0dev").unwrap();
        assert_eq!(actual, expected);
    }

    /// <https://github.com/astral-sh/uv/issues/1798>
    #[test]
    fn trailing_alpha_zero() {
        let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=9.0.0a1.0")
            .unwrap()
            .into();
        let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=9.0.0a1").unwrap();
        assert_eq!(actual, expected);

        let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=9.0a1.0")
            .unwrap()
            .into();
        let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=9.0a1").unwrap();
        assert_eq!(actual, expected);

        let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=9a1.0")
            .unwrap()
            .into();
        let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=9a1").unwrap();
        assert_eq!(actual, expected);
    }

    /// <https://github.com/astral-sh/uv/issues/2551>
    #[test]
    fn stray_quote_preserve_marker() {
        let actual: Requirement =
            LenientRequirement::from_str("numpy >=1.19; python_version >= \"3.7\"")
                .unwrap()
                .into();
        let expected: Requirement =
            Requirement::from_str("numpy >=1.19; python_version >= \"3.7\"").unwrap();
        assert_eq!(actual, expected);

        let actual: Requirement =
            LenientRequirement::from_str("numpy \">=1.19\"; python_version >= \"3.7\"")
                .unwrap()
                .into();
        let expected: Requirement =
            Requirement::from_str("numpy >=1.19; python_version >= \"3.7\"").unwrap();
        assert_eq!(actual, expected);

        let actual: Requirement =
            LenientRequirement::from_str("'numpy' >=1.19\"; python_version >= \"3.7\"")
                .unwrap()
                .into();
        let expected: Requirement =
            Requirement::from_str("numpy >=1.19; python_version >= \"3.7\"").unwrap();
        assert_eq!(actual, expected);
    }
}

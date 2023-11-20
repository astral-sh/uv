use std::str::FromStr;

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{de, Deserialize, Deserializer, Serialize};

use pep440_rs::{Pep440Error, VersionSpecifiers};
use pep508_rs::{Pep508Error, Requirement};
use puffin_macros::warn_once;

/// Ex) `>=7.2.0<8.0.0`
static MISSING_COMMA: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d)([<>=~^!])").unwrap());
/// Ex) `!=~5.0`
static NOT_EQUAL_TILDE: Lazy<Regex> = Lazy::new(|| Regex::new(r"!=~((?:\d\.)*\d)").unwrap());
/// Ex) `>=1.9.*`
static GREATER_THAN_STAR: Lazy<Regex> = Lazy::new(|| Regex::new(r">=(\d+\.\d+)\.\*").unwrap());
/// Ex) `!=3.0*`
static MISSING_DOT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d\.\d)+\*").unwrap());
/// Ex) `>=3.6,`
static TRAILING_COMMA: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d\.\d)+,$").unwrap());

/// Regex to match the invalid specifier, replacement to fix it and message about was wrong and
/// fixed
static FIXUPS: &[(&Lazy<Regex>, &str, &str)] = &[
    // Given `>=7.2.0<8.0.0`, rewrite to `>=7.2.0,<8.0.0`.
    (&MISSING_COMMA, r"$1,$2", "Inserting missing comma"),
    // Given `!=~5.0,>=4.12`, rewrite to `!=5.0.*,>=4.12`.
    (
        &NOT_EQUAL_TILDE,
        r"!=${1}.*",
        "Replacing invalid tilde with wildcard",
    ),
    // Given `>=1.9.*`, rewrite to `>=1.9`.
    (
        &GREATER_THAN_STAR,
        r">=${1}",
        "Removing star after greater equal",
    ),
    // Given `!=3.0*`, rewrite to `!=3.0.*`.
    (&MISSING_DOT, r"${1}.*", "Inserting missing dot"),
    // Given `>=3.6,`, rewrite to `>=3.6`
    (&TRAILING_COMMA, r"${1}", "Removing trailing comma"),
];

/// Like [`Requirement`], but attempts to correct some common errors in user-provided requirements.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct LenientRequirement(Requirement);

impl FromStr for LenientRequirement {
    type Err = Pep508Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match Requirement::from_str(input) {
            Ok(requirement) => Ok(Self(requirement)),
            Err(err) => {
                for (matcher, replacement, message) in FIXUPS {
                    let patched = matcher.replace_all(input, *replacement);
                    if patched != input {
                        if let Ok(requirement) = Requirement::from_str(&patched) {
                            warn_once!(
                                "{message} to fix invalid requirement (before: `{input}`; after: `{patched}`)",
                            );
                            return Ok(Self(requirement));
                        }
                    }
                }

                Err(err)
            }
        }
    }
}

impl From<LenientRequirement> for Requirement {
    fn from(requirement: LenientRequirement) -> Self {
        requirement.0
    }
}

/// Like [`VersionSpecifiers`], but attempts to correct some common errors in user-provided requirements.
///
/// For example, we turn `>=3.x.*` into `>=3.x`.
#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
pub struct LenientVersionSpecifiers(VersionSpecifiers);

impl FromStr for LenientVersionSpecifiers {
    type Err = Pep440Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match VersionSpecifiers::from_str(input) {
            Ok(specifiers) => Ok(Self(specifiers)),
            Err(err) => {
                for (matcher, replacement, message) in FIXUPS {
                    let patched = matcher.replace_all(input, *replacement);
                    if patched != input {
                        if let Ok(specifiers) = VersionSpecifiers::from_str(&patched) {
                            warn_once!(
                                "{message} to fix invalid specifiers (before: `{input}`; after: `{patched}`)",
                            );
                            return Ok(Self(specifiers));
                        }
                    }
                }

                Err(err)
            }
        }
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
mod tests {
    use pep440_rs::VersionSpecifiers;
    use std::str::FromStr;

    use crate::LenientVersionSpecifiers;
    use pep508_rs::Requirement;

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
}

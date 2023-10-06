//! WIP Draft for a poetry/cargo like, modern dependency specification
//!
//! This still needs
//! * Better VersionSpecifier (e.g. allowing `^1.19`) and it's sentry integration
//! * PEP 440/PEP 508 translation
//! * a json schema

#![cfg(feature = "modern")]

use crate::MarkerValue::QuotedString;
use crate::{MarkerExpression, MarkerOperator, MarkerTree, MarkerValue, Requirement, VersionOrUrl};
use anyhow::{bail, format_err, Context};
use once_cell::sync::Lazy;
use pep440_rs::{Operator, Pep440Error, Version, VersionSpecifier, VersionSpecifiers};
use regex::Regex;
use serde::{de, Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use url::Url;

/// Shared fields for version/git/file/path/url dependencies (`optional`, `extras`, `markers`)
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize)]
pub struct RequirementModernCommon {
    /// Whether this is an optional dependency. This is inverted from PEP 508 extras where the
    /// requirements has the extras attached, as here the extras has a table where each extra
    /// says which optional dependencies it activates
    #[serde(default)]
    pub optional: bool,
    /// The list of extras <https://packaging.python.org/en/latest/tutorials/installing-packages/#installing-extras>
    pub extras: Option<Vec<String>>,
    /// The list of markers <https://peps.python.org/pep-0508/#environment-markers>.
    /// Note that this will not accept extras.
    ///
    /// TODO: Deserialize into `MarkerTree` that does not accept the extras key
    pub markers: Option<String>,
}

/// Instead of only PEP 440 specifier, you can also set a single version (exact) or TODO use
/// the semver caret
#[derive(Eq, PartialEq, Debug, Clone, Serialize)]
pub enum VersionSpecifierModern {
    /// e.g. `4.12.1-beta.1`
    Version(Version),
    /// e.g. `== 4.12.1-beta.1` or `>=3.8,<4.0`
    VersionSpecifier(VersionSpecifiers),
}

impl VersionSpecifierModern {
    /// `4.12.1-beta.1` -> `== 4.12.1-beta.1`
    /// `== 4.12.1-beta.1` -> `== 4.12.1-beta.1`
    /// `>=3.8,<4.0` -> `>=3.8,<4.0`
    /// TODO: `^1.19` -> `>=1.19,<2.0`
    pub fn to_pep508_specifier(&self) -> VersionSpecifiers {
        match self {
            // unwrapping is safe here because we're using Operator::Equal
            VersionSpecifierModern::Version(version) => {
                [VersionSpecifier::new(Operator::Equal, version.clone(), false).unwrap()]
                    .into_iter()
                    .collect()
            }
            VersionSpecifierModern::VersionSpecifier(version_specifiers) => {
                version_specifiers.clone()
            }
        }
    }
}

impl FromStr for VersionSpecifierModern {
    /// TODO: Modern needs it's own error type
    type Err = Pep440Error;

    /// dispatching between just a version and a version specifier set
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // If it starts with
        if s.trim_start().starts_with(|x: char| x.is_ascii_digit()) {
            Ok(Self::Version(Version::from_str(s).map_err(|err| {
                // TODO: Fix this in pep440_rs
                Pep440Error {
                    message: err,
                    line: s.to_string(),
                    start: 0,
                    width: 1,
                }
            })?))
        } else if s.starts_with('^') {
            todo!("TODO caret operator is not supported yet");
        } else {
            Ok(Self::VersionSpecifier(VersionSpecifiers::from_str(s)?))
        }
    }
}

/// https://github.com/serde-rs/serde/issues/908#issuecomment-298027413
impl<'de> Deserialize<'de> for VersionSpecifierModern {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        FromStr::from_str(&s).map_err(de::Error::custom)
    }
}

/// WIP Draft for a poetry/cargo like, modern dependency specification
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RequirementModern {
    /// e.g. `numpy = "1.24.1"`
    Dependency(VersionSpecifierModern),
    /// e.g. `numpy = { version = "1.24.1" }` or `django-anymail = { version = "1.24.1", extras = ["sendgrid"], optional = true }`
    LongDependency {
        /// e.g. `1.2.3.beta1`
        version: VersionSpecifierModern,
        #[serde(flatten)]
        #[allow(missing_docs)]
        common: RequirementModernCommon,
    },
    /// e.g. `tqdm = { git = "https://github.com/tqdm/tqdm", rev = "0bb91857eca0d4aea08f66cf1c8949abe0cd6b7a" }`
    GitDependency {
        /// URL of the git repository e.g. `https://github.com/tqdm/tqdm`
        git: Url,
        /// The git branch to use
        branch: Option<String>,
        /// The git revision to use. Can be the short revision (`0bb9185`) or the long revision
        /// (`0bb91857eca0d4aea08f66cf1c8949abe0cd6b7a`)
        rev: Option<String>,
        #[serde(flatten)]
        #[allow(missing_docs)]
        common: RequirementModernCommon,
    },
    /// e.g. `tqdm = { file = "tqdm-4.65.0-py3-none-any.whl" }`
    FileDependency {
        /// Path to a source distribution (e.g. `tqdm-4.65.0.tar.gz`) or wheel (e.g. `tqdm-4.65.0-py3-none-any.whl`)
        file: String,
        #[serde(flatten)]
        #[allow(missing_docs)]
        common: RequirementModernCommon,
    },
    /// Path to a directory with source distributions and/or wheels e.g.
    /// `scilib_core = { path = "build_wheels/scilib_core/" }`.
    ///
    /// Use this option if you e.g. have multiple platform platform dependent wheels or want to
    /// have a fallback to a source distribution for you wheel.
    PathDependency {
        /// e.g. `dist/`, `target/wheels` or `vendored`
        path: String,
        #[serde(flatten)]
        #[allow(missing_docs)]
        common: RequirementModernCommon,
    },
    /// e.g. `jax = { url = "https://storage.googleapis.com/jax-releases/cuda112/jaxlib-0.1.64+cuda112-cp39-none-manylinux2010_x86_64.whl" }`
    UrlDependency {
        /// URL to a source distribution or wheel. The file available there must be named
        /// appropriately for a source distribution or a wheel.
        url: Url,
        #[serde(flatten)]
        #[allow(missing_docs)]
        common: RequirementModernCommon,
    },
}

/// Adopted from the grammar at <https://peps.python.org/pep-0508/#extras>
static EXTRA_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-zA-Z0-9]([-_.]*[a-zA-Z0-9])*$").unwrap());

impl RequirementModern {
    /// Check the things that serde doesn't check, namely that extra names are valid
    pub fn check(&self) -> anyhow::Result<()> {
        match self {
            Self::LongDependency { common, .. }
            | Self::GitDependency { common, .. }
            | Self::FileDependency { common, .. }
            | Self::PathDependency { common, .. }
            | Self::UrlDependency { common, .. } => {
                if let Some(extras) = &common.extras {
                    for extra in extras {
                        if !EXTRA_REGEX.is_match(extra) {
                            bail!("Not a valid extra name: '{}'", extra)
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// WIP Converts the modern format to PEP 508
    pub fn to_pep508(
        &self,
        name: &str,
        extras: &HashMap<String, Vec<String>>,
    ) -> Result<Requirement, anyhow::Error> {
        let default = RequirementModernCommon {
            optional: false,
            extras: None,
            markers: None,
        };

        let common = match self {
            RequirementModern::Dependency(..) => &default,
            RequirementModern::LongDependency { common, .. }
            | RequirementModern::GitDependency { common, .. }
            | RequirementModern::FileDependency { common, .. }
            | RequirementModern::PathDependency { common, .. }
            | RequirementModern::UrlDependency { common, .. } => common,
        };

        let marker = if common.optional {
            // invert the extras table from the modern format
            // extra1 -> optional_dep1, optional_dep2, ...
            // to the PEP 508 format
            // optional_dep1; extra == "extra1" or extra == "extra2"
            let dep_markers = extras
                .iter()
                .filter(|(_marker, dependencies)| dependencies.contains(&name.to_string()))
                .map(|(marker, _dependencies)| {
                    MarkerTree::Expression(MarkerExpression {
                        l_value: MarkerValue::Extra,
                        operator: MarkerOperator::Equal,
                        r_value: QuotedString(marker.to_string()),
                    })
                })
                .collect();
            // any of these extras activates the dependency -> or clause
            let dep_markers = MarkerTree::Or(dep_markers);
            let joined_marker = if let Some(user_markers) = &common.markers {
                let user_markers = MarkerTree::from_str(user_markers)
                    .context("TODO: parse this in serde already")?;
                // but the dependency needs to be activated and match the other markers
                // -> and clause
                MarkerTree::And(vec![user_markers, dep_markers])
            } else {
                dep_markers
            };
            Some(joined_marker)
        } else {
            None
        };

        if let Some(extras) = &common.extras {
            debug_assert!(extras.iter().all(|extra| EXTRA_REGEX.is_match(extra)));
        }

        let version_or_url = match self {
            RequirementModern::Dependency(version) => {
                VersionOrUrl::VersionSpecifier(version.to_pep508_specifier())
            }
            RequirementModern::LongDependency { version, .. } => {
                VersionOrUrl::VersionSpecifier(version.to_pep508_specifier())
            }
            RequirementModern::GitDependency {
                git, branch, rev, ..
            } => {
                // TODO: Read https://peps.python.org/pep-0440/#direct-references properly
                // set_scheme doesn't like us adding `git+` to https, therefore this hack
                let mut url =
                    Url::parse(&format!("git+{}", git)).expect("TODO: Better url validation");
                match (branch, rev) {
                    (Some(_branch), Some(_rev)) => {
                        bail!("You can set both branch and rev (for {})", name)
                    }
                    (Some(branch), None) => url.set_path(&format!("{}@{}", url.path(), branch)),
                    (None, Some(rev)) => url.set_path(&format!("{}@{}", url.path(), rev)),
                    (None, None) => {}
                }

                VersionOrUrl::Url(url)
            }
            RequirementModern::FileDependency { file, .. } => VersionOrUrl::Url(
                Url::from_file_path(file)
                    .map_err(|()| format_err!("File must be absolute (for {})", name))?,
            ),
            RequirementModern::PathDependency { path, .. } => VersionOrUrl::Url(
                Url::from_directory_path(path)
                    .map_err(|()| format_err!("Path must be absolute (for {})", name))?,
            ),
            RequirementModern::UrlDependency { url, .. } => VersionOrUrl::Url(url.clone()),
        };

        Ok(Requirement {
            name: name.to_string(),
            extras: common.extras.clone(),
            version_or_url: Some(version_or_url),
            marker,
        })
    }
}

#[cfg(test)]
mod test {
    use crate::modern::{RequirementModern, VersionSpecifierModern};
    use crate::Requirement;
    use indoc::indoc;
    use pep440_rs::VersionSpecifiers;
    use serde::Deserialize;
    use std::collections::{BTreeMap, HashMap};

    use std::str::FromStr;

    #[test]
    fn test_basic() {
        let deps: HashMap<String, RequirementModern> =
            toml::from_str(r#"numpy = "==1.19""#).unwrap();
        assert_eq!(
            deps["numpy"],
            RequirementModern::Dependency(VersionSpecifierModern::VersionSpecifier(
                VersionSpecifiers::from_str("==1.19").unwrap()
            ))
        );
        assert_eq!(
            deps["numpy"].to_pep508("numpy", &HashMap::new()).unwrap(),
            Requirement::from_str("numpy== 1.19").unwrap()
        );
    }

    #[test]
    fn test_conversion() {
        #[derive(Deserialize)]
        struct PyprojectToml {
            // BTreeMap to keep the order
            #[serde(rename = "modern-dependencies")]
            modern_dependencies: BTreeMap<String, RequirementModern>,
            extras: HashMap<String, Vec<String>>,
        }

        let pyproject_toml = indoc! {r#"
            [modern-dependencies]
            pydantic = "1.10.5"
            numpy = ">=1.24.2, <2.0.0"
            pandas = { version = ">=1.5.3, <2.0.0" }
            flask = { version = "2.2.3 ", extras = ["dotenv"], optional = true }
            tqdm = { git = "https://github.com/tqdm/tqdm", rev = "0bb91857eca0d4aea08f66cf1c8949abe0cd6b7a" }
            jax = { url = "https://storage.googleapis.com/jax-releases/cuda112/jaxlib-0.1.64+cuda112-cp39-none-manylinux2010_x86_64.whl" }
            zstandard = { file = "/home/ferris/wheels/zstandard/zstandard-0.20.0.tar.gz" }
            h5py = { path = "/home/ferris/wheels/h5py/" }

            [extras]
            internet = ["flask"]
            "#
        };

        let deps: PyprojectToml = toml::from_str(pyproject_toml).unwrap();

        let actual: Vec<String> = deps
            .modern_dependencies
            .iter()
            .map(|(name, spec)| spec.to_pep508(name, &deps.extras).unwrap().to_string())
            .collect();
        let expected: Vec<String> = vec![
            "flask[dotenv] ==2.2.3 ; extra == 'internet'".to_string(),
            "h5py @ file:///home/ferris/wheels/h5py/".to_string(),
            "jax @ https://storage.googleapis.com/jax-releases/cuda112/jaxlib-0.1.64+cuda112-cp39-none-manylinux2010_x86_64.whl".to_string(),
            "numpy >=1.24.2, <2.0.0".to_string(),
            "pandas >=1.5.3, <2.0.0".to_string(),
            "pydantic ==1.10.5".to_string(),
            "tqdm @ git+https://github.com/tqdm/tqdm@0bb91857eca0d4aea08f66cf1c8949abe0cd6b7a".to_string(),
            "zstandard @ file:///home/ferris/wheels/zstandard/zstandard-0.20.0.tar.gz".to_string()
        ];
        assert_eq!(actual, expected)
    }
}

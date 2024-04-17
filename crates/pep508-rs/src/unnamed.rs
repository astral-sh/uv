use crate::{Cursor, MarkerEnvironment, MarkerTree, Pep508Error, VerbatimUrl};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::str::FromStr;
use uv_normalize::ExtraName;

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
#[cfg(feature = "serde")]
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
#[cfg(feature = "serde")]
impl Serialize for UnnamedRequirement {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl UnnamedRequirement {
    /// Returns whether the markers apply for the given environment
    pub fn evaluate_markers(&self, env: &MarkerEnvironment, extras: &[ExtraName]) -> bool {
        if let Some(marker) = &self.marker {
            marker.evaluate(env, extras)
        } else {
            true
        }
    }
}

impl FromStr for UnnamedRequirement {
    type Err = Pep508Error;

    /// Parse a PEP 508-like direct URL requirement without a package name.
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        crate::parse_unnamed_requirement(&mut Cursor::new(input), None)
    }
}

impl UnnamedRequirement {
    /// Parse a PEP 508-like direct URL requirement without a package name.
    pub fn parse(input: &str, working_dir: impl AsRef<Path>) -> Result<Self, Pep508Error> {
        crate::parse_unnamed_requirement(&mut Cursor::new(input), Some(working_dir.as_ref()))
    }
}

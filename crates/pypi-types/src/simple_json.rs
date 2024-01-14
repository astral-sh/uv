use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use pep440_rs::{VersionSpecifiers, VersionSpecifiersParseError};

use crate::lenient_requirement::LenientVersionSpecifiers;

#[derive(Debug, Clone, Deserialize)]
pub struct SimpleJson {
    pub files: Vec<File>,
}

/// A single (remote) file belonging to a package, either a wheel or a source distribution.
///
/// <https://peps.python.org/pep-0691/#project-detail>
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct File {
    // Non-PEP 691-compliant alias used by PyPI.
    #[serde(alias = "data_dist_info_metadata")]
    pub dist_info_metadata: Option<DistInfoMetadata>,
    pub filename: String,
    pub hashes: Hashes,
    /// There are a number of invalid specifiers on pypi, so we first try to parse it into a [`VersionSpecifiers`]
    /// according to spec (PEP 440), then a [`LenientVersionSpecifiers`] with fixup for some common problems and if this
    /// still fails, we skip the file when creating a version map.
    #[serde(default, deserialize_with = "deserialize_version_specifiers_lenient")]
    pub requires_python: Option<Result<VersionSpecifiers, VersionSpecifiersParseError>>,
    pub size: Option<u64>,
    pub upload_time: Option<DateTime<Utc>>,
    pub url: String,
    pub yanked: Option<Yanked>,
}

fn deserialize_version_specifiers_lenient<'de, D>(
    deserializer: D,
) -> Result<Option<Result<VersionSpecifiers, VersionSpecifiersParseError>>, D::Error>
where
    D: Deserializer<'de>,
{
    let maybe_string: Option<String> = Option::deserialize(deserializer)?;
    let Some(string) = maybe_string else {
        return Ok(None);
    };
    Ok(Some(
        LenientVersionSpecifiers::from_str(&string).map(Into::into),
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DistInfoMetadata {
    Bool(bool),
    Hashes(Hashes),
}

impl DistInfoMetadata {
    pub fn is_available(&self) -> bool {
        match self {
            Self::Bool(is_available) => *is_available,
            Self::Hashes(_) => true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Yanked {
    Bool(bool),
    Reason(String),
}

impl Yanked {
    pub fn is_yanked(&self) -> bool {
        match self {
            Yanked::Bool(is_yanked) => *is_yanked,
            Yanked::Reason(_) => true,
        }
    }
}

/// A dictionary mapping a hash name to a hex encoded digest of the file.
///
/// PEP 691 says multiple hashes can be included and the interpretation is left to the client, we
/// only support SHA 256 atm.
#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Default, Serialize, Deserialize)]
pub struct Hashes {
    pub sha256: Option<String>,
}

impl Hashes {
    /// Format as `<algorithm>:<hash>`.
    ///
    /// Currently limited to SHA256.
    pub fn to_string(&self) -> Option<String> {
        self.sha256
            .as_ref()
            .map(|sha256| format!("sha256:{sha256}"))
    }
}

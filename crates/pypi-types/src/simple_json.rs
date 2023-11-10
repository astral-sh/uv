use pep440_rs::VersionSpecifiers;
use serde::{de, Deserialize, Deserializer, Serialize};
use std::str::FromStr;

use crate::lenient_requirement::LenientVersionSpecifiers;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleJson {
    pub files: Vec<File>,
    pub meta: Meta,
    pub name: String,
    pub versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct File {
    pub core_metadata: Metadata,
    pub data_dist_info_metadata: Metadata,
    pub filename: String,
    pub hashes: Hashes,
    /// Note: Deserialized with [`LenientVersionSpecifiers`] since there are a number of invalid
    /// versions on pypi
    #[serde(deserialize_with = "deserialize_version_specifiers_lenient")]
    pub requires_python: Option<VersionSpecifiers>,
    pub size: usize,
    pub upload_time: String,
    pub url: String,
    pub yanked: Yanked,
}

fn deserialize_version_specifiers_lenient<'de, D>(
    deserializer: D,
) -> Result<Option<VersionSpecifiers>, D::Error>
where
    D: Deserializer<'de>,
{
    let maybe_string: Option<String> = Option::deserialize(deserializer)?;
    let Some(string) = maybe_string else {
        return Ok(None);
    };
    let lenient = LenientVersionSpecifiers::from_str(&string).map_err(de::Error::custom)?;
    Ok(Some(lenient.into()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Metadata {
    Bool(bool),
    Hashes(Hashes),
}

impl Metadata {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hashes {
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Meta {
    #[serde(rename = "_last-serial")]
    pub last_serial: i64,
    pub api_version: String,
}

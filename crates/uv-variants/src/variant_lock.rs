use std::str::FromStr;

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer};
use thiserror::Error;

use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::{UnnamedRequirementUrl, VariantFeature, VariantNamespace, VariantValue};
use uv_pypi_types::VerbatimParsedUrl;

#[derive(Debug, Error)]
pub enum VariantLockError {
    #[error("Invalid resolved requirement format: {0}")]
    InvalidResolvedFormat(String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VariantLock {
    pub metadata: VariantLockMetadata,
    pub provider: Vec<VariantLockProvider>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VariantLockMetadata {
    pub created_by: String,
    pub version: Version,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VariantLockProvider {
    pub resolved: Vec<VariantLockResolved>,
    pub plugin_api: Option<String>,
    pub namespace: VariantNamespace,
    pub properties: IndexMap<VariantFeature, Vec<VariantValue>>,
}

/// A resolved requirement in the form `<name>==<version>` or `<name> @ <url>`
#[derive(Debug, Clone)]
pub enum VariantLockResolved {
    Version(PackageName, Version),
    Url(PackageName, Box<VerbatimParsedUrl>),
}

impl VariantLockResolved {
    pub fn name(&self) -> &PackageName {
        match self {
            Self::Version(name, _) => name,
            Self::Url(name, _) => name,
        }
    }
}

impl FromStr for VariantLockResolved {
    type Err = VariantLockError;

    fn from_str(resolved: &str) -> Result<Self, Self::Err> {
        if let Some((name, version)) = resolved.split_once("==") {
            Ok(Self::Version(
                PackageName::from_str(name.trim())
                    .map_err(|_| VariantLockError::InvalidResolvedFormat(resolved.to_string()))?,
                Version::from_str(version.trim())
                    .map_err(|_| VariantLockError::InvalidResolvedFormat(resolved.to_string()))?,
            ))
        } else if let Some((name, url)) = resolved.split_once(" @ ") {
            Ok(Self::Url(
                PackageName::from_str(name.trim())
                    .map_err(|_| VariantLockError::InvalidResolvedFormat(resolved.to_string()))?,
                Box::new(
                    VerbatimParsedUrl::parse_unnamed_url(url.trim()).map_err(|_| {
                        VariantLockError::InvalidResolvedFormat(resolved.to_string())
                    })?,
                ),
            ))
        } else {
            Err(VariantLockError::InvalidResolvedFormat(
                resolved.to_string(),
            ))
        }
    }
}

impl<'de> Deserialize<'de> for VariantLockResolved {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

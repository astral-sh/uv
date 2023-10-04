use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::PypiClientError;
use crate::PypiClient;

impl PypiClient {
    pub async fn simple(
        &self,
        package_name: impl AsRef<str>,
    ) -> Result<SimpleJson, PypiClientError> {
        // Format the URL for PyPI.
        let mut url = self.registry.join("simple")?;
        url.path_segments_mut().unwrap().push(package_name.as_ref());
        url.path_segments_mut().unwrap().push("");
        url.set_query(Some("format=application/vnd.pypi.simple.v1+json"));

        // Fetch from the registry.
        let text = self.simple_impl(package_name, &url).await?;
        serde_json::from_str(&text)
            .map_err(move |e| PypiClientError::from_json_err(e, url.to_string()))
    }

    async fn simple_impl(
        &self,
        package_name: impl AsRef<str>,
        url: &Url,
    ) -> Result<String, PypiClientError> {
        Ok(self
            .client
            .get(url.clone())
            .send()
            .await?
            .error_for_status()
            .map_err(|err| {
                if err.status() == Some(StatusCode::NOT_FOUND) {
                    PypiClientError::PackageNotFound(
                        (*self.registry).clone(),
                        package_name.as_ref().to_string(),
                    )
                } else {
                    PypiClientError::RequestError(err)
                }
            })?
            .text()
            .await?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimpleJson {
    files: Vec<File>,
    meta: Meta,
    name: String,
    versions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct File {
    core_metadata: Metadata,
    data_dist_info_metadata: Metadata,
    filename: String,
    hashes: Hashes,
    requires_python: Option<String>,
    size: i64,
    upload_time: String,
    url: String,
    yanked: Yanked,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Metadata {
    Bool(bool),
    Hashes(Hashes),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Yanked {
    Bool(bool),
    Reason(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Hashes {
    sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Meta {
    #[serde(rename = "_last-serial")]
    last_serial: i64,
    api_version: String,
}


/// The metadata for a single package, including the pubishing versions and their artifacts.
///
/// In npm, this is referred to as a "packument", which is a portmanteau of "package" and
/// "document".
#[derive(Debug, Serialize, Deserialize)]
pub struct PackageDocument {
    pub meta: Meta,
    pub artifacts: Vec<ArtifactInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub version: String,
}

impl Default for Meta {
    fn default() -> Self {
        Self {
            // According to the spec, clients SHOULD introspect each response for the repository
            // version; if it doesn't exist, clients MUST assume that it is version 1.0.
            version: "1.0".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactInfo {
    pub name: String,
    pub url: Url,
    pub hash: Option<String>,
    pub requires_python: Option<String>,
    pub dist_info_metadata: DistInfoMetadata,
    pub yanked: Yanked,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Option<RawDistInfoMetadata>")]
pub struct DistInfoMetadata {
    pub available: bool,
    pub hash: Option<String>,
}

impl From<Option<RawDistInfoMetadata>> for DistInfoMetadata {
    fn from(maybe_raw: Option<RawDistInfoMetadata>) -> Self {
        match maybe_raw {
            None => Default::default(),
            Some(raw) => match raw {
                RawDistInfoMetadata::NoHashes(available) => Self {
                    available,
                    hash: None,
                },
                RawDistInfoMetadata::WithHashes(_) => Self {
                    available: true,
                    hash: None,
                },
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum RawDistInfoMetadata {
    NoHashes(bool),
    WithHashes(HashMap<String, String>),
}

#[derive(Debug, Clone, Deserialize)]
enum RawYanked {
    NoReason(bool),
    WithReason(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "RawYanked")]
pub struct Yanked {
    pub yanked: bool,
    pub reason: Option<String>,
}

impl From<RawYanked> for Yanked {
    fn from(raw: RawYanked) -> Self {
        match raw {
            RawYanked::NoReason(yanked) => Self {
                yanked,
                reason: None,
            },
            RawYanked::WithReason(reason) => Self {
                yanked: true,
                reason: Some(reason),
            },
        }
    }
}

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
    pub files: Vec<File>,
    pub meta: Meta,
    pub name: String,
    pub versions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct File {
    pub core_metadata: Metadata,
    pub data_dist_info_metadata: Metadata,
    pub filename: String,
    pub hashes: Hashes,
    pub requires_python: Option<String>,
    pub size: i64,
    pub upload_time: String,
    pub url: String,
    pub yanked: Yanked,
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
    pub sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Meta {
    #[serde(rename = "_last-serial")]
    pub last_serial: i64,
    pub api_version: String,
}

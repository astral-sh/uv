use std::fmt::Debug;

use futures::{AsyncRead, StreamExt, TryStreamExt};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::trace;
use url::Url;

use puffin_package::metadata::Metadata21;
use puffin_package::package_name::PackageName;

use crate::client::PypiClient;
use crate::error::PypiClientError;

impl PypiClient {
    /// Fetch a package from the `PyPI` simple API.
    pub async fn simple(
        &self,
        package_name: impl AsRef<str>,
    ) -> Result<SimpleJson, PypiClientError> {
        // Format the URL for PyPI.
        let mut url = self.registry.join("simple")?;
        url.path_segments_mut()
            .unwrap()
            .push(PackageName::normalize(&package_name).as_ref());
        url.path_segments_mut().unwrap().push("");
        url.set_query(Some("format=application/vnd.pypi.simple.v1+json"));

        trace!(
            "fetching metadata for {} from {}",
            package_name.as_ref(),
            url
        );

        // Fetch from the registry.
        let text = self.simple_impl(&package_name, &url).await?;
        serde_json::from_str(&text)
            .map_err(move |e| PypiClientError::from_json_err(e, String::new()))
    }

    async fn simple_impl(
        &self,
        package_name: impl AsRef<str>,
        url: &Url,
    ) -> Result<String, PypiClientError> {
        Ok(self
            .client
            .get(url.clone())
            .header("Accept-Encoding", "gzip")
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

    /// Fetch the metadata from a wheel file.
    pub async fn file(&self, file: File) -> Result<Metadata21, PypiClientError> {
        // Per PEP 658, if `data-dist-info-metadata` is available, we can request it directly;
        // otherwise, send to our dedicated caching proxy.
        let url = if file.data_dist_info_metadata.is_available() {
            Url::parse(&format!("{}.metadata", file.url))?
        } else {
            self.proxy.join(
                file.url
                    .strip_prefix("https://files.pythonhosted.org/")
                    .unwrap(),
            )?
        };

        trace!("fetching file {} from {}", file.filename, url);

        // Fetch from the registry.
        let text = self.file_impl(&file.filename, &url).await?;
        Metadata21::parse(text.as_bytes()).map_err(std::convert::Into::into)
    }

    async fn file_impl(
        &self,
        filename: impl AsRef<str>,
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
                    PypiClientError::FileNotFound(
                        (*self.registry).clone(),
                        filename.as_ref().to_string(),
                    )
                } else {
                    PypiClientError::RequestError(err)
                }
            })?
            .text()
            .await?)
    }

    /// Stream a file from an external URL.
    pub async fn stream_external(
        &self,
        url: &Url,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send + Sync>, PypiClientError> {
        Ok(Box::new(
            self.uncached_client
                .get(url.to_string())
                .send()
                .await?
                .error_for_status()?
                .bytes_stream()
                .map(|r| match r {
                    Ok(bytes) => Ok(bytes),
                    Err(err) => Err(std::io::Error::new(std::io::ErrorKind::Other, err)),
                })
                .into_async_read(),
        ))
    }
}

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
    pub requires_python: Option<String>,
    pub size: usize,
    pub upload_time: String,
    pub url: String,
    pub yanked: Yanked,
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

//! Types and interfaces for interacting with Warehouse-compatible JSON APIs.

use std::str::FromStr as _;

use crate::types;
use futures::{StreamExt as _, TryStreamExt as _};
use jiff::Timestamp;
use reqwest_middleware::ClientWithMiddleware;
use serde::Deserialize;
use tracing::trace;
use uv_configuration::Concurrency;
use uv_pep440::Version;
use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};

pub const API_BASE: &str = "https://pypi.org/";

/// Errors during PyPI service interactions.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An error during an HTTP request, including middleware errors.
    #[error(transparent)]
    ReqwestMiddleware(#[from] reqwest_middleware::Error),
    /// An error when constructing the URL for an API request.
    #[error("Invalid API URL: {0}")]
    Url(DisplaySafeUrl, #[source] DisplaySafeUrlError),
}

#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    #[serde(default)]
    vulnerabilities: Vec<Vulnerability>,
}

#[derive(Debug, Deserialize)]
struct Vulnerability {
    aliases: Vec<String>,
    details: Option<String>,
    fixed_in: Vec<String>,
    id: String,
    link: Option<DisplaySafeUrl>,
    summary: Option<String>,
    withdrawn: Option<Timestamp>,
}

/// Represents the Warehouse JSON API as a vulnerability service.
pub struct Pypi {
    base_url: DisplaySafeUrl,
    client: ClientWithMiddleware,
    concurrency: Concurrency,
}

impl Pypi {
    /// Create a new PyPI client with the given HTTP client and optional base URL.
    pub fn new(
        client: ClientWithMiddleware,
        base_url: Option<DisplaySafeUrl>,
        concurrency: Concurrency,
    ) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| {
                DisplaySafeUrl::parse(API_BASE).expect("impossible: embedded URL is invalid")
            }),
            client,
            concurrency,
        }
    }

    pub async fn query_batch(
        &self,
        dependencies: &[types::Dependency],
    ) -> Result<Vec<types::Finding>, Error> {
        if dependencies.is_empty() {
            return Ok(vec![]);
        }

        let findings = futures::stream::iter(dependencies.iter().cloned())
            .map(|dependency| async move {
                let vulnerabilities = self.fetch_release(&dependency).await?;
                Ok::<_, Error>((dependency, vulnerabilities))
            })
            .buffer_unordered(self.concurrency.downloads)
            .try_collect::<Vec<_>>()
            .await?
            .into_iter()
            .flat_map(|(dependency, vulnerabilities)| {
                vulnerabilities
                    .into_iter()
                    .filter_map(move |vulnerability| Self::vulnerability_to_finding(&dependency, vulnerability))
            })
            .collect();

        Ok(findings)
    }

    async fn fetch_release(
        &self,
        dependency: &types::Dependency,
    ) -> Result<Vec<Vulnerability>, Error> {
        let url = self
            .base_url
            .join(&format!("pypi/{}/{}/json", dependency.name(), dependency.version()))
            .map_err(|error| Error::Url(self.base_url.clone(), error))?;
        let response: ReleaseResponse = self
            .client
            .get(url.as_ref())
            .send()
            .await?
            .error_for_status()
            .map_err(reqwest_middleware::Error::Reqwest)?
            .json()
            .await
            .map_err(reqwest_middleware::Error::Reqwest)?;
        Ok(response.vulnerabilities)
    }

    fn vulnerability_to_finding(
        dependency: &types::Dependency,
        vulnerability: Vulnerability,
    ) -> Option<types::Finding> {
        if vulnerability.withdrawn.is_some() {
            return None;
        }

        let fix_versions = vulnerability
            .fixed_in
            .into_iter()
            .filter_map(|fixed| {
                if let Ok(version) = Version::from_str(&fixed) {
                    Some(version)
                } else {
                    trace!(
                        "Skipping invalid (non-PEP 440) version in PyPI record {id}: {fixed}",
                        id = vulnerability.id,
                    );
                    None
                }
            })
            .collect();

        Some(types::Finding::Vulnerability(Box::new(types::Vulnerability::new(
            dependency.clone(),
            types::VulnerabilityID::new(vulnerability.id),
            vulnerability.summary,
            vulnerability.details,
            vulnerability.link,
            fix_versions,
            vulnerability
                .aliases
                .into_iter()
                .map(types::VulnerabilityID::new)
                .collect(),
            None,
            None,
        ))))
    }
}
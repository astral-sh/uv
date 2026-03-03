//! Trusted publishing (via OIDC) with GitHub Actions and GitLab CI.

use base64::Engine;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use reqwest::StatusCode;
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use std::env;
use std::fmt::Display;
use thiserror::Error;
use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};
use uv_static::EnvVars;

pub(crate) mod pypi;
pub(crate) mod pyx;

#[derive(Debug, Error)]
pub enum TrustedPublishingError {
    #[error(transparent)]
    Url(#[from] DisplaySafeUrlError),
    #[error("Failed to obtain OIDC token: is the `id-token: write` permission missing?")]
    GitHubPermissions(#[source] ambient_id::Error),
    /// A hard failure during OIDC token discovery.
    #[error("Failed to discover OIDC token")]
    Discovery(#[source] ambient_id::Error),
    /// A soft failure during OIDC token discovery.
    ///
    /// In practice, this usually means the user attempted to force trusted
    /// publishing outside of something like GitHub Actions or GitLab CI.
    #[error("No OIDC token discovered: are you in a supported trusted publishing environment?")]
    NoToken,
    #[error("Failed to fetch: `{0}`")]
    Reqwest(DisplaySafeUrl, #[source] reqwest::Error),
    #[error("Failed to fetch: `{0}`")]
    ReqwestMiddleware(DisplaySafeUrl, #[source] reqwest_middleware::Error),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::error::Error),
    #[error(
        "Server returned error code {0}, is trusted publishing correctly configured?\nResponse: {1}\nToken claims, which must match the publisher configuration: {2:#?}"
    )]
    TokenRejected(StatusCode, String, OidcTokenClaims),
    /// When trusted publishing is misconfigured, the error above should occur, not this one.
    #[error(
        "Server returned error code {0}, and the OIDC has an unexpected format.\nResponse: {1}"
    )]
    InvalidOidcToken(StatusCode, String),
    /// The user gave us a malformed upload URL for trusted publishing with pyx.
    #[error("The upload URL `{0}` does not look like a valid pyx upload URL")]
    InvalidPyxUploadUrl(DisplaySafeUrl),
}

#[derive(Deserialize)]
#[serde(transparent)]
pub struct TrustedPublishingToken(String);

impl Display for TrustedPublishingToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The response from querying `https://pypi.org/_/oidc/audience`.
#[derive(Deserialize)]
struct Audience {
    audience: String,
}

/// The body for querying `$ACTIONS_ID_TOKEN_REQUEST_URL&audience=pypi`.
#[derive(Serialize)]
struct MintTokenRequest {
    token: String,
}

/// The response from querying `$ACTIONS_ID_TOKEN_REQUEST_URL&audience=pypi`.
#[derive(Deserialize)]
struct PublishToken {
    token: TrustedPublishingToken,
}

/// The payload of the OIDC token.
#[derive(Deserialize, Debug)]
#[allow(dead_code)]
#[serde(untagged)]
pub enum OidcTokenClaims {
    GitHub(GitHubTokenClaims),
    GitLab(GitLabTokenClaims),
    Buildkite(BuildkiteTokenClaims),
}

/// The relevant payload of a GitHub OIDC token.
#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub struct GitHubTokenClaims {
    sub: String,
    repository: String,
    repository_owner: String,
    repository_owner_id: String,
    job_workflow_ref: String,
    r#ref: String,
    environment: Option<String>,
}

/// The relevant payload of a GitLab OIDC token.
#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub struct GitLabTokenClaims {
    sub: String,
    project_path: String,
    ci_config_ref_uri: String,
    environment: Option<String>,
}

/// The relevant payload of a Buildkite OIDC token.
#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub struct BuildkiteTokenClaims {
    sub: String,
    pipeline_slug: String,
    organization_slug: String,
}

/// A service (i.e. uploadable index) that supports trusted publishing.
///
/// Interactions should go through the default [`get_token`]; implementors
/// should implement the constituent trait methods.
pub(crate) trait TrustedPublishingService {
    /// Borrow an HTTP client with middleware.
    fn client(&self) -> &ClientWithMiddleware;

    /// Retrieve the service's expected OIDC audience.
    async fn audience(&self) -> Result<String, TrustedPublishingError>;

    /// Exchange an ambient OIDC identity token for a short-lived upload token on the service.
    async fn exchange_token(
        &self,
        oidc_token: ambient_id::IdToken,
    ) -> Result<TrustedPublishingToken, TrustedPublishingError>;

    /// Perform the full trusted publishing token exchange.
    async fn get_token(&self) -> Result<Option<TrustedPublishingToken>, TrustedPublishingError> {
        // Get the OIDC token's audience from the registry.
        let audience = self.audience().await?;

        // Perform ambient OIDC token discovery.
        // Depending on the host (GitHub Actions, GitLab CI, etc.)
        // this may perform additional network requests.
        let oidc_token = get_oidc_token(&audience, self.client()).await?;

        // Exchange the OIDC token for a short-lived upload token,
        // if OIDC token discovery succeeded.
        if let Some(oidc_token) = oidc_token {
            let publish_token = self.exchange_token(oidc_token).await?;

            // If we're on GitHub Actions, mask the exchanged token in logs.
            #[expect(clippy::print_stdout)]
            if env::var(EnvVars::GITHUB_ACTIONS) == Ok("true".to_string()) {
                println!("::add-mask::{publish_token}");
            }

            Ok(Some(publish_token))
        } else {
            // Not in a supported CI environment for trusted publishing.
            Ok(None)
        }
    }
}

/// Perform ambient OIDC token discovery.
async fn get_oidc_token(
    audience: &str,
    client: &ClientWithMiddleware,
) -> Result<Option<ambient_id::IdToken>, TrustedPublishingError> {
    let detector = ambient_id::Detector::new_with_client(client.clone());

    match detector.detect(audience).await {
        Ok(token) => Ok(token),
        // Specialize the error case insufficient permissions error case,
        // since we can offer the user a hint about fixing their permissions.
        Err(
            err @ ambient_id::Error::GitHubActions(
                ambient_id::GitHubError::InsufficientPermissions(_),
            ),
        ) => Err(TrustedPublishingError::GitHubPermissions(err)),
        Err(err) => Err(TrustedPublishingError::Discovery(err)),
    }
}

/// Parse the JSON Web Token that the OIDC token is.
///
/// See: <https://github.com/pypa/gh-action-pypi-publish/blob/db8f07d3871a0a180efa06b95d467625c19d5d5f/oidc-exchange.py#L165-L184>
fn decode_oidc_token(oidc_token: &str) -> Option<OidcTokenClaims> {
    let token_segments = oidc_token.splitn(3, '.').collect::<Vec<&str>>();
    let [_header, payload, _signature] = *token_segments.into_boxed_slice() else {
        return None;
    };
    let decoded = BASE64_URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&decoded).ok()
}

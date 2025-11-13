//! Trusted publishing (via OIDC) with GitHub Actions and GitLab CI.

use base64::Engine;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use reqwest::StatusCode;
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use std::env;
use std::fmt::Display;
use thiserror::Error;
use tracing::{debug, trace};
use url::Url;
use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};
use uv_static::EnvVars;

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
        "PyPI returned error code {0}, is trusted publishing correctly configured?\nResponse: {1}\nToken claims, which must match the PyPI configuration: {2:#?}"
    )]
    Pypi(StatusCode, String, OidcTokenClaims),
    /// When trusted publishing is misconfigured, the error above should occur, not this one.
    #[error("PyPI returned error code {0}, and the OIDC has an unexpected format.\nResponse: {1}")]
    InvalidOidcToken(StatusCode, String),
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
pub struct OidcTokenClaims {
    sub: String,
    repository: String,
    repository_owner: String,
    repository_owner_id: String,
    job_workflow_ref: String,
    r#ref: String,
}

/// Returns the short-lived token to use for uploading.
///
/// Return states:
/// - `Ok(Some(token))`: Successfully obtained a trusted publishing token.
/// - `Ok(None)`: Not in a supported CI environment for trusted publishing.
/// - `Err(...)`: An error occurred while trying to obtain the token.
pub(crate) async fn get_token(
    registry: &DisplaySafeUrl,
    client: &ClientWithMiddleware,
) -> Result<Option<TrustedPublishingToken>, TrustedPublishingError> {
    // Get the OIDC token's audience from the registry.
    let audience = get_audience(registry, client).await?;

    // Perform ambient OIDC token discovery.
    // Depending on the host (GitHub Actions, GitLab CI, etc.)
    // this may perform additional network requests.
    let oidc_token = get_oidc_token(&audience, client).await?;

    // Exchange the OIDC token for a short-lived upload token,
    // if OIDC token discovery succeeded.
    if let Some(oidc_token) = oidc_token {
        let publish_token = get_publish_token(registry, oidc_token, client).await?;

        // If we're on GitHub Actions, mask the exchanged token in logs.
        #[allow(clippy::print_stdout)]
        if env::var(EnvVars::GITHUB_ACTIONS) == Ok("true".to_string()) {
            println!("::add-mask::{publish_token}");
        }

        Ok(Some(publish_token))
    } else {
        // Not in a supported CI environment for trusted publishing.
        Ok(None)
    }
}

async fn get_audience(
    registry: &DisplaySafeUrl,
    client: &ClientWithMiddleware,
) -> Result<String, TrustedPublishingError> {
    // `pypa/gh-action-pypi-publish` uses `netloc` (RFC 1808), which is deprecated for authority
    // (RFC 3986).
    // Prefer HTTPS for OIDC discovery; allow HTTP only in test builds
    let scheme: &str = if cfg!(feature = "test") {
        registry.scheme()
    } else {
        "https"
    };
    let audience_url = DisplaySafeUrl::parse(&format!(
        "{}://{}/_/oidc/audience",
        scheme,
        registry.authority()
    ))?;
    debug!("Querying the trusted publishing audience from {audience_url}");
    let response = client
        .get(Url::from(audience_url.clone()))
        .send()
        .await
        .map_err(|err| TrustedPublishingError::ReqwestMiddleware(audience_url.clone(), err))?;
    let audience = response
        .error_for_status()
        .map_err(|err| TrustedPublishingError::Reqwest(audience_url.clone(), err))?
        .json::<Audience>()
        .await
        .map_err(|err| TrustedPublishingError::Reqwest(audience_url.clone(), err))?;
    trace!("The audience is `{}`", &audience.audience);
    Ok(audience.audience)
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

async fn get_publish_token(
    registry: &DisplaySafeUrl,
    oidc_token: ambient_id::IdToken,
    client: &ClientWithMiddleware,
) -> Result<TrustedPublishingToken, TrustedPublishingError> {
    // Prefer HTTPS for OIDC minting; allow HTTP only in test builds
    let scheme: &str = if cfg!(feature = "test") {
        registry.scheme()
    } else {
        "https"
    };
    let mint_token_url = DisplaySafeUrl::parse(&format!(
        "{}://{}/_/oidc/mint-token",
        scheme,
        registry.authority()
    ))?;
    debug!("Querying the trusted publishing upload token from {mint_token_url}");
    let mint_token_payload = MintTokenRequest {
        token: oidc_token.reveal().to_string(),
    };
    let response = client
        .post(Url::from(mint_token_url.clone()))
        .body(serde_json::to_vec(&mint_token_payload)?)
        .send()
        .await
        .map_err(|err| TrustedPublishingError::ReqwestMiddleware(mint_token_url.clone(), err))?;

    // reqwest's implementation of `.json()` also goes through `.bytes()`
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|err| TrustedPublishingError::Reqwest(mint_token_url.clone(), err))?;

    if status.is_success() {
        let publish_token: PublishToken = serde_json::from_slice(&body)?;
        Ok(publish_token.token)
    } else {
        match decode_oidc_token(oidc_token.reveal()) {
            Some(claims) => {
                // An error here means that something is misconfigured, e.g. a typo in the PyPI
                // configuration, so we're showing the body and the JWT claims for more context, see
                // https://docs.pypi.org/trusted-publishers/troubleshooting/#token-minting
                // for what the body can mean.
                Err(TrustedPublishingError::Pypi(
                    status,
                    String::from_utf8_lossy(&body).to_string(),
                    claims,
                ))
            }
            None => {
                // This is not a user configuration error, the OIDC token should always have a valid
                // format.
                Err(TrustedPublishingError::InvalidOidcToken(
                    status,
                    String::from_utf8_lossy(&body).to_string(),
                ))
            }
        }
    }
}

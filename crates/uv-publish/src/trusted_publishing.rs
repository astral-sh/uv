//! Trusted publishing (via OIDC) with GitHub actions.

use reqwest::{header, StatusCode};
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use std::env;
use std::env::VarError;
use std::fmt::Display;
use thiserror::Error;
use tracing::{debug, trace};
use url::Url;
use uv_static::EnvVars;

#[derive(Debug, Error)]
pub enum TrustedPublishingError {
    #[error(transparent)]
    Var(#[from] VarError),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error("Failed to fetch {0}")]
    Reqwest(Url, #[source] reqwest::Error),
    #[error("Failed to fetch {0}")]
    ReqwestMiddleware(Url, #[source] reqwest_middleware::Error),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::error::Error),
    #[error(
        "PyPI returned error code {0}, is trusted publishing correctly configured?\nResponse: {1}"
    )]
    Pypi(StatusCode, String),
}

#[derive(Deserialize)]
#[serde(transparent)]
pub struct TrustedPublishingToken(String);

impl From<TrustedPublishingToken> for String {
    fn from(token: TrustedPublishingToken) -> Self {
        token.0
    }
}

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

/// The response from querying `$ACTIONS_ID_TOKEN_REQUEST_URL&audience=pypi`.
#[derive(Deserialize)]
struct OidcToken {
    value: String,
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

/// Returns the short-lived token to use for uploading.
pub(crate) async fn get_token(
    registry: &Url,
    client: &ClientWithMiddleware,
) -> Result<TrustedPublishingToken, TrustedPublishingError> {
    // If this fails, we can skip the audience request.
    let oidc_token_request_token = env::var(EnvVars::ACTIONS_ID_TOKEN_REQUEST_TOKEN)?;

    // Request 1: Get the audience
    let audience = get_audience(registry, client).await?;

    // Request 2: Get the OIDC token from GitHub.
    let oidc_token = get_oidc_token(&audience, &oidc_token_request_token, client).await?;

    // Request 3: Get the publishing token from PyPI.
    let publish_token = get_publish_token(registry, &oidc_token, client).await?;

    debug!("Received token, using trusted publishing");

    // Tell GitHub Actions to mask the token in any console logs.
    #[allow(clippy::print_stdout)]
    if env::var(EnvVars::GITHUB_ACTIONS) == Ok("true".to_string()) {
        println!("::add-mask::{}", &publish_token);
    }

    Ok(publish_token)
}

async fn get_audience(
    registry: &Url,
    client: &ClientWithMiddleware,
) -> Result<String, TrustedPublishingError> {
    // `pypa/gh-action-pypi-publish` uses `netloc` (RFC 1808), which is deprecated for authority
    // (RFC 3986).
    let audience_url = Url::parse(&format!("https://{}/_/oidc/audience", registry.authority()))?;
    debug!("Querying the trusted publishing audience from {audience_url}");
    let response = client
        .get(audience_url.clone())
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

async fn get_oidc_token(
    audience: &str,
    oidc_token_request_token: &str,
    client: &ClientWithMiddleware,
) -> Result<String, TrustedPublishingError> {
    let mut oidc_token_url = Url::parse(&env::var(EnvVars::ACTIONS_ID_TOKEN_REQUEST_URL)?)?;
    oidc_token_url
        .query_pairs_mut()
        .append_pair("audience", audience);
    debug!("Querying the trusted publishing OIDC token from {oidc_token_url}");
    let authorization = format!("bearer {oidc_token_request_token}");
    let response = client
        .get(oidc_token_url.clone())
        .header(header::AUTHORIZATION, authorization)
        .send()
        .await
        .map_err(|err| TrustedPublishingError::ReqwestMiddleware(oidc_token_url.clone(), err))?;
    let oidc_token: OidcToken = response
        .error_for_status()
        .map_err(|err| TrustedPublishingError::Reqwest(oidc_token_url.clone(), err))?
        .json()
        .await
        .map_err(|err| TrustedPublishingError::Reqwest(oidc_token_url.clone(), err))?;
    Ok(oidc_token.value)
}

async fn get_publish_token(
    registry: &Url,
    oidc_token: &str,
    client: &ClientWithMiddleware,
) -> Result<TrustedPublishingToken, TrustedPublishingError> {
    let mint_token_url = Url::parse(&format!(
        "https://{}/_/oidc/mint-token",
        registry.authority()
    ))?;
    debug!("Querying the trusted publishing upload token from {mint_token_url}");
    let mint_token_payload = MintTokenRequest {
        token: oidc_token.to_string(),
    };
    let response = client
        .post(mint_token_url.clone())
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
        // An error here means that something is misconfigured, e.g. a typo in the PyPI
        // configuration, so we're showing the body for more context, see
        // https://docs.pypi.org/trusted-publishers/troubleshooting/#token-minting
        // for what the body can mean.
        Err(TrustedPublishingError::Pypi(
            status,
            String::from_utf8_lossy(&body).to_string(),
        ))
    }
}

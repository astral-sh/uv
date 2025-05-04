//! Trusted publishing (via OIDC) with GitHub actions.

use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use base64::Engine;
use reqwest::{header, StatusCode};
use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use std::env;
use std::env::VarError;
use std::ffi::OsString;
use std::fmt::Display;
use thiserror::Error;
use tracing::{debug, trace};
use url::Url;
use uv_static::EnvVars;

#[derive(Debug, Error)]
pub enum TrustedPublishingError {
    #[error("Environment variable {0} not set, is the `id-token: write` permission missing?")]
    MissingEnvVar(&'static str),
    #[error("Environment variable {0} is not valid UTF-8: `{1:?}`")]
    InvalidEnvVar(&'static str, OsString),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error("Failed to fetch: `{0}`")]
    Reqwest(Url, #[source] reqwest::Error),
    #[error("Failed to fetch: `{0}`")]
    ReqwestMiddleware(Url, #[source] reqwest_middleware::Error),
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

impl TrustedPublishingError {
    fn from_var_err(env_var: &'static str, err: VarError) -> Self {
        match err {
            VarError::NotPresent => Self::MissingEnvVar(env_var),
            VarError::NotUnicode(os_string) => Self::InvalidEnvVar(env_var, os_string),
        }
    }
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
pub(crate) async fn get_token(
    registry: &Url,
    client: &ClientWithMiddleware,
) -> Result<TrustedPublishingToken, TrustedPublishingError> {
    // If this fails, we can skip the audience request.
    let oidc_token_request_token =
        env::var(EnvVars::ACTIONS_ID_TOKEN_REQUEST_TOKEN).map_err(|err| {
            TrustedPublishingError::from_var_err(EnvVars::ACTIONS_ID_TOKEN_REQUEST_TOKEN, err)
        })?;

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
    let oidc_token_url = env::var(EnvVars::ACTIONS_ID_TOKEN_REQUEST_URL).map_err(|err| {
        TrustedPublishingError::from_var_err(EnvVars::ACTIONS_ID_TOKEN_REQUEST_URL, err)
    })?;
    let mut oidc_token_url = Url::parse(&oidc_token_url)?;
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
        match decode_oidc_token(oidc_token) {
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

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use reqwest_middleware::ClientWithMiddleware;
use tracing::debug;
use url::Url;

use uv_cache_key::CanonicalUrl;
use uv_redacted::DisplaySafeUrl;
use uv_small_str::SmallString;
use uv_state::{StateBucket, StateStore};
use uv_static::EnvVars;

use crate::{Credentials, Realm};

/// Retrieve the pyx API key from the environment variable, or return `None`.
fn read_pyx_api_key() -> Option<String> {
    std::env::var(EnvVars::PYX_API_KEY).ok()
}

/// Retrieve the pyx authentication token (JWT) from the environment variable, or return `None`.
fn read_pyx_auth_token() -> Option<AccessToken> {
    std::env::var(EnvVars::PYX_AUTH_TOKEN).ok().map(AccessToken)
}

/// An encoded JWT access token.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct AccessToken(String);

impl AccessToken {
    /// Return the [`AccessToken`] as a vector of bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0.into_bytes()
    }

    /// Return the [`AccessToken`] as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<[u8]> for AccessToken {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl std::fmt::Display for AccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An access token with an accompanying refresh token.
///
/// Refresh tokens are single-use tokens that can be exchanged for a renewed access token
/// and a new refresh token.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct OAuthTokens {
    pub access_token: AccessToken,
    pub refresh_token: String,
}

/// An access token with an accompanying API key.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ApiKeyTokens {
    pub access_token: AccessToken,
    pub api_key: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum Tokens {
    /// An access token with an accompanying refresh token.
    ///
    /// Refresh tokens are single-use tokens that can be exchanged for a renewed access token
    /// and a new refresh token.
    OAuth(OAuthTokens),
    /// An access token with an accompanying API key.
    ///
    /// API keys are long-lived tokens that can be exchanged for an access token.
    ApiKey(ApiKeyTokens),
}

impl From<Tokens> for AccessToken {
    fn from(tokens: Tokens) -> Self {
        match tokens {
            Tokens::OAuth(OAuthTokens { access_token, .. }) => access_token,
            Tokens::ApiKey(ApiKeyTokens { access_token, .. }) => access_token,
        }
    }
}

impl From<Tokens> for Credentials {
    fn from(tokens: Tokens) -> Self {
        let access_token = match tokens {
            Tokens::OAuth(OAuthTokens { access_token, .. }) => access_token,
            Tokens::ApiKey(ApiKeyTokens { access_token, .. }) => access_token,
        };
        Self::from(access_token)
    }
}

impl From<AccessToken> for Credentials {
    fn from(access_token: AccessToken) -> Self {
        Self::Bearer {
            token: access_token.into_bytes(),
        }
    }
}

/// The default tolerance for the access token expiration.
pub const DEFAULT_TOLERANCE_SECS: u64 = 60 * 5;

#[derive(Debug, Clone)]
pub struct PyxTokenStore {
    /// The root directory for the token store (e.g., `/Users/ferris/.local/share/uv/credentials`).
    root: PathBuf,
    /// The subdirectory for the token store (e.g., `/Users/ferris/.local/share/uv/credentials/3859a629b26fda96`).
    path: PathBuf,
    /// The API URL for the token store (e.g., `https://api.pyx.dev`).
    api: DisplaySafeUrl,
    /// The CDN domain for the token store (e.g., `astralhosted.com`).
    cdn: SmallString,
}

impl PyxTokenStore {
    /// Create a new [`PyxTokenStore`] from settings.
    pub fn from_settings() -> Result<Self, TokenStoreError> {
        // Read the API URL and CDN domain from the environment variables, or fallback to the
        // defaults.
        let api = if let Ok(api_url) = std::env::var(EnvVars::PYX_API_URL) {
            DisplaySafeUrl::parse(&api_url)
        } else {
            DisplaySafeUrl::parse("https://api.pyx.dev")
        }?;
        let cdn = std::env::var(EnvVars::PYX_CDN_DOMAIN)
            .ok()
            .map(SmallString::from)
            .unwrap_or_else(|| SmallString::from(arcstr::literal!("astralhosted.com")));

        // Read the credentials directory from the environment variable, or fallback to the default
        // credentials directory.
        let root = if let Some(tool_dir) = std::env::var_os(EnvVars::UV_CREDENTIALS_DIR) {
            std::path::absolute(tool_dir)?
        } else {
            StateStore::from_settings(None)?.bucket(StateBucket::Credentials)
        };

        // Use a separate subdirectory for each API URL.
        let digest = uv_cache_key::cache_digest(&CanonicalUrl::new(&api));
        let path = root.join(digest);

        Ok(Self {
            root,
            path,
            api,
            cdn,
        })
    }

    /// Return the root directory for the token store.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Return the API URL for the token store.
    pub fn api(&self) -> &DisplaySafeUrl {
        &self.api
    }

    /// Get or initialize an [`AccessToken`] from the store.
    ///
    /// If an access token is set in the environment, it will be returned as-is.
    ///
    /// If an access token is present on-disk, it will be returned (and refreshed, if necessary).
    ///
    /// If no access token is found, but an API key is present, the API key will be used to
    /// bootstrap an access token.
    pub async fn access_token(
        &self,
        client: &ClientWithMiddleware,
        tolerance_secs: u64,
    ) -> Result<Option<AccessToken>, TokenStoreError> {
        // If the access token is already set in the environment, return it.
        if let Some(access_token) = read_pyx_auth_token() {
            return Ok(Some(access_token));
        }

        // Initialize the tokens from the store.
        let tokens = self.init(client, tolerance_secs).await?;

        // Extract the access token from the OAuth tokens or API key.
        Ok(tokens.map(AccessToken::from))
    }

    /// Initialize the [`Tokens`] from the store.
    ///
    /// If an access token is already present, it will be returned (and refreshed, if necessary).
    ///
    /// If no access token is found, but an API key is present, the API key will be used to
    /// bootstrap an access token.
    pub async fn init(
        &self,
        client: &ClientWithMiddleware,
        tolerance_secs: u64,
    ) -> Result<Option<Tokens>, TokenStoreError> {
        match self.read().await? {
            Some(tokens) => {
                // Refresh the tokens if they are expired.
                let tokens = self.refresh(tokens, client, tolerance_secs).await?;
                Ok(Some(tokens))
            }
            None => {
                // If no tokens are present, bootstrap them from an API key.
                self.bootstrap(client).await
            }
        }
    }

    /// Write the tokens to the store.
    pub async fn write(&self, tokens: &Tokens) -> Result<(), TokenStoreError> {
        fs_err::tokio::create_dir_all(&self.path).await?;
        match tokens {
            Tokens::OAuth(tokens) => {
                // Write OAuth tokens to a generic `tokens.json` file.
                fs_err::tokio::write(self.path.join("tokens.json"), serde_json::to_vec(tokens)?)
                    .await?;
            }
            Tokens::ApiKey(tokens) => {
                // Write API key tokens to a file based on the API key.
                let digest = uv_cache_key::cache_digest(&tokens.api_key);
                fs_err::tokio::write(
                    self.path.join(format!("{digest}.json")),
                    &tokens.access_token,
                )
                .await?;
            }
        }
        Ok(())
    }

    /// Returns `true` if the user appears to have credentials (which may be invalid).
    pub fn has_credentials(&self) -> bool {
        read_pyx_auth_token().is_some()
            || read_pyx_api_key().is_some()
            || self.path.join("tokens.json").is_file()
    }

    /// Read the tokens from the store.
    pub async fn read(&self) -> Result<Option<Tokens>, TokenStoreError> {
        // Retrieve the API URL from the environment variable, or error if unset.
        if let Some(api_key) = read_pyx_api_key() {
            // Read the API key tokens from a file based on the API key.
            let digest = uv_cache_key::cache_digest(&api_key);
            match fs_err::tokio::read(self.path.join(format!("{digest}.json"))).await {
                Ok(data) => {
                    let access_token = AccessToken(String::from_utf8(data).expect("Invalid UTF-8"));
                    Ok(Some(Tokens::ApiKey(ApiKeyTokens {
                        access_token,
                        api_key,
                    })))
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(err) => Err(err.into()),
            }
        } else {
            match fs_err::tokio::read(self.path.join("tokens.json")).await {
                Ok(data) => {
                    let tokens: OAuthTokens = serde_json::from_slice(&data)?;
                    Ok(Some(Tokens::OAuth(tokens)))
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(err) => Err(err.into()),
            }
        }
    }

    /// Remove the tokens from the store.
    pub async fn delete(&self) -> Result<(), io::Error> {
        fs_err::tokio::remove_dir_all(&self.path).await?;
        Ok(())
    }

    /// Bootstrap the tokens from the store.
    async fn bootstrap(
        &self,
        client: &ClientWithMiddleware,
    ) -> Result<Option<Tokens>, TokenStoreError> {
        #[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
        struct Payload {
            access_token: AccessToken,
        }

        // Retrieve the API key from the environment variable, if set.
        let Some(api_key) = read_pyx_api_key() else {
            return Ok(None);
        };

        debug!("Bootstrapping access token from an API key");

        // Parse the API URL.
        let mut url = self.api.clone();
        url.set_path("auth/cli/access-token");

        let mut request = reqwest::Request::new(reqwest::Method::POST, Url::from(url));
        request.headers_mut().insert(
            "Authorization",
            reqwest::header::HeaderValue::from_str(&format!("Bearer {api_key}"))?,
        );

        let response = client.execute(request).await?;
        let Payload { access_token } = response.error_for_status()?.json::<Payload>().await?;
        let tokens = Tokens::ApiKey(ApiKeyTokens {
            access_token,
            api_key,
        });

        // Write the tokens to disk.
        self.write(&tokens).await?;

        Ok(Some(tokens))
    }

    /// Refresh the tokens in the store, if they are expired.
    ///
    /// In theory, we should _also_ refresh if we hit a 401; but for now, we only refresh ahead of
    /// time.
    async fn refresh(
        &self,
        tokens: Tokens,
        client: &ClientWithMiddleware,
        tolerance_secs: u64,
    ) -> Result<Tokens, TokenStoreError> {
        // Decode the access token.
        let jwt = Jwt::decode(match &tokens {
            Tokens::OAuth(OAuthTokens { access_token, .. }) => access_token.as_str(),
            Tokens::ApiKey(ApiKeyTokens { access_token, .. }) => access_token.as_str(),
        })?;

        // If the access token is expired, refresh it.
        let is_up_to_date = match jwt.exp {
            None => {
                debug!("Access token has no expiration; refreshing...");
                false
            }
            Some(..) if tolerance_secs == 0 => {
                debug!("Refreshing access token due to zero tolerance...");
                false
            }
            Some(jwt) => {
                let exp = jiff::Timestamp::from_second(jwt)?;
                let now = jiff::Timestamp::now();
                if exp < now {
                    debug!("Access token is expired (`{exp}`); refreshing...");
                    false
                } else if exp < now + Duration::from_secs(tolerance_secs) {
                    debug!(
                        "Access token will expire within the tolerance (`{exp}`); refreshing..."
                    );
                    false
                } else {
                    debug!("Access token is up-to-date (`{exp}`)");
                    true
                }
            }
        };

        if is_up_to_date {
            return Ok(tokens);
        }

        let tokens = match tokens {
            Tokens::OAuth(OAuthTokens { refresh_token, .. }) => {
                // Parse the API URL.
                let mut url = self.api.clone();
                url.set_path("auth/cli/refresh");

                let mut request = reqwest::Request::new(reqwest::Method::POST, Url::from(url));
                let body = serde_json::json!({
                    "refresh_token": refresh_token
                });
                *request.body_mut() = Some(body.to_string().into());

                let response = client.execute(request).await?;
                let tokens = response.error_for_status()?.json::<OAuthTokens>().await?;
                Tokens::OAuth(tokens)
            }
            Tokens::ApiKey(ApiKeyTokens { api_key, .. }) => {
                #[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
                struct Payload {
                    access_token: AccessToken,
                }

                // Parse the API URL.
                let mut url = self.api.clone();
                url.set_path("auth/cli/access-token");

                let mut request = reqwest::Request::new(reqwest::Method::POST, Url::from(url));
                request.headers_mut().insert(
                    "Authorization",
                    reqwest::header::HeaderValue::from_str(&format!("Bearer {api_key}"))?,
                );

                let response = client.execute(request).await?;
                let Payload { access_token } =
                    response.error_for_status()?.json::<Payload>().await?;
                Tokens::ApiKey(ApiKeyTokens {
                    access_token,
                    api_key,
                })
            }
        };

        // Write the new tokens to disk.
        self.write(&tokens).await?;
        Ok(tokens)
    }

    /// Returns `true` if the given URL is known to this token store.
    pub fn is_known_url(&self, url: &Url) -> bool {
        // Determine whether the URL matches the API realm.
        if Realm::from(url) == Realm::from(&*self.api) {
            return true;
        }

        // Determine whether the URL matches the CDN domain (or a subdomain of it).
        if matches_domain(url, &self.cdn) {
            return true;
        }

        false
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TokenStoreError {
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    ReqwestMiddleware(#[from] reqwest_middleware::Error),
    #[error(transparent)]
    InvalidHeaderValue(#[from] reqwest::header::InvalidHeaderValue),
    #[error(transparent)]
    Jiff(#[from] jiff::Error),
    #[error(transparent)]
    Jwt(#[from] JwtError),
}

impl TokenStoreError {
    /// Returns `true` if the error is a 401 (Unauthorized) error.
    pub fn is_unauthorized(&self) -> bool {
        match self {
            Self::Reqwest(err) => err.status() == Some(reqwest::StatusCode::UNAUTHORIZED),
            Self::ReqwestMiddleware(err) => err.status() == Some(reqwest::StatusCode::UNAUTHORIZED),
            _ => false,
        }
    }
}

/// The payload of the JWT.
#[derive(Debug, serde::Deserialize)]
struct Jwt {
    exp: Option<i64>,
}

impl Jwt {
    /// Decode the JWT from the access token.
    fn decode(access_token: &str) -> Result<Self, JwtError> {
        let mut token_segments = access_token.splitn(3, '.');

        let _header = token_segments.next().ok_or(JwtError::MissingHeader)?;
        let payload = token_segments.next().ok_or(JwtError::MissingPayload)?;
        let _signature = token_segments.next().ok_or(JwtError::MissingSignature)?;
        if token_segments.next().is_some() {
            return Err(JwtError::TooManySegments);
        }

        let decoded = BASE64_URL_SAFE_NO_PAD.decode(payload)?;

        let jwt = serde_json::from_slice::<Self>(&decoded)?;
        Ok(jwt)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum JwtError {
    #[error("JWT is missing a header")]
    MissingHeader,
    #[error("JWT is missing a payload")]
    MissingPayload,
    #[error("JWT is missing a signature")]
    MissingSignature,
    #[error("JWT has too many segments")]
    TooManySegments,
    #[error(transparent)]
    Base64(#[from] base64::DecodeError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

/// Returns `true` if the target URL is on the given domain.
fn matches_domain(url: &Url, domain: &str) -> bool {
    url.domain().is_some_and(|subdomain| {
        subdomain == domain
            || subdomain
                .strip_suffix(domain)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_domain() {
        assert!(matches_domain(
            &Url::parse("https://example.com").unwrap(),
            "example.com"
        ));
        assert!(matches_domain(
            &Url::parse("https://foo.example.com").unwrap(),
            "example.com"
        ));
        assert!(matches_domain(
            &Url::parse("https://bar.foo.example.com").unwrap(),
            "example.com"
        ));

        assert!(!matches_domain(
            &Url::parse("https://example.com").unwrap(),
            "other.com"
        ));
        assert!(!matches_domain(
            &Url::parse("https://example.org").unwrap(),
            "example.com"
        ));
        assert!(!matches_domain(
            &Url::parse("https://badexample.com").unwrap(),
            "example.com"
        ));
    }
}

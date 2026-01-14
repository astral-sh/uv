use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use etcetera::BaseStrategy;
use reqwest_middleware::ClientWithMiddleware;
use tracing::debug;
use url::Url;

use uv_cache_key::CanonicalUrl;
use uv_redacted::{DisplaySafeUrl, DisplaySafeUrlError};
use uv_small_str::SmallString;
use uv_state::{StateBucket, StateStore};
use uv_static::EnvVars;

use crate::credentials::Token;
use crate::{AccessToken, Credentials, Realm};

/// Retrieve the pyx API key from the environment variable, or return `None`.
fn read_pyx_api_key() -> Option<String> {
    std::env::var(EnvVars::PYX_API_KEY)
        .ok()
        .or_else(|| std::env::var(EnvVars::UV_API_KEY).ok())
}

/// Retrieve the pyx authentication token (JWT) from the environment variable, or return `None`.
fn read_pyx_auth_token() -> Option<AccessToken> {
    std::env::var(EnvVars::PYX_AUTH_TOKEN)
        .ok()
        .or_else(|| std::env::var(EnvVars::UV_AUTH_TOKEN).ok())
        .map(AccessToken::from)
}

/// An access token with an accompanying refresh token.
///
/// Refresh tokens are single-use tokens that can be exchanged for a renewed access token
/// and a new refresh token.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PyxOAuthTokens {
    pub access_token: AccessToken,
    pub refresh_token: String,
}

/// An access token with an accompanying API key.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PyxApiKeyTokens {
    pub access_token: AccessToken,
    pub api_key: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum PyxTokens {
    /// An access token with an accompanying refresh token.
    ///
    /// Refresh tokens are single-use tokens that can be exchanged for a renewed access token
    /// and a new refresh token.
    OAuth(PyxOAuthTokens),
    /// An access token with an accompanying API key.
    ///
    /// API keys are long-lived tokens that can be exchanged for an access token.
    ApiKey(PyxApiKeyTokens),
}

impl From<PyxTokens> for AccessToken {
    fn from(tokens: PyxTokens) -> Self {
        match tokens {
            PyxTokens::OAuth(PyxOAuthTokens { access_token, .. }) => access_token,
            PyxTokens::ApiKey(PyxApiKeyTokens { access_token, .. }) => access_token,
        }
    }
}

impl From<PyxTokens> for Credentials {
    fn from(tokens: PyxTokens) -> Self {
        let access_token = match tokens {
            PyxTokens::OAuth(PyxOAuthTokens { access_token, .. }) => access_token,
            PyxTokens::ApiKey(PyxApiKeyTokens { access_token, .. }) => access_token,
        };
        Self::from(access_token)
    }
}

impl From<AccessToken> for Credentials {
    fn from(access_token: AccessToken) -> Self {
        Self::Bearer {
            token: Token::new(access_token.into_bytes()),
        }
    }
}

/// The default tolerance for the access token expiration.
pub const DEFAULT_TOLERANCE_SECS: u64 = 60 * 5;

#[derive(Debug, Clone)]
struct PyxDirectories {
    /// The root directory for the token store (e.g., `/Users/ferris/.local/share/pyx/credentials`).
    root: PathBuf,
    /// The subdirectory for the token store (e.g., `/Users/ferris/.local/share/uv/credentials/3859a629b26fda96`).
    subdirectory: PathBuf,
}

impl PyxDirectories {
    /// Detect the [`PyxDirectories`] for a given API URL.
    fn from_api(api: &DisplaySafeUrl) -> Result<Self, io::Error> {
        // Store credentials in a subdirectory based on the API URL.
        let digest = uv_cache_key::cache_digest(&CanonicalUrl::new(api));

        // If the user explicitly set `PYX_CREDENTIALS_DIR`, use that.
        if let Some(root) = std::env::var_os(EnvVars::PYX_CREDENTIALS_DIR) {
            let root = std::path::absolute(root)?;
            let subdirectory = root.join(&digest);
            return Ok(Self { root, subdirectory });
        }

        // If the user has pyx credentials in their uv credentials directory, read them for
        // backwards compatibility.
        let root = if let Some(tool_dir) = std::env::var_os(EnvVars::UV_CREDENTIALS_DIR) {
            std::path::absolute(tool_dir)?
        } else {
            StateStore::from_settings(None)?.bucket(StateBucket::Credentials)
        };
        let subdirectory = root.join(&digest);
        if subdirectory.exists() {
            return Ok(Self { root, subdirectory });
        }

        // Otherwise, use (e.g.) `~/.local/share/pyx`.
        let Ok(xdg) = etcetera::base_strategy::choose_base_strategy() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Could not determine user data directory",
            ));
        };

        let root = xdg.data_dir().join("pyx").join("credentials");
        let subdirectory = root.join(&digest);
        Ok(Self { root, subdirectory })
    }
}

#[derive(Debug, Clone)]
pub struct PyxTokenStore {
    /// The root directory for the token store (e.g., `/Users/ferris/.local/share/pyx/credentials`).
    root: PathBuf,
    /// The subdirectory for the token store (e.g., `/Users/ferris/.local/share/uv/credentials/3859a629b26fda96`).
    subdirectory: PathBuf,
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

        // Determine the root directory for the token store.
        let PyxDirectories { root, subdirectory } = PyxDirectories::from_api(&api)?;

        Ok(Self {
            root,
            subdirectory,
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

    /// Initialize the [`PyxTokens`] from the store.
    ///
    /// If an access token is already present, it will be returned (and refreshed, if necessary).
    ///
    /// If no access token is found, but an API key is present, the API key will be used to
    /// bootstrap an access token.
    pub async fn init(
        &self,
        client: &ClientWithMiddleware,
        tolerance_secs: u64,
    ) -> Result<Option<PyxTokens>, TokenStoreError> {
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
    pub async fn write(&self, tokens: &PyxTokens) -> Result<(), TokenStoreError> {
        fs_err::tokio::create_dir_all(&self.subdirectory).await?;
        match tokens {
            PyxTokens::OAuth(tokens) => {
                // Write OAuth tokens to a generic `tokens.json` file.
                fs_err::tokio::write(
                    self.subdirectory.join("tokens.json"),
                    serde_json::to_vec(tokens)?,
                )
                .await?;
            }
            PyxTokens::ApiKey(tokens) => {
                // Write API key tokens to a file based on the API key.
                let digest = uv_cache_key::cache_digest(&tokens.api_key);
                fs_err::tokio::write(
                    self.subdirectory.join(format!("{digest}.json")),
                    &tokens.access_token,
                )
                .await?;
            }
        }
        Ok(())
    }

    /// Returns `true` if the user appears to have an authentication token set.
    pub fn has_auth_token(&self) -> bool {
        read_pyx_auth_token().is_some()
    }

    /// Returns `true` if the user appears to have an API key set.
    pub fn has_api_key(&self) -> bool {
        read_pyx_api_key().is_some()
    }

    /// Returns `true` if the user appears to have OAuth tokens stored on disk.
    pub fn has_oauth_tokens(&self) -> bool {
        self.subdirectory.join("tokens.json").is_file()
    }

    /// Returns `true` if the user appears to have credentials (which may be invalid).
    pub fn has_credentials(&self) -> bool {
        self.has_auth_token() || self.has_api_key() || self.has_oauth_tokens()
    }

    /// Read the tokens from the store.
    pub async fn read(&self) -> Result<Option<PyxTokens>, TokenStoreError> {
        if let Some(api_key) = read_pyx_api_key() {
            // Read the API key tokens from a file based on the API key.
            let digest = uv_cache_key::cache_digest(&api_key);
            match fs_err::tokio::read(self.subdirectory.join(format!("{digest}.json"))).await {
                Ok(data) => {
                    let access_token =
                        AccessToken::from(String::from_utf8(data).expect("Invalid UTF-8"));
                    Ok(Some(PyxTokens::ApiKey(PyxApiKeyTokens {
                        access_token,
                        api_key,
                    })))
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(err) => Err(err.into()),
            }
        } else {
            match fs_err::tokio::read(self.subdirectory.join("tokens.json")).await {
                Ok(data) => {
                    let tokens: PyxOAuthTokens = serde_json::from_slice(&data)?;
                    Ok(Some(PyxTokens::OAuth(tokens)))
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(err) => Err(err.into()),
            }
        }
    }

    /// Remove the tokens from the store.
    pub async fn delete(&self) -> Result<(), io::Error> {
        fs_err::tokio::remove_dir_all(&self.subdirectory).await?;
        Ok(())
    }

    /// Bootstrap the tokens from the store.
    async fn bootstrap(
        &self,
        client: &ClientWithMiddleware,
    ) -> Result<Option<PyxTokens>, TokenStoreError> {
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
        let tokens = PyxTokens::ApiKey(PyxApiKeyTokens {
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
        tokens: PyxTokens,
        client: &ClientWithMiddleware,
        tolerance_secs: u64,
    ) -> Result<PyxTokens, TokenStoreError> {
        // Decode the access token.
        let jwt = PyxJwt::decode(match &tokens {
            PyxTokens::OAuth(PyxOAuthTokens { access_token, .. }) => access_token,
            PyxTokens::ApiKey(PyxApiKeyTokens { access_token, .. }) => access_token,
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
            PyxTokens::OAuth(PyxOAuthTokens { refresh_token, .. }) => {
                // Parse the API URL.
                let mut url = self.api.clone();
                url.set_path("auth/cli/refresh");

                let mut request = reqwest::Request::new(reqwest::Method::POST, Url::from(url));
                let body = serde_json::json!({
                    "refresh_token": refresh_token
                });
                *request.body_mut() = Some(body.to_string().into());

                let response = client.execute(request).await?;
                let tokens = response
                    .error_for_status()?
                    .json::<PyxOAuthTokens>()
                    .await?;
                PyxTokens::OAuth(tokens)
            }
            PyxTokens::ApiKey(PyxApiKeyTokens { api_key, .. }) => {
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
                PyxTokens::ApiKey(PyxApiKeyTokens {
                    access_token,
                    api_key,
                })
            }
        };

        // Write the new tokens to disk.
        self.write(&tokens).await?;
        Ok(tokens)
    }

    /// Returns `true` if the given URL is "known" to this token store (i.e., should be
    /// authenticated using the store's tokens).
    pub fn is_known_url(&self, url: &Url) -> bool {
        is_known_url(url, &self.api, &self.cdn)
    }

    /// Returns `true` if the URL is on a "known" domain (i.e., the same domain as the API or CDN).
    ///
    /// Like [`is_known_url`](Self::is_known_url), but also returns `true` if the API is on the
    /// subdomain of the URL (e.g., if the API is `api.pyx.dev` and the URL is `pyx.dev`).
    pub fn is_known_domain(&self, url: &Url) -> bool {
        is_known_domain(url, &self.api, &self.cdn)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TokenStoreError {
    #[error(transparent)]
    Url(#[from] DisplaySafeUrlError),
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
pub struct PyxJwt {
    /// The expiration time of the JWT, as a Unix timestamp.
    pub exp: Option<i64>,
    /// The issuer of the JWT.
    pub iss: Option<String>,
    /// The name of the organization, if any.
    #[serde(rename = "urn:pyx:org_name")]
    pub name: Option<String>,
}

impl PyxJwt {
    /// Decode the JWT from the access token.
    pub fn decode(access_token: &AccessToken) -> Result<Self, JwtError> {
        let mut token_segments = access_token.as_str().splitn(3, '.');

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

fn is_known_url(url: &Url, api: &DisplaySafeUrl, cdn: &str) -> bool {
    // Determine whether the URL matches the API realm.
    if Realm::from(url) == Realm::from(&**api) {
        return true;
    }

    // Determine whether the URL matches the CDN domain (or a subdomain of it).
    //
    // For example, if URL is on `files.astralhosted.com` and the CDN domain is
    // `astralhosted.com`, consider it known.
    if matches!(url.scheme(), "https") && matches_domain(url, cdn) {
        return true;
    }

    false
}

fn is_known_domain(url: &Url, api: &DisplaySafeUrl, cdn: &str) -> bool {
    // Determine whether the URL matches the API domain.
    if let Some(domain) = url.domain() {
        if matches_domain(api, domain) {
            return true;
        }
    }
    is_known_url(url, api, cdn)
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
    fn test_is_known_url() {
        let api_url = DisplaySafeUrl::parse("https://api.pyx.dev").unwrap();
        let cdn_domain = "astralhosted.com";

        // Same realm as API.
        assert!(is_known_url(
            &Url::parse("https://api.pyx.dev/simple/").unwrap(),
            &api_url,
            cdn_domain
        ));

        // Different path on same API domain
        assert!(is_known_url(
            &Url::parse("https://api.pyx.dev/v1/").unwrap(),
            &api_url,
            cdn_domain
        ));

        // CDN domain.
        assert!(is_known_url(
            &Url::parse("https://astralhosted.com/packages/").unwrap(),
            &api_url,
            cdn_domain
        ));

        // CDN subdomain.
        assert!(is_known_url(
            &Url::parse("https://files.astralhosted.com/packages/").unwrap(),
            &api_url,
            cdn_domain
        ));

        // CDN on HTTP.
        assert!(!is_known_url(
            &Url::parse("http://astralhosted.com/packages/").unwrap(),
            &api_url,
            cdn_domain
        ));

        // Unknown domain.
        assert!(!is_known_url(
            &Url::parse("https://pypi.org/simple/").unwrap(),
            &api_url,
            cdn_domain
        ));

        // Similar but not matching domain.
        assert!(!is_known_url(
            &Url::parse("https://badastralhosted.com/packages/").unwrap(),
            &api_url,
            cdn_domain
        ));
    }

    #[test]
    fn test_is_known_domain() {
        let api_url = DisplaySafeUrl::parse("https://api.pyx.dev").unwrap();
        let cdn_domain = "astralhosted.com";

        // Same realm as API.
        assert!(is_known_domain(
            &Url::parse("https://api.pyx.dev/simple/").unwrap(),
            &api_url,
            cdn_domain
        ));

        // API super-domain.
        assert!(is_known_domain(
            &Url::parse("https://pyx.dev").unwrap(),
            &api_url,
            cdn_domain
        ));

        // API subdomain.
        assert!(!is_known_domain(
            &Url::parse("https://foo.api.pyx.dev").unwrap(),
            &api_url,
            cdn_domain
        ));

        // Different subdomain.
        assert!(!is_known_domain(
            &Url::parse("https://beta.pyx.dev/").unwrap(),
            &api_url,
            cdn_domain
        ));

        // CDN domain.
        assert!(is_known_domain(
            &Url::parse("https://astralhosted.com/packages/").unwrap(),
            &api_url,
            cdn_domain
        ));

        // CDN subdomain.
        assert!(is_known_domain(
            &Url::parse("https://files.astralhosted.com/packages/").unwrap(),
            &api_url,
            cdn_domain
        ));

        // Unknown domain.
        assert!(!is_known_domain(
            &Url::parse("https://pypi.org/simple/").unwrap(),
            &api_url,
            cdn_domain
        ));

        // Different TLD.
        assert!(!is_known_domain(
            &Url::parse("https://pyx.com/").unwrap(),
            &api_url,
            cdn_domain
        ));
    }

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

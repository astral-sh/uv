use std::sync::{Arc, LazyLock};

use tracing::trace;

use uv_redacted::DisplaySafeUrl;

use crate::credentials::Authentication;
pub use access_token::AccessToken;
use cache::CredentialsCache;
pub use credentials::{Credentials, Username};
pub use index::{AuthPolicy, Index, Indexes};
pub use keyring::KeyringProvider;
pub use middleware::AuthMiddleware;
pub use pyx::{
    DEFAULT_TOLERANCE_SECS, PyxJwt, PyxOAuthTokens, PyxTokenStore, PyxTokens, TokenStoreError,
};
pub use realm::{Realm, RealmRef};
pub use service::{Service, ServiceParseError};
pub use store::{AuthBackend, AuthScheme, TextCredentialStore, TomlCredentialError};

mod access_token;
mod cache;
mod credentials;
mod index;
mod keyring;
mod middleware;
mod providers;
mod pyx;
mod realm;
mod service;
mod store;

// TODO(zanieb): Consider passing a cache explicitly throughout

/// Global authentication cache for a uv invocation
///
/// This is used to share credentials across uv clients.
pub(crate) static CREDENTIALS_CACHE: LazyLock<CredentialsCache> =
    LazyLock::new(CredentialsCache::default);

/// Populate the global authentication store with credentials on a URL, if there are any.
///
/// Returns `true` if the store was updated.
pub fn store_credentials_from_url(url: &DisplaySafeUrl) -> bool {
    if let Some(credentials) = Credentials::from_url(url) {
        trace!("Caching credentials for {url}");

        // If credentials already exist in the cache for this URL, do not override them
        // with URL-derived credentials. This ensures credentials provided via environment
        // variables (or other higher-precedence sources) are not replaced by credentials
        // embedded in an index URL.
        if CREDENTIALS_CACHE.get_url(url, &Username::none()).is_some() {
            trace!("Skipping caching credentials for {url}: credentials already present");
            return false;
        }

        CREDENTIALS_CACHE.insert(url, Arc::new(Authentication::from(credentials)));
        true
    } else {
        false
    }
}

/// Populate the global authentication store with credentials on a URL, if there are any.
///
/// Returns `true` if the store was updated.
pub fn store_credentials(url: &DisplaySafeUrl, credentials: Credentials) {
    trace!("Caching credentials for {url}");
    CREDENTIALS_CACHE.insert(url, Arc::new(Authentication::from(credentials)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_credentials_from_url_does_not_override_existing_cache() {
        use crate::credentials::Credentials;

        let base = DisplaySafeUrl::parse("https://example.com/simple/real").unwrap();
        // Seed the cache with env-var-like credentials using store_credentials
        let env_creds =
            Credentials::basic(Some("envuser".to_string()), Some("envpass".to_string()));
        store_credentials(&base, env_creds.clone());

        // Construct a URL with embedded credentials that would otherwise override the cache
        let mut url_with_creds = base.clone();
        let _ = url_with_creds.set_username("urluser");
        let _ = url_with_creds.set_password(Some("urlpass"));

        // Attempt to store credentials from the URL; expect we will not override existing env creds
        assert!(!store_credentials_from_url(&url_with_creds));

        // Ensure the cached credentials are the original env creds
        let cached = CREDENTIALS_CACHE.get_url(&base, &Username::none()).unwrap();
        assert_eq!(cached.username(), env_creds.username());
        assert_eq!(cached.password(), env_creds.password());
    }
}

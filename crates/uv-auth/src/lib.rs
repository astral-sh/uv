use std::sync::{Arc, LazyLock};

use tracing::trace;

use uv_redacted::DisplaySafeUrl;

use cache::CredentialsCache;
pub use credentials::Credentials;
pub use index::{AuthPolicy, Index, Indexes};
pub use keyring::KeyringProvider;
pub use middleware::AuthMiddleware;
pub use realm::Realm;
pub use service::{
    AccessToken, DEFAULT_TOLERANCE_SECS, OAuthTokens, TokenStore, TokenStoreError, Tokens,
};

mod cache;
mod credentials;
mod index;
mod keyring;
mod middleware;
mod providers;
mod realm;
mod service;
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
        CREDENTIALS_CACHE.insert(url, Arc::new(credentials));
        true
    } else {
        false
    }
}

/// Populate the global authentication store with credentials on a URL, if there are any.
///
/// Returns `true` if the store was updated.
pub fn store_credentials(url: &DisplaySafeUrl, credentials: Arc<Credentials>) {
    trace!("Caching credentials for {url}");
    CREDENTIALS_CACHE.insert(url, credentials);
}

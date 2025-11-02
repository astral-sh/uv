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

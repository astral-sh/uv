use std::sync::{Arc, LazyLock};

use tracing::trace;
use url::Url;

use cache::CredentialsCache;
pub use credentials::Credentials;
pub use keyring::KeyringProvider;
pub use middleware::AuthMiddleware;
use realm::Realm;

mod cache;
mod credentials;
mod keyring;
mod middleware;
mod realm;

// TODO(zanieb): Consider passing a cache explicitly throughout

/// Global authentication cache for a uv invocation
///
/// This is used to share credentials across uv clients.
pub(crate) static CREDENTIALS_CACHE: LazyLock<CredentialsCache> =
    LazyLock::new(CredentialsCache::default);

/// Populate the global authentication store with credentials on a URL, if there are any.
///
/// Returns `true` if the store was updated.
pub fn store_credentials_from_url(url: &Url) -> bool {
    if let Some(credentials) = Credentials::from_url(url) {
        trace!("Caching credentials for {url}");
        CREDENTIALS_CACHE.insert(url, Arc::new(credentials));
        true
    } else {
        false
    }
}

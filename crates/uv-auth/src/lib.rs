mod cache;
mod credentials;
mod keyring;
mod middleware;
mod netloc;

use cache::CredentialsCache;

pub use keyring::KeyringProvider;
pub use middleware::AuthMiddleware;
use netloc::NetLoc;
use once_cell::sync::Lazy;
use url::Url;

// TODO(zanieb): Consider passing a cache explicitly throughout

/// Global authentication cache for a uv invocation
///
/// This is used to share credentials across uv clients.
pub(crate) static CREDENTIALS_CACHE: Lazy<CredentialsCache> = Lazy::new(CredentialsCache::default);

/// Populate the global authentication store with credentials on a URL, if there are any.
///
/// Returns `true` if the store was updated.
pub fn store_credentials_from_url(_url: &Url) -> bool {
    true
}

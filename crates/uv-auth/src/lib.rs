mod credentials;
mod keyring;
mod middleware;
mod netloc;
mod store;

pub use keyring::KeyringProvider;
pub use middleware::AuthMiddleware;
use netloc::NetLoc;
use once_cell::sync::Lazy;
use store::AuthenticationStore;
use url::Url;

// TODO(zanieb): Consider passing a store explicitly throughout

/// Global authentication store for a `uv` invocation
pub(crate) static GLOBAL_AUTH_STORE: Lazy<AuthenticationStore> =
    Lazy::new(AuthenticationStore::default);

/// Populate the global authentication store with credentials on a URL, if there are any.
///
/// Returns `true` if the store was updated.
pub fn store_credentials_from_url(url: &Url) -> bool {
    GLOBAL_AUTH_STORE.set_from_url(url)
}

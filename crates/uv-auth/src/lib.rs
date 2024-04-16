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

// TODO(zanieb): Consider passing a cache explicitly throughout

/// Global authentication cache for a uv invocation
///
/// This is used to share credentials across uv clients.
pub(crate) static CREDENTIALS_CACHE: Lazy<CredentialsCache> = Lazy::new(CredentialsCache::default);

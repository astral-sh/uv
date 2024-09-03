use std::sync::{Arc, LazyLock};

use tracing::{debug, trace};
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

/// Populate the global authentication store with credentials from the environment, if any.
///
/// Respects `UV_BASIC_AUTH_URLS` with whitespace separated URLs.
///
/// Supports formats like:
///
/// - `UV_BASIC_AUTH_URLS="username:password@hostname"`
/// - `UV_BASIC_AUTH_URLS="username@hostname"`
/// - `UV_BASIC_AUTH_URLS="username:password@hostname username:password@other-hostname"`
/// - `UV_BASIC_AUTH_URLS="username:password@hostname/path"`
///
/// Only `https` URLs are supported, URLs with other schemes are ignored.
///
/// Populates the URL prefix cache in the global [`CredentialsCache`]. The credentials are not
/// added the the realm-level cache to ensure that a URL with a path is not used unless a request
/// is made to a child of the provided URL.
pub fn store_credentials_from_environment() -> bool {
    let Some(urls) = std::env::var_os("UV_BASIC_AUTH_URLS") else {
        return false;
    };

    let mut populated = false;
    for mut url in urls
        .to_string_lossy()
        .split_ascii_whitespace()
        .filter_map(parse_url_from_env)
    {
        if let Some(credentials) = Credentials::from_url(&url) {
            CREDENTIALS_CACHE.insert_url(&url, Arc::new(credentials));
            // Redact the password for display
            if url.password().is_some() {
                let _ = url.set_password(Some("***"));
            }
            debug!("Added credentials for `{url}`");
            populated = true;
        } else {
            debug!("Ignoring URL without credentials in `UV_BASIC_AUTH_URLS`: {url}");
        }
    }

    populated
}

/// Parse a URL, allowing the scheme to be missing (and inferred as `https://`)
fn parse_url_from_env(url: &str) -> Option<Url> {
    if url.starts_with("https://") {
        Url::parse(url)
    } else if url.starts_with("http://") {
        debug!("Ignoring insecure URL in `UV_BASIC_AUTH_URLS`: {url}");
        return None;
    } else {
        let url = format!("https://{url}");
        Url::parse(&url)
    }
    .inspect_err(|err| debug!("Ignoring invalid URL in `UV_BASIC_AUTH_URLS`: {err}"))
    .ok()
}

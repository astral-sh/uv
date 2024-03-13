mod keyring;
mod middleware;
mod store;

pub use keyring::KeyringProvider;
pub use middleware::AuthMiddleware;
use once_cell::sync::Lazy;
pub use store::AuthenticationStore;

use url::Url;

// TODO(zanieb): Consider passing a store explicitly throughout

/// Global authentication store for a `uv` invocation
pub static GLOBAL_AUTH_STORE: Lazy<AuthenticationStore> = Lazy::new(AuthenticationStore::default);

/// Used to determine if authentication information should be retained on a new URL.
/// Based on the specification defined in RFC 7235 and 7230.
///
/// <https://datatracker.ietf.org/doc/html/rfc7235#section-2.2>
/// <https://datatracker.ietf.org/doc/html/rfc7230#section-5.5>
//
// The "scheme" and "authority" components must match to retain authentication
// The "authority", is composed of the host and port.
//
// The scheme must always be an exact match.
// Note some clients such as Python's `requests` library allow an upgrade
// from `http` to `https` but this is not spec-compliant.
// <https://github.com/pypa/pip/blob/75f54cae9271179b8cc80435f92336c97e349f9d/src/pip/_vendor/requests/sessions.py#L133-L136>
//
// The host must always be an exact match.
//
// The port is only allowed to differ if it it matches the "default port" for the scheme.
// However, `url` (and therefore `reqwest`) sets the `port` to `None` if it matches the default port
// so we do not need any special handling here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct NetLoc {
    scheme: String,
    host: Option<String>,
    port: Option<u16>,
}

impl From<&Url> for NetLoc {
    fn from(url: &Url) -> Self {
        Self {
            scheme: url.scheme().to_string(),
            host: url.host_str().map(str::to_string),
            port: url.port(),
        }
    }
}

#[cfg(test)]
mod tests {
    use url::{ParseError, Url};

    use crate::NetLoc;

    #[test]
    fn test_should_retain_auth() -> Result<(), ParseError> {
        // Exact match (https)
        assert_eq!(
            NetLoc::from(&Url::parse("https://example.com")?),
            NetLoc::from(&Url::parse("https://example.com")?)
        );

        // Exact match (with port)
        assert_eq!(
            NetLoc::from(&Url::parse("https://example.com:1234")?),
            NetLoc::from(&Url::parse("https://example.com:1234")?)
        );

        // Exact match (http)
        assert_eq!(
            NetLoc::from(&Url::parse("http://example.com")?),
            NetLoc::from(&Url::parse("http://example.com")?)
        );

        // Okay, path differs
        assert_eq!(
            NetLoc::from(&Url::parse("http://example.com/foo")?),
            NetLoc::from(&Url::parse("http://example.com/bar")?)
        );

        // Okay, default port differs (https)
        assert_eq!(
            NetLoc::from(&Url::parse("https://example.com:443")?),
            NetLoc::from(&Url::parse("https://example.com")?)
        );

        // Okay, default port differs (http)
        assert_eq!(
            NetLoc::from(&Url::parse("http://example.com:80")?),
            NetLoc::from(&Url::parse("http://example.com")?)
        );

        // Mismatched scheme
        assert_ne!(
            NetLoc::from(&Url::parse("https://example.com")?),
            NetLoc::from(&Url::parse("http://example.com")?)
        );

        // Mismatched scheme, we explicitly do not allow upgrade to https
        assert_ne!(
            NetLoc::from(&Url::parse("http://example.com")?),
            NetLoc::from(&Url::parse("https://example.com")?)
        );

        // Mismatched host
        assert_ne!(
            NetLoc::from(&Url::parse("https://foo.com")?),
            NetLoc::from(&Url::parse("https://bar.com")?)
        );

        // Mismatched port
        assert_ne!(
            NetLoc::from(&Url::parse("https://example.com:1234")?),
            NetLoc::from(&Url::parse("https://example.com:5678")?)
        );

        // Mismatched port, with one as default for scheme
        assert_ne!(
            NetLoc::from(&Url::parse("https://example.com:443")?),
            NetLoc::from(&Url::parse("https://example.com:5678")?)
        );
        assert_ne!(
            NetLoc::from(&Url::parse("https://example.com:1234")?),
            NetLoc::from(&Url::parse("https://example.com:443")?)
        );

        // Mismatched port, with default for a different scheme
        assert_ne!(
            NetLoc::from(&Url::parse("https://example.com:80")?),
            NetLoc::from(&Url::parse("https://example.com")?)
        );

        Ok(())
    }
}

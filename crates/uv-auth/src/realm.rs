use std::{fmt::Display, fmt::Formatter};

use url::Url;
use uv_small_str::SmallString;

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
// The port is only allowed to differ if it matches the "default port" for the scheme.
// However, `url` (and therefore `reqwest`) sets the `port` to `None` if it matches the default port
// so we do not need any special handling here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Realm {
    scheme: SmallString,
    host: Option<SmallString>,
    port: Option<u16>,
}

impl From<&Url> for Realm {
    fn from(url: &Url) -> Self {
        Self {
            scheme: SmallString::from(url.scheme()),
            host: url.host_str().map(SmallString::from),
            port: url.port(),
        }
    }
}

impl Display for Realm {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(port) = self.port {
            write!(
                f,
                "{}://{}:{port}",
                self.scheme,
                self.host.as_deref().unwrap_or_default()
            )
        } else {
            write!(
                f,
                "{}://{}",
                self.scheme,
                self.host.as_deref().unwrap_or_default()
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use url::{ParseError, Url};

    use crate::Realm;

    #[test]
    fn test_should_retain_auth() -> Result<(), ParseError> {
        // Exact match (https)
        assert_eq!(
            Realm::from(&Url::parse("https://example.com")?),
            Realm::from(&Url::parse("https://example.com")?)
        );

        // Exact match (with port)
        assert_eq!(
            Realm::from(&Url::parse("https://example.com:1234")?),
            Realm::from(&Url::parse("https://example.com:1234")?)
        );

        // Exact match (http)
        assert_eq!(
            Realm::from(&Url::parse("http://example.com")?),
            Realm::from(&Url::parse("http://example.com")?)
        );

        // Okay, path differs
        assert_eq!(
            Realm::from(&Url::parse("http://example.com/foo")?),
            Realm::from(&Url::parse("http://example.com/bar")?)
        );

        // Okay, default port differs (https)
        assert_eq!(
            Realm::from(&Url::parse("https://example.com:443")?),
            Realm::from(&Url::parse("https://example.com")?)
        );

        // Okay, default port differs (http)
        assert_eq!(
            Realm::from(&Url::parse("http://example.com:80")?),
            Realm::from(&Url::parse("http://example.com")?)
        );

        // Mismatched scheme
        assert_ne!(
            Realm::from(&Url::parse("https://example.com")?),
            Realm::from(&Url::parse("http://example.com")?)
        );

        // Mismatched scheme, we explicitly do not allow upgrade to https
        assert_ne!(
            Realm::from(&Url::parse("http://example.com")?),
            Realm::from(&Url::parse("https://example.com")?)
        );

        // Mismatched host
        assert_ne!(
            Realm::from(&Url::parse("https://foo.com")?),
            Realm::from(&Url::parse("https://bar.com")?)
        );

        // Mismatched port
        assert_ne!(
            Realm::from(&Url::parse("https://example.com:1234")?),
            Realm::from(&Url::parse("https://example.com:5678")?)
        );

        // Mismatched port, with one as default for scheme
        assert_ne!(
            Realm::from(&Url::parse("https://example.com:443")?),
            Realm::from(&Url::parse("https://example.com:5678")?)
        );
        assert_ne!(
            Realm::from(&Url::parse("https://example.com:1234")?),
            Realm::from(&Url::parse("https://example.com:443")?)
        );

        // Mismatched port, with default for a different scheme
        assert_ne!(
            Realm::from(&Url::parse("https://example.com:80")?),
            Realm::from(&Url::parse("https://example.com")?)
        );

        Ok(())
    }
}

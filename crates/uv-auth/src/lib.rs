/// HTTP authentication utilities.
use tracing::warn;
use url::Url;

/// Optimized version of [`safe_copy_url_auth`] which avoids parsing a string
/// into a URL unless the given URL has authentication to copy. Useful for patterns
/// where the returned URL would immediately be cast into a string.
///
/// Returns [`Err`] if there is authentication to copy and `new_url` is not a valid URL.
/// Returns [`None`] if there is no authentication to copy.
pub fn safe_copy_url_auth_to_str(
    trusted_url: &Url,
    new_url: &str,
) -> Result<Option<Url>, url::ParseError> {
    if trusted_url.username().is_empty() && trusted_url.password().is_none() {
        return Ok(None);
    }

    let new_url = Url::parse(new_url)?;
    Ok(Some(safe_copy_url_auth(trusted_url, new_url)))
}

/// Copy authentication from one URL to another URL if applicable.
///
/// See [`should_retain_auth`] for details on when authentication is retained.
#[must_use]
pub fn safe_copy_url_auth(trusted_url: &Url, mut new_url: Url) -> Url {
    if should_retain_auth(trusted_url, &new_url) {
        new_url
            .set_username(trusted_url.username())
            .unwrap_or_else(|()| warn!("Failed to transfer username to response URL: {new_url}"));
        new_url
            .set_password(trusted_url.password())
            .unwrap_or_else(|()| warn!("Failed to transfer password to response URL: {new_url}"));
    }
    new_url
}

/// Determine if authentication information should be retained on a new URL.
/// Implements the specification defined in RFC 7235 and 7230.
///
/// <https://datatracker.ietf.org/doc/html/rfc7235#section-2.2>
/// <https://datatracker.ietf.org/doc/html/rfc7230#section-5.5>
fn should_retain_auth(trusted_url: &Url, new_url: &Url) -> bool {
    // The "scheme" and "authority" components must match to retain authentication
    // The "authority", is composed of the host and port.

    // Check the scheme.
    // The scheme must always be an exact match.
    // Note some clients such as Python's `requests` library allow an upgrade
    // from `http` to `https` but this is not spec-compliant.
    // <https://github.com/pypa/pip/blob/75f54cae9271179b8cc80435f92336c97e349f9d/src/pip/_vendor/requests/sessions.py#L133-L136>
    if trusted_url.scheme() != new_url.scheme() {
        return false;
    }

    // The host must always be an exact match.
    if trusted_url.host() != new_url.host() {
        return false;
    }

    // Check the port.
    // The port is only allowed to differ if it it matches the "default port" for the scheme.
    // However, `reqwest` sets the `port` to `None` if it matches the default port so we do
    // not need any special handling here.
    if trusted_url.port() != new_url.port() {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use url::{ParseError, Url};

    use crate::should_retain_auth;

    #[test]
    fn test_should_retain_auth() -> Result<(), ParseError> {
        // Exact match (https)
        assert!(should_retain_auth(
            &Url::parse("https://example.com")?,
            &Url::parse("https://example.com")?,
        ));

        // Exact match (with port)
        assert!(should_retain_auth(
            &Url::parse("https://example.com:1234")?,
            &Url::parse("https://example.com:1234")?,
        ));

        // Exact match (http)
        assert!(should_retain_auth(
            &Url::parse("http://example.com")?,
            &Url::parse("http://example.com")?,
        ));

        // Okay, path differs
        assert!(should_retain_auth(
            &Url::parse("http://example.com/foo")?,
            &Url::parse("http://example.com/bar")?,
        ));

        // Okay, default port differs (https)
        assert!(should_retain_auth(
            &Url::parse("https://example.com:443")?,
            &Url::parse("https://example.com")?,
        ));
        assert!(should_retain_auth(
            &Url::parse("https://example.com")?,
            &Url::parse("https://example.com:443")?,
        ));

        // Okay, default port differs (http)
        assert!(should_retain_auth(
            &Url::parse("http://example.com:80")?,
            &Url::parse("http://example.com")?,
        ));
        assert!(should_retain_auth(
            &Url::parse("http://example.com")?,
            &Url::parse("http://example.com:80")?,
        ));

        // Mismatched scheme
        assert!(!should_retain_auth(
            &Url::parse("https://example.com")?,
            &Url::parse("http://example.com")?,
        ));

        // Mismatched scheme, we explicitly do not allow upgrade to https
        assert!(!should_retain_auth(
            &Url::parse("http://example.com")?,
            &Url::parse("https://example.com")?,
        ));

        // Mismatched host
        assert!(!should_retain_auth(
            &Url::parse("https://foo.com")?,
            &Url::parse("https://bar.com")?,
        ));

        // Mismatched port
        assert!(!should_retain_auth(
            &Url::parse("https://example.com:1234")?,
            &Url::parse("https://example.com:5678")?,
        ));

        // Mismatched port, with one as default for scheme
        assert!(!should_retain_auth(
            &Url::parse("https://example.com:443")?,
            &Url::parse("https://example.com:5678")?,
        ));
        assert!(!should_retain_auth(
            &Url::parse("https://example.com:1234")?,
            &Url::parse("https://example.com:443")?,
        ));

        // Mismatched port, with default for a different scheme
        assert!(!should_retain_auth(
            &Url::parse("https://example.com")?,
            &Url::parse("https://example.com:80")?,
        ));
        assert!(!should_retain_auth(
            &Url::parse("https://example.com:80")?,
            &Url::parse("https://example.com")?,
        ));

        Ok(())
    }
}

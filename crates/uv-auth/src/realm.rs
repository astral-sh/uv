use std::hash::{Hash, Hasher};
use std::{fmt::Display, fmt::Formatter};
use url::Url;
use uv_redacted::DisplaySafeUrl;
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
#[derive(Debug, Clone)]
pub struct Realm {
    scheme: SmallString,
    host: Option<SmallString>,
    port: Option<u16>,
}

impl From<&DisplaySafeUrl> for Realm {
    fn from(url: &DisplaySafeUrl) -> Self {
        Self::from(&**url)
    }
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

impl PartialEq for Realm {
    fn eq(&self, other: &Self) -> bool {
        RealmRef::from(self) == RealmRef::from(other)
    }
}

impl Eq for Realm {}

impl Hash for Realm {
    fn hash<H: Hasher>(&self, state: &mut H) {
        RealmRef::from(self).hash(state);
    }
}

/// A reference to a [`Realm`] that can be used for zero-allocation comparisons.
#[derive(Debug, Copy, Clone)]
pub struct RealmRef<'a> {
    scheme: &'a str,
    host: Option<&'a str>,
    port: Option<u16>,
}

impl RealmRef<'_> {
    /// Returns true if this realm is a subdomain of the other realm.
    pub(crate) fn is_subdomain_of(&self, other: Self) -> bool {
        other.scheme == self.scheme
            && other.port == self.port
            && other.host.is_some_and(|other_host| {
                self.host.is_some_and(|self_host| {
                    self_host
                        .strip_suffix(other_host)
                        .is_some_and(|prefix| prefix.ends_with('.'))
                })
            })
    }
}

impl<'a> From<&'a Url> for RealmRef<'a> {
    fn from(url: &'a Url) -> Self {
        Self {
            scheme: url.scheme(),
            host: url.host_str(),
            port: url.port(),
        }
    }
}

impl PartialEq for RealmRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.scheme == other.scheme && self.host == other.host && self.port == other.port
    }
}

impl Eq for RealmRef<'_> {}

impl Hash for RealmRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.scheme.hash(state);
        self.host.hash(state);
        self.port.hash(state);
    }
}

impl<'a> PartialEq<RealmRef<'a>> for Realm {
    fn eq(&self, rhs: &RealmRef<'a>) -> bool {
        RealmRef::from(self) == *rhs
    }
}

impl PartialEq<Realm> for RealmRef<'_> {
    fn eq(&self, rhs: &Realm) -> bool {
        *self == RealmRef::from(rhs)
    }
}

impl<'a> From<&'a Realm> for RealmRef<'a> {
    fn from(realm: &'a Realm) -> Self {
        Self {
            scheme: &realm.scheme,
            host: realm.host.as_deref(),
            port: realm.port,
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

    #[test]
    fn test_is_subdomain_of() -> Result<(), ParseError> {
        use crate::realm::RealmRef;

        // Subdomain relationship: sub.example.com is a subdomain of example.com
        let subdomain_url = Url::parse("https://sub.example.com")?;
        let domain_url = Url::parse("https://example.com")?;
        let subdomain = RealmRef::from(&subdomain_url);
        let domain = RealmRef::from(&domain_url);
        assert!(subdomain.is_subdomain_of(domain));

        // Deeper subdomain: foo.bar.example.com is a subdomain of example.com
        let deep_subdomain_url = Url::parse("https://foo.bar.example.com")?;
        let deep_subdomain = RealmRef::from(&deep_subdomain_url);
        assert!(deep_subdomain.is_subdomain_of(domain));

        // Deeper subdomain: foo.bar.example.com is also a subdomain of bar.example.com
        let parent_subdomain_url = Url::parse("https://bar.example.com")?;
        let parent_subdomain = RealmRef::from(&parent_subdomain_url);
        assert!(deep_subdomain.is_subdomain_of(parent_subdomain));

        // Not a subdomain: example.com is not a subdomain of sub.example.com
        assert!(!domain.is_subdomain_of(subdomain));

        // Same domain is not a subdomain of itself
        assert!(!domain.is_subdomain_of(domain));

        // Different TLD: example.org is not a subdomain of example.com
        let different_tld_url = Url::parse("https://example.org")?;
        let different_tld = RealmRef::from(&different_tld_url);
        assert!(!different_tld.is_subdomain_of(domain));

        // Partial match but not a subdomain: notexample.com is not a subdomain of example.com
        let partial_match_url = Url::parse("https://notexample.com")?;
        let partial_match = RealmRef::from(&partial_match_url);
        assert!(!partial_match.is_subdomain_of(domain));

        // Different scheme: http subdomain is not a subdomain of https domain
        let http_subdomain_url = Url::parse("http://sub.example.com")?;
        let https_domain_url = Url::parse("https://example.com")?;
        let http_subdomain = RealmRef::from(&http_subdomain_url);
        let https_domain = RealmRef::from(&https_domain_url);
        assert!(!http_subdomain.is_subdomain_of(https_domain));

        // Different port: same subdomain with different port is not a subdomain
        let subdomain_port_8080_url = Url::parse("https://sub.example.com:8080")?;
        let domain_port_9090_url = Url::parse("https://example.com:9090")?;
        let subdomain_port_8080 = RealmRef::from(&subdomain_port_8080_url);
        let domain_port_9090 = RealmRef::from(&domain_port_9090_url);
        assert!(!subdomain_port_8080.is_subdomain_of(domain_port_9090));

        // Same port: subdomain with same explicit port is a subdomain
        let subdomain_with_port_url = Url::parse("https://sub.example.com:8080")?;
        let domain_with_port_url = Url::parse("https://example.com:8080")?;
        let subdomain_with_port = RealmRef::from(&subdomain_with_port_url);
        let domain_with_port = RealmRef::from(&domain_with_port_url);
        assert!(subdomain_with_port.is_subdomain_of(domain_with_port));

        // Default port handling: subdomain with implicit port is a subdomain
        let subdomain_default_url = Url::parse("https://sub.example.com")?;
        let domain_explicit_443_url = Url::parse("https://example.com:443")?;
        let subdomain_default = RealmRef::from(&subdomain_default_url);
        let domain_explicit_443 = RealmRef::from(&domain_explicit_443_url);
        assert!(subdomain_default.is_subdomain_of(domain_explicit_443));

        // Edge case: empty host (shouldn't happen with valid URLs but testing defensive code)
        let file_url = Url::parse("file:///path/to/file")?;
        let https_url = Url::parse("https://example.com")?;
        let file_realm = RealmRef::from(&file_url);
        let https_realm = RealmRef::from(&https_url);
        assert!(!file_realm.is_subdomain_of(https_realm));
        assert!(!https_realm.is_subdomain_of(file_realm));

        // Subdomain with path (path should be ignored)
        let subdomain_with_path_url = Url::parse("https://sub.example.com/path")?;
        let domain_with_path_url = Url::parse("https://example.com/other")?;
        let subdomain_with_path = RealmRef::from(&subdomain_with_path_url);
        let domain_with_path = RealmRef::from(&domain_with_path_url);
        assert!(subdomain_with_path.is_subdomain_of(domain_with_path));

        Ok(())
    }
}

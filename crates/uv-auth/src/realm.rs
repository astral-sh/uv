use std::{fmt::Display, fmt::Formatter};

use url::Url;

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
    scheme: String,
    host: Option<String>,
    port: Option<u16>,
}

impl From<&Url> for Realm {
    fn from(url: &Url) -> Self {
        Self {
            scheme: url.scheme().to_string(),
            host: url.host_str().map(str::to_string),
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
mod tests;

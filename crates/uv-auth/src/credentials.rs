use base64::prelude::BASE64_STANDARD;
use base64::read::DecoderReader;
use base64::write::EncoderWriter;
use std::borrow::Cow;
use std::fmt;
use uv_redacted::DisplaySafeUrl;

use netrc::Netrc;
use reqwest::Request;
use reqwest::header::HeaderValue;
use std::io::Read;
use std::io::Write;
use url::Url;

use uv_static::EnvVars;

#[derive(Clone, Debug, PartialEq)]
pub enum Credentials {
    Basic {
        /// The username to use for authentication.
        username: Username,
        /// The password to use for authentication.
        password: Option<Password>,
    },
    Bearer {
        /// The token to use for authentication.
        token: Vec<u8>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash, Default)]
pub struct Username(Option<String>);

impl Username {
    /// Create a new username.
    ///
    /// Unlike `reqwest`, empty usernames are be encoded as `None` instead of an empty string.
    pub(crate) fn new(value: Option<String>) -> Self {
        // Ensure empty strings are `None`
        Self(value.filter(|s| !s.is_empty()))
    }

    pub(crate) fn none() -> Self {
        Self::new(None)
    }

    pub(crate) fn is_none(&self) -> bool {
        self.0.is_none()
    }

    pub(crate) fn is_some(&self) -> bool {
        self.0.is_some()
    }

    pub(crate) fn as_deref(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

impl From<String> for Username {
    fn from(value: String) -> Self {
        Self::new(Some(value))
    }
}

impl From<Option<String>> for Username {
    fn from(value: Option<String>) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Default)]
pub struct Password(String);

impl Password {
    pub fn new(password: String) -> Self {
        Self(password)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "****")
    }
}

impl Credentials {
    /// Create a set of HTTP Basic Authentication credentials.
    #[allow(dead_code)]
    pub fn basic(username: Option<String>, password: Option<String>) -> Self {
        Self::Basic {
            username: Username::new(username),
            password: password.map(Password),
        }
    }

    /// Create a set of Bearer Authentication credentials.
    #[allow(dead_code)]
    pub fn bearer(token: Vec<u8>) -> Self {
        Self::Bearer { token }
    }

    pub fn username(&self) -> Option<&str> {
        match self {
            Self::Basic { username, .. } => username.as_deref(),
            Self::Bearer { .. } => None,
        }
    }

    pub(crate) fn to_username(&self) -> Username {
        match self {
            Self::Basic { username, .. } => username.clone(),
            Self::Bearer { .. } => Username::none(),
        }
    }

    pub(crate) fn as_username(&self) -> Cow<'_, Username> {
        match self {
            Self::Basic { username, .. } => Cow::Borrowed(username),
            Self::Bearer { .. } => Cow::Owned(Username::none()),
        }
    }

    pub fn password(&self) -> Option<&str> {
        match self {
            Self::Basic { password, .. } => password.as_ref().map(Password::as_str),
            Self::Bearer { .. } => None,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        match self {
            Self::Basic { username, password } => username.is_none() && password.is_none(),
            Self::Bearer { token } => token.is_empty(),
        }
    }

    /// Return [`Credentials`] for a [`Url`] from a [`Netrc`] file, if any.
    ///
    /// If a username is provided, it must match the login in the netrc file or [`None`] is returned.
    pub(crate) fn from_netrc(
        netrc: &Netrc,
        url: &DisplaySafeUrl,
        username: Option<&str>,
    ) -> Option<Self> {
        let host = url.host_str()?;
        let entry = netrc
            .hosts
            .get(host)
            .or_else(|| netrc.hosts.get("default"))?;

        // Ensure the username matches if provided
        if username.is_some_and(|username| username != entry.login) {
            return None;
        }

        Some(Credentials::Basic {
            username: Username::new(Some(entry.login.clone())),
            password: Some(Password(entry.password.clone())),
        })
    }

    /// Parse [`Credentials`] from a URL, if any.
    ///
    /// Returns [`None`] if both [`Url::username`] and [`Url::password`] are not populated.
    pub fn from_url(url: &Url) -> Option<Self> {
        if url.username().is_empty() && url.password().is_none() {
            return None;
        }
        Some(Self::Basic {
            // Remove percent-encoding from URL credentials
            // See <https://github.com/pypa/pip/blob/06d21db4ff1ab69665c22a88718a4ea9757ca293/src/pip/_internal/utils/misc.py#L497-L499>
            username: if url.username().is_empty() {
                None
            } else {
                Some(
                    percent_encoding::percent_decode_str(url.username())
                        .decode_utf8()
                        .expect("An encoded username should always decode")
                        .into_owned(),
                )
            }
            .into(),
            password: url.password().map(|password| {
                Password(
                    percent_encoding::percent_decode_str(password)
                        .decode_utf8()
                        .expect("An encoded password should always decode")
                        .into_owned(),
                )
            }),
        })
    }

    /// Extract the [`Credentials`] from the environment, given a named source.
    ///
    /// For example, given a name of `"pytorch"`, search for `UV_INDEX_PYTORCH_USERNAME` and
    /// `UV_INDEX_PYTORCH_PASSWORD`.
    pub fn from_env(name: impl AsRef<str>) -> Option<Self> {
        let username = std::env::var(EnvVars::index_username(name.as_ref())).ok();
        let password = std::env::var(EnvVars::index_password(name.as_ref())).ok();
        if username.is_none() && password.is_none() {
            None
        } else {
            Some(Self::basic(username, password))
        }
    }

    /// Parse [`Credentials`] from an HTTP request, if any.
    ///
    /// Only HTTP Basic Authentication is supported.
    pub(crate) fn from_request(request: &Request) -> Option<Self> {
        // First, attempt to retrieve the credentials from the URL
        Self::from_url(request.url()).or(
            // Then, attempt to pull the credentials from the headers
            request
                .headers()
                .get(reqwest::header::AUTHORIZATION)
                .map(Self::from_header_value)?,
        )
    }

    /// Parse [`Credentials`] from an authorization header, if any.
    ///
    /// HTTP Basic and Bearer Authentication are both supported.
    /// [`None`] will be returned if another authorization scheme is detected.
    ///
    /// Panics if the authentication is not conformant to the HTTP Basic Authentication scheme:
    /// - The contents must be base64 encoded
    /// - There must be a `:` separator
    pub(crate) fn from_header_value(header: &HeaderValue) -> Option<Self> {
        // Parse a `Basic` authentication header.
        if let Some(mut value) = header.as_bytes().strip_prefix(b"Basic ") {
            let mut decoder = DecoderReader::new(&mut value, &BASE64_STANDARD);
            let mut buf = String::new();
            decoder
                .read_to_string(&mut buf)
                .expect("HTTP Basic Authentication should be base64 encoded");
            let (username, password) = buf
                .split_once(':')
                .expect("HTTP Basic Authentication should include a `:` separator");
            let username = if username.is_empty() {
                None
            } else {
                Some(username.to_string())
            };
            let password = if password.is_empty() {
                None
            } else {
                Some(password.to_string())
            };
            return Some(Self::Basic {
                username: Username::new(username),
                password: password.map(Password),
            });
        }

        // Parse a `Bearer` authentication header.
        if let Some(token) = header.as_bytes().strip_prefix(b"Bearer ") {
            return Some(Self::Bearer {
                token: token.to_vec(),
            });
        }

        None
    }

    /// Create an HTTP Basic Authentication header for the credentials.
    ///
    /// Panics if the username or password cannot be base64 encoded.
    pub fn to_header_value(&self) -> HeaderValue {
        match self {
            Self::Basic { .. } => {
                // See: <https://github.com/seanmonstar/reqwest/blob/2c11ef000b151c2eebeed2c18a7b81042220c6b0/src/util.rs#L3>
                let mut buf = b"Basic ".to_vec();
                {
                    let mut encoder = EncoderWriter::new(&mut buf, &BASE64_STANDARD);
                    write!(encoder, "{}:", self.username().unwrap_or_default())
                        .expect("Write to base64 encoder should succeed");
                    if let Some(password) = self.password() {
                        write!(encoder, "{password}")
                            .expect("Write to base64 encoder should succeed");
                    }
                }
                let mut header =
                    HeaderValue::from_bytes(&buf).expect("base64 is always valid HeaderValue");
                header.set_sensitive(true);
                header
            }
            Self::Bearer { token } => {
                let mut header = HeaderValue::from_bytes(&[b"Bearer ", token.as_slice()].concat())
                    .expect("Bearer token is always valid HeaderValue");
                header.set_sensitive(true);
                header
            }
        }
    }

    /// Apply the credentials to the given URL.
    ///
    /// Any existing credentials will be overridden.
    #[must_use]
    pub fn apply(&self, mut url: DisplaySafeUrl) -> DisplaySafeUrl {
        if let Some(username) = self.username() {
            let _ = url.set_username(username);
        }
        if let Some(password) = self.password() {
            let _ = url.set_password(Some(password));
        }
        url
    }

    /// Attach the credentials to the given request.
    ///
    /// Any existing credentials will be overridden.
    #[must_use]
    pub fn authenticate(&self, mut request: Request) -> Request {
        request
            .headers_mut()
            .insert(reqwest::header::AUTHORIZATION, Self::to_header_value(self));
        request
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;

    use super::*;

    #[test]
    fn from_url_no_credentials() {
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        assert_eq!(Credentials::from_url(url), None);
    }

    #[test]
    fn from_url_username_and_password() {
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_username("user").unwrap();
        auth_url.set_password(Some("password")).unwrap();
        let credentials = Credentials::from_url(&auth_url).unwrap();
        assert_eq!(credentials.username(), Some("user"));
        assert_eq!(credentials.password(), Some("password"));
    }

    #[test]
    fn from_url_no_username() {
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_password(Some("password")).unwrap();
        let credentials = Credentials::from_url(&auth_url).unwrap();
        assert_eq!(credentials.username(), None);
        assert_eq!(credentials.password(), Some("password"));
    }

    #[test]
    fn from_url_no_password() {
        let url = &Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_username("user").unwrap();
        let credentials = Credentials::from_url(&auth_url).unwrap();
        assert_eq!(credentials.username(), Some("user"));
        assert_eq!(credentials.password(), None);
    }

    #[test]
    fn authenticated_request_from_url() {
        let url = Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_username("user").unwrap();
        auth_url.set_password(Some("password")).unwrap();
        let credentials = Credentials::from_url(&auth_url).unwrap();

        let mut request = Request::new(reqwest::Method::GET, url);
        request = credentials.authenticate(request);

        let mut header = request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .expect("Authorization header should be set")
            .clone();
        header.set_sensitive(false);

        assert_debug_snapshot!(header, @r###""Basic dXNlcjpwYXNzd29yZA==""###);
        assert_eq!(Credentials::from_header_value(&header), Some(credentials));
    }

    #[test]
    fn authenticated_request_from_url_with_percent_encoded_user() {
        let url = Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_username("user@domain").unwrap();
        auth_url.set_password(Some("password")).unwrap();
        let credentials = Credentials::from_url(&auth_url).unwrap();

        let mut request = Request::new(reqwest::Method::GET, url);
        request = credentials.authenticate(request);

        let mut header = request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .expect("Authorization header should be set")
            .clone();
        header.set_sensitive(false);

        assert_debug_snapshot!(header, @r###""Basic dXNlckBkb21haW46cGFzc3dvcmQ=""###);
        assert_eq!(Credentials::from_header_value(&header), Some(credentials));
    }

    #[test]
    fn authenticated_request_from_url_with_percent_encoded_password() {
        let url = Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_username("user").unwrap();
        auth_url.set_password(Some("password==")).unwrap();
        let credentials = Credentials::from_url(&auth_url).unwrap();

        let mut request = Request::new(reqwest::Method::GET, url);
        request = credentials.authenticate(request);

        let mut header = request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .expect("Authorization header should be set")
            .clone();
        header.set_sensitive(false);

        assert_debug_snapshot!(header, @r###""Basic dXNlcjpwYXNzd29yZD09""###);
        assert_eq!(Credentials::from_header_value(&header), Some(credentials));
    }

    // Test that we don't include the password in debug messages.
    #[test]
    fn test_password_obfuscation() {
        let credentials =
            Credentials::basic(Some(String::from("user")), Some(String::from("password")));
        let debugged = format!("{credentials:?}");
        assert_eq!(
            debugged,
            "Basic { username: Username(Some(\"user\")), password: Some(****) }"
        );
    }
}

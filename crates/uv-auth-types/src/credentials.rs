use std::borrow::Cow;
use std::fmt;
use std::io::Read;
use std::io::Write;
use std::str::Utf8Error;

use base64::prelude::BASE64_STANDARD;
use base64::read::DecoderReader;
use base64::write::EncoderWriter;
use http::header::{HeaderValue, InvalidHeaderValue};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use uv_netrc::Netrc;
use uv_redacted::DisplaySafeUrl;
use uv_static::EnvVars;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Credentials {
    /// RFC 7617 HTTP Basic Authentication
    Basic {
        /// The username to use for authentication.
        username: Username,
        /// The password to use for authentication.
        password: Option<Password>,
    },
    /// RFC 6750 Bearer Token Authentication
    Bearer {
        /// The token to use for authentication.
        token: Token,
    },
}

#[derive(Debug, Error)]
pub enum CredentialsFromUrlError {
    #[error("URL username contains invalid UTF-8")]
    InvalidUsernameUtf8(#[source] Utf8Error),
    #[error("URL password contains invalid UTF-8")]
    InvalidPasswordUtf8(#[source] Utf8Error),
}

#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Username(Option<String>);

impl Username {
    /// Create a new username.
    ///
    /// Unlike `reqwest`, empty usernames are be encoded as `None` instead of an empty string.
    pub fn new(value: Option<String>) -> Self {
        // Ensure empty strings are `None`
        Self(value.filter(|s| !s.is_empty()))
    }

    pub fn none() -> Self {
        Self::new(None)
    }

    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }

    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }

    pub fn as_deref(&self) -> Option<&str> {
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

#[derive(Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Password(String);

impl Password {
    pub fn new(password: String) -> Self {
        Self(password)
    }

    /// Return the [`Password`] as a string slice.
    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "****")
    }
}

#[derive(Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Default, Deserialize)]
#[serde(transparent)]
pub struct Token(Vec<u8>);

impl Token {
    pub fn new(token: Vec<u8>) -> Self {
        Self(token)
    }

    /// Return the [`Token`] as a byte slice.
    fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    /// Convert the [`Token`] into its underlying [`Vec<u8>`].
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    /// Return whether the [`Token`] is empty.
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "****")
    }
}
impl Credentials {
    /// Create a set of HTTP Basic Authentication credentials.
    pub fn basic(username: Option<String>, password: Option<String>) -> Self {
        Self::Basic {
            username: Username::new(username),
            password: password.map(Password),
        }
    }

    /// Create a set of Bearer Authentication credentials.
    pub fn bearer(token: Vec<u8>) -> Self {
        Self::Bearer {
            token: Token::new(token),
        }
    }

    pub fn username(&self) -> Option<&str> {
        match self {
            Self::Basic { username, .. } => username.as_deref(),
            Self::Bearer { .. } => None,
        }
    }

    pub fn to_username(&self) -> Username {
        match self {
            Self::Basic { username, .. } => username.clone(),
            Self::Bearer { .. } => Username::none(),
        }
    }

    pub fn as_username(&self) -> Cow<'_, Username> {
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

    pub fn is_authenticated(&self) -> bool {
        match self {
            Self::Basic {
                username: _,
                password,
            } => password.is_some(),
            Self::Bearer { token } => !token.is_empty(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Basic { username, password } => username.is_none() && password.is_none(),
            Self::Bearer { token } => token.is_empty(),
        }
    }

    /// Return [`Credentials`] for a [`Url`] from a [`Netrc`] file, if any.
    ///
    /// If a username is provided, it must match the login in the netrc file or [`None`] is returned.
    pub fn from_netrc(netrc: &Netrc, url: &DisplaySafeUrl, username: Option<&str>) -> Option<Self> {
        let host = url.host_str()?;
        let entry = netrc
            .hosts
            .get(host)
            .or_else(|| netrc.hosts.get("default"))?;

        // Ensure the username matches if provided
        if username.is_some_and(|username| username != entry.login) {
            return None;
        }

        Some(Self::Basic {
            username: Username::new(Some(entry.login.clone())),
            password: Some(Password(entry.password.clone())),
        })
    }

    /// Parse [`Credentials`] from a URL, if any.
    ///
    /// Returns [`None`] if both [`Url::username`] and [`Url::password`] are not populated.
    pub fn from_url(url: &Url) -> Result<Option<Self>, CredentialsFromUrlError> {
        if url.username().is_empty() && url.password().is_none() {
            return Ok(None);
        }

        // Remove percent-encoding from URL credentials.
        // See <https://github.com/pypa/pip/blob/06d21db4ff1ab69665c22a88718a4ea9757ca293/src/pip/_internal/utils/misc.py#L497-L499>
        let username = if url.username().is_empty() {
            None
        } else {
            Some(
                percent_encoding::percent_decode_str(url.username())
                    .decode_utf8()
                    .map_err(CredentialsFromUrlError::InvalidUsernameUtf8)?
                    .into_owned(),
            )
        };
        let password = url
            .password()
            .map(|password| {
                percent_encoding::percent_decode_str(password)
                    .decode_utf8()
                    .map(|password| Password(password.into_owned()))
                    .map_err(CredentialsFromUrlError::InvalidPasswordUtf8)
            })
            .transpose()?;

        Ok(Some(Self::Basic {
            username: username.into(),
            password,
        }))
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

    /// Parse [`Credentials`] from an authorization header, if any.
    ///
    /// HTTP Basic and Bearer Authentication are both supported.
    /// [`None`] will be returned if another authorization scheme is detected.
    ///
    /// Panics if the authentication is not conformant to the HTTP Basic Authentication scheme:
    /// - The contents must be base64 encoded
    /// - There must be a `:` separator
    pub fn from_header_value(header: &HeaderValue) -> Option<Self> {
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
                token: Token::new(token.to_vec()),
            });
        }

        None
    }

    /// Create an HTTP authorization header for the credentials.
    ///
    /// Returns an error if the bearer token contains invalid header characters.
    pub fn to_header_value(&self) -> Result<HeaderValue, InvalidHeaderValue> {
        let header_bytes = match self {
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
                buf
            }
            Self::Bearer { token } => [b"Bearer ", token.as_slice()].concat(),
        };
        let mut header = HeaderValue::from_bytes(&header_bytes)?;
        header.set_sensitive(true);
        Ok(header)
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
}

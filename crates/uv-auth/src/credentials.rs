use base64::prelude::BASE64_STANDARD;
use base64::write::EncoderWriter;
use netrc::Authenticator;
use reqwest::header::HeaderValue;
use std::io::Write;
use url::Url;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Credentials {
    username: String,
    password: Option<String>,
}

impl Credentials {
    pub fn new(username: String, password: Option<String>) -> Self {
        Self { username, password }
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }

    /// Extract credentials from a URL.
    ///
    /// Returns `None` if `username` and `password` are not populated.
    pub fn from_url(url: &Url) -> Option<Self> {
        if url.username().is_empty() && url.password().is_none() {
            return None;
        }
        Some(Self {
            // Remove percent-encoding from URL credentials
            // See <https://github.com/pypa/pip/blob/06d21db4ff1ab69665c22a88718a4ea9757ca293/src/pip/_internal/utils/misc.py#L497-L499>
            username: urlencoding::decode(url.username())
                .expect("An encoded username should always decode")
                .into_owned(),
            password: url.password().map(|password| {
                urlencoding::decode(password)
                    .expect("An encoded password should always decode")
                    .into_owned()
            }),
        })
    }
}

impl From<Authenticator> for Credentials {
    fn from(auth: Authenticator) -> Self {
        Credentials {
            username: auth.login,
            password: Some(auth.password),
        }
    }
}

impl Credentials {
    /// Attach the credentials to the given request.
    ///
    /// Any existing credentials will be overridden.
    #[must_use]
    pub fn authenticated_request(&self, mut request: reqwest::Request) -> reqwest::Request {
        request.headers_mut().insert(
            reqwest::header::AUTHORIZATION,
            basic_auth(self.username(), self.password()),
        );
        request
    }
}

/// Create a `HeaderValue` for basic authentication.
///
/// Source: <https://github.com/seanmonstar/reqwest/blob/2c11ef000b151c2eebeed2c18a7b81042220c6b0/src/util.rs#L3>
fn basic_auth<U, P>(username: U, password: Option<P>) -> HeaderValue
where
    U: std::fmt::Display,
    P: std::fmt::Display,
{
    let mut buf = b"Basic ".to_vec();
    {
        let mut encoder = EncoderWriter::new(&mut buf, &BASE64_STANDARD);
        let _ = write!(encoder, "{}:", username);
        if let Some(password) = password {
            let _ = write!(encoder, "{}", password);
        }
    }
    let mut header = HeaderValue::from_bytes(&buf).expect("base64 is always valid HeaderValue");
    header.set_sensitive(true);
    header
}

#[cfg(test)]
mod test {
    use std::io::Read;

    use base64::read::DecoderReader;
    use insta::{assert_debug_snapshot, assert_snapshot};

    use super::*;

    fn decode_basic_auth(header: HeaderValue) -> String {
        let mut value = header.as_bytes();
        value = value
            .strip_prefix(b"Basic ")
            .expect("Basic authentication should start with 'Basic '");
        let mut decoder = DecoderReader::new(&mut value, &BASE64_STANDARD);
        let mut buf = "Basic: ".to_string();
        decoder
            .read_to_string(&mut buf)
            .expect("Header contents should be valid base64");
        buf
    }

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
        assert_eq!(credentials.username(), "user");
        assert_eq!(credentials.password(), Some("password"));
    }

    #[test]
    fn authenticated_request_from_url() {
        let url = Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_username("user").unwrap();
        auth_url.set_password(Some("password")).unwrap();
        let credentials = Credentials::from_url(&auth_url).unwrap();

        let mut request = reqwest::Request::new(reqwest::Method::GET, url);
        request = credentials.authenticated_request(request);

        let mut header = request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .expect("Authorization header should be set")
            .clone();
        header.set_sensitive(false);

        assert_debug_snapshot!(header, @r###""Basic dXNlcjpwYXNzd29yZA==""###);
        assert_snapshot!(decode_basic_auth(header), @"Basic: user:password");
    }

    #[test]
    fn authenticated_request_from_url_with_percent_encoded_user() {
        let url = Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_username("user@domain").unwrap();
        auth_url.set_password(Some("password")).unwrap();
        let credentials = Credentials::from_url(&auth_url).unwrap();

        let mut request = reqwest::Request::new(reqwest::Method::GET, url);
        request = credentials.authenticated_request(request);

        let mut header = request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .expect("Authorization header should be set")
            .clone();
        header.set_sensitive(false);

        assert_debug_snapshot!(header, @r###""Basic dXNlckBkb21haW46cGFzc3dvcmQ=""###);
        assert_snapshot!(decode_basic_auth(header), @"Basic: user@domain:password");
    }

    #[test]
    fn authenticated_request_from_url_with_percent_encoded_password() {
        let url = Url::parse("https://example.com/simple/first/").unwrap();
        let mut auth_url = url.clone();
        auth_url.set_username("user").unwrap();
        auth_url.set_password(Some("password==")).unwrap();
        let credentials = Credentials::from_url(&auth_url).unwrap();

        let mut request = reqwest::Request::new(reqwest::Method::GET, url);
        request = credentials.authenticated_request(request);

        let mut header = request
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .expect("Authorization header should be set")
            .clone();
        header.set_sensitive(false);

        assert_debug_snapshot!(header, @r###""Basic dXNlcjpwYXNzd29yZD09""###);
        assert_snapshot!(decode_basic_auth(header), @"Basic: user:password==");
    }
}

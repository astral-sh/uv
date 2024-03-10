use netrc::Netrc;
use reqwest::{header::HeaderValue, Request, Response};
use reqwest_middleware::{Middleware, Next};
use std::path::Path;
use task_local_extensions::Extensions;

use crate::{
    keyring::get_keyring_auth,
    store::{AuthenticationStore, Credential},
};

/// A middleware that adds basic authentication to requests based on the netrc file and the keyring.
///
/// Netrc support Based on: <https://github.com/gribouille/netrc>.
pub struct AuthMiddleware {
    nrc: Option<Netrc>,
    use_keyring: bool,
}

impl AuthMiddleware {
    pub fn new(use_keyring: bool) -> Self {
        AuthMiddleware {
            nrc: Netrc::new().ok(),
            use_keyring,
        }
    }

    pub fn from_netrc_file(file: &Path, use_keyring: bool) -> Self {
        Self {
            nrc: Netrc::from_file(file).ok(),
            use_keyring,
        }
    }
}

#[async_trait::async_trait]
impl Middleware for AuthMiddleware {
    async fn handle(
        &self,
        mut req: Request,
        _extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<Response> {
        let url = req.url().clone();
        // If the request already has an authorization header, we don't need to do anything.
        // This gives in-URL credentials precedence over the netrc file.
        if req.headers().contains_key(reqwest::header::AUTHORIZATION) {
            if !url.username().is_empty() {
                AuthenticationStore::save_from_url(&url);
            }
            return next.run(req, _extensions).await;
        }

        // Try auth strategies in order of precedence:
        if let Some(stored_auth) = AuthenticationStore::get(&url) {
            // If we've already seen this URL, we can use the stored credentials
            if let Some(auth) = stored_auth {
                match auth {
                    Credential::Basic(_) => {
                        req.headers_mut().insert(
                            reqwest::header::AUTHORIZATION,
                            basic_auth(auth.username(), auth.password()),
                        );
                    }
                    // Url must already have auth if before middleware runs - see `AuthenticationStore::with_url_encoded_auth`
                    Credential::UrlEncoded(_) => (),
                }
            }
        } else if let Some(auth) = self.nrc.as_ref().and_then(|nrc| {
            // If we find a matching entry in the netrc file, we can use it
            url.host_str()
                .and_then(|host| nrc.hosts.get(host).or_else(|| nrc.hosts.get("default")))
        }) {
            let auth = Credential::from(auth);
            req.headers_mut().insert(
                reqwest::header::AUTHORIZATION,
                basic_auth(auth.username(), auth.password()),
            );
            AuthenticationStore::set(&url, Some(auth));
        } else if self.use_keyring {
            // If we have keyring support enabled, we check there as well
            if let Ok(auth) = get_keyring_auth(&url) {
                // Keyring auth found - save it and return the request
                req.headers_mut().insert(
                    reqwest::header::AUTHORIZATION,
                    basic_auth(auth.username(), auth.password()),
                );
                AuthenticationStore::set(&url, Some(auth));
            }
        }

        // If we still don't have any credentials, we save the URL so we don't have to check netrc or keyring again
        if !req.headers().contains_key(reqwest::header::AUTHORIZATION) {
            AuthenticationStore::set(&url, None);
        }

        next.run(req, _extensions).await
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
    use base64::prelude::BASE64_STANDARD;
    use base64::write::EncoderWriter;
    use std::io::Write;

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
mod tests {
    use super::*;
    use reqwest::Client;
    use reqwest_middleware::ClientBuilder;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use wiremock::matchers::{basic_auth, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const NETRC: &str = r#"default login myuser password mypassword"#;

    #[tokio::test]
    async fn test_init() -> Result<(), std::io::Error> {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/hello"))
            .and(basic_auth("myuser", "mypassword"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let status = ClientBuilder::new(Client::builder().build().unwrap())
            .build()
            .get(format!("{}/hello", &server.uri()))
            .send()
            .await
            .unwrap()
            .status();

        assert_eq!(status, 404);

        let mut netrc_file = NamedTempFile::new()?;
        writeln!(netrc_file, "{}", NETRC)?;

        let status = ClientBuilder::new(Client::builder().build().unwrap())
            .with(AuthMiddleware::from_netrc_file(netrc_file.path(), false))
            .build()
            .get(format!("{}/hello", &server.uri()))
            .send()
            .await
            .unwrap()
            .status();

        assert_eq!(status, 200);
        Ok(())
    }
}

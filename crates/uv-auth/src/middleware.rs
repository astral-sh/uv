use std::path::Path;

use netrc::Netrc;
use reqwest::{header::HeaderValue, Request, Response};
use reqwest_middleware::{Middleware, Next};
use task_local_extensions::Extensions;
use tracing::{debug, warn};

use crate::{
    keyring::{get_keyring_subprocess_auth, KeyringProvider},
    store::Credential,
    GLOBAL_AUTH_STORE,
};

/// A middleware that adds basic authentication to requests based on the netrc file and the keyring.
///
/// Netrc support Based on: <https://github.com/gribouille/netrc>.
pub struct AuthMiddleware {
    nrc: Option<Netrc>,
    keyring_provider: KeyringProvider,
}

impl AuthMiddleware {
    pub fn new(keyring_provider: KeyringProvider) -> Self {
        Self {
            nrc: Netrc::new().ok(),
            keyring_provider,
        }
    }

    pub fn from_netrc_file(file: &Path, keyring_provider: KeyringProvider) -> Self {
        Self {
            nrc: Netrc::from_file(file).ok(),
            keyring_provider,
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
            debug!("Request already has an authorization header: {url}");
            return next.run(req, _extensions).await;
        }

        // Try auth strategies in order of precedence:
        if let Some(stored_auth) = GLOBAL_AUTH_STORE.get(&url) {
            // If we've already seen this URL, we can use the stored credentials
            if let Some(auth) = stored_auth {
                debug!("Adding authentication to already-seen URL: {url}");
                req.headers_mut().insert(
                    reqwest::header::AUTHORIZATION,
                    basic_auth(auth.username(), auth.password()),
                );
            } else {
                debug!("No credentials found for already-seen URL: {url}");
            }
        } else if let Some(auth) = self.nrc.as_ref().and_then(|nrc| {
            // If we find a matching entry in the netrc file, we can use it
            url.host_str()
                .and_then(|host| nrc.hosts.get(host).or_else(|| nrc.hosts.get("default")))
        }) {
            let auth = Credential::from(auth.to_owned());
            req.headers_mut().insert(
                reqwest::header::AUTHORIZATION,
                basic_auth(auth.username(), auth.password()),
            );
            GLOBAL_AUTH_STORE.set(&url, Some(auth));
        } else if matches!(self.keyring_provider, KeyringProvider::Subprocess) {
            // If we have keyring support enabled, we check there as well
            match get_keyring_subprocess_auth(&url) {
                Ok(Some(auth)) => {
                    req.headers_mut().insert(
                        reqwest::header::AUTHORIZATION,
                        basic_auth(auth.username(), auth.password()),
                    );
                    GLOBAL_AUTH_STORE.set(&url, Some(auth));
                }
                Ok(None) => {
                    debug!("No keyring credentials found for {url}");
                }
                Err(e) => {
                    warn!("Failed to get keyring credentials for {url}: {e}");
                }
            }
        }

        // If we still don't have any credentials, we save the URL so we don't have to check netrc or keyring again
        if !req.headers().contains_key(reqwest::header::AUTHORIZATION) {
            debug!("No credentials found for: {url}");
            GLOBAL_AUTH_STORE.set(&url, None);
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
    use std::io::Write;

    use reqwest::Client;
    use reqwest_middleware::ClientBuilder;
    use tempfile::NamedTempFile;
    use wiremock::matchers::{basic_auth, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const NETRC: &str = r#"default login myuser password mypassword"#;

    #[tokio::test]
    async fn test_init() -> Result<(), Box<dyn std::error::Error>> {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/hello"))
            .and(basic_auth("myuser", "mypassword"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let status = ClientBuilder::new(Client::builder().build()?)
            .build()
            .get(format!("{}/hello", &server.uri()))
            .send()
            .await?
            .status();

        assert_eq!(status, 404);

        let mut netrc_file = NamedTempFile::new()?;
        writeln!(netrc_file, "{}", NETRC)?;

        let status = ClientBuilder::new(Client::builder().build()?)
            .with(AuthMiddleware::from_netrc_file(
                netrc_file.path(),
                KeyringProvider::Disabled,
            ))
            .build()
            .get(format!("{}/hello", &server.uri()))
            .send()
            .await?
            .status();

        assert_eq!(status, 200);
        Ok(())
    }
}

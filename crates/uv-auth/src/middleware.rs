use http::Extensions;
use std::path::Path;

use netrc::Netrc;
use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next};
use tracing::{debug, warn};

use crate::{credentials::Credentials, keyring::KeyringProvider, GLOBAL_AUTH_STORE};

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
        mut request: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<Response> {
        let url = request.url().clone();

        // If the request already has an authorization header, we don't need to do anything.
        // This gives in-URL credentials precedence over the netrc file.
        if request
            .headers()
            .contains_key(reqwest::header::AUTHORIZATION)
        {
            debug!("Credentials are already present for {url}");
            return next.run(request, extensions).await;
        }

        if let Some(credentials) = GLOBAL_AUTH_STORE.get(&url) {
            debug!("Adding stored credentials to {url}");
            request = credentials.authenticated_request(request)
        } else if GLOBAL_AUTH_STORE.should_attempt_fetch(&url) {
            if let Some(credentials) = self.nrc.as_ref().and_then(|nrc| {
                // If we find a matching entry in the netrc file, we can use it
                url.host_str()
                    .and_then(|host| nrc.hosts.get(host).or_else(|| nrc.hosts.get("default")))
            }) {
                let credentials = Credentials::from(credentials.to_owned());
                request = credentials.authenticated_request(request);
                GLOBAL_AUTH_STORE.set(&url, credentials);
            } else {
                // If we have keyring support enabled, we check there as well
                match self.keyring_provider.fetch(&url) {
                    Ok(Some(credentials)) => {
                        request = credentials.authenticated_request(request);
                        GLOBAL_AUTH_STORE.set(&url, credentials);
                    }
                    Ok(None) => {
                        debug!("No keyring credentials found for {url}");
                    }
                    Err(e) => {
                        warn!("Failed to get keyring credentials for {url}: {e}");
                    }
                }
            }

            GLOBAL_AUTH_STORE.set_fetch_attempted(&url);
        }

        next.run(request, extensions).await
    }
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

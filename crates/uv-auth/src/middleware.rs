use http::Extensions;

use netrc::Netrc;
use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next};
use tracing::{debug, warn};

use crate::{
    credentials::Credentials, keyring::KeyringProvider, AuthenticationStore, GLOBAL_AUTH_STORE,
};

/// A middleware that adds basic authentication to requests based on the netrc file and the keyring.
///
/// Netrc support Based on: <https://github.com/gribouille/netrc>.
pub struct AuthMiddleware {
    netrc: Option<Netrc>,
    keyring: KeyringProvider,
    authentication_store: Option<AuthenticationStore>,
}

impl AuthMiddleware {
    pub fn new() -> Self {
        Self {
            netrc: Netrc::new().ok(),
            keyring: KeyringProvider::Disabled,
            authentication_store: None,
        }
    }

    /// Configure the [`Netrc`] credential file to use.
    ///
    /// `None` disables authentication via netrc.
    #[must_use]
    pub fn with_netrc(mut self, netrc: Option<Netrc>) -> Self {
        self.netrc = netrc;
        self
    }

    /// Configure the [`KeyringProvider`] to use.
    #[must_use]
    pub fn with_keyring(mut self, keyring: KeyringProvider) -> Self {
        self.keyring = keyring;
        self
    }

    /// Configure the [`AuthenticationStore`] to use.
    #[must_use]
    pub fn with_authentication_store(mut self, store: AuthenticationStore) -> Self {
        self.authentication_store = Some(store);
        self
    }

    /// Get the configured authentication store.
    ///
    /// If not set, the global store is used.
    fn authentication_store(&self) -> &AuthenticationStore {
        self.authentication_store
            .as_ref()
            .unwrap_or(&GLOBAL_AUTH_STORE)
    }
}

impl Default for AuthMiddleware {
    fn default() -> Self {
        AuthMiddleware::new()
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

        // Stash any authentication attached to this request for future requests
        // in the same realm.
        self.authentication_store()
            .set_default_from_request(&request);

        // If the request already has an authorization header, respect them.
        if request
            .headers()
            .contains_key(reqwest::header::AUTHORIZATION)
            || request.url().password().is_some()
        {
            debug!("Credentials are already present for {url}");
            return next.run(request, extensions).await;
        }

        if let Some(credentials) = self.authentication_store().get(&url) {
            debug!("Adding stored credentials to {url}");
            request = credentials.authenticated_request(request)
        } else if self.authentication_store().should_attempt_fetch(&url) {
            if let Some(credentials) = self.netrc.as_ref().and_then(|netrc| {
                debug!("Checking netrc for credentials for `{}`", url.to_string());
                Credentials::from_netrc(netrc, request.url())
            }) {
                debug!("Adding credentials from the netrc file to {url}");
                request = credentials.authenticated_request(request);
                self.authentication_store().set(&url, credentials);
            } else if !matches!(self.keyring, KeyringProvider::Disabled) {
                // If we have keyring support enabled, we check there as well
                match self.keyring.fetch(&url) {
                    Ok(Some(credentials)) => {
                        debug!("Adding credentials from the keyring to {url}");
                        request = credentials.authenticated_request(request);
                        self.authentication_store().set(&url, credentials);
                    }
                    Ok(None) => {
                        debug!("No keyring credentials found for {url}");
                    }
                    Err(e) => {
                        warn!("Failed to get keyring credentials for {url}: {e}");
                    }
                }
            } else {
                debug!("No authentication providers found.");
            }

            self.authentication_store().set_fetch_attempted(&url);
        }

        next.run(request, extensions).await
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use reqwest::Client;
    use tempfile::NamedTempFile;
    use test_log::test;

    use url::Url;
    use wiremock::matchers::{basic_auth, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    type Error = Box<dyn std::error::Error>;

    async fn start_test_server(username: &'static str, password: &'static str) -> MockServer {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(basic_auth(username, password))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        server
    }

    fn test_client_builder() -> reqwest_middleware::ClientBuilder {
        reqwest_middleware::ClientBuilder::new(
            Client::builder()
                .build()
                .expect("Reqwest client should build"),
        )
    }

    #[test(tokio::test)]
    async fn test_no_credentials() -> Result<(), Error> {
        let server = start_test_server("user", "password").await;
        let client = test_client_builder()
            .with(AuthMiddleware::new().with_authentication_store(AuthenticationStore::new()))
            .build();

        assert_eq!(
            client
                .get(format!("{}/foo", server.uri()))
                .send()
                .await?
                .status(),
            401
        );

        assert_eq!(
            client
                .get(format!("{}/bar", server.uri()))
                .send()
                .await?
                .status(),
            401
        );

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_credentials_prepopulated_from_url() -> Result<(), Error> {
        let username = "user";
        let password = "password";

        let server = start_test_server(username, password).await;

        let mut url = Url::parse(&server.uri())?;
        url.set_username(username).unwrap();
        url.set_password(Some(password)).unwrap();

        let store = AuthenticationStore::new();
        assert!(store.set_from_url(&url));

        let client = test_client_builder()
            .with(AuthMiddleware::new().with_authentication_store(store))
            .build();

        assert_eq!(
            client.get(server.uri()).send().await?.status(),
            200,
            "Requests should not require credentials"
        );
        assert_eq!(
            client
                .get(format!("{}/foo", server.uri()))
                .send()
                .await?
                .status(),
            200,
            "Requests to paths in the same realm should be authorized"
        );

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_credentials_in_url() -> Result<(), Error> {
        let username = "user";
        let password = "password";

        let server = start_test_server(username, password).await;

        let mut url = Url::parse(&server.uri())?;
        url.set_username(username).unwrap();
        url.set_password(Some(password)).unwrap();

        let client = test_client_builder()
            .with(AuthMiddleware::new().with_authentication_store(AuthenticationStore::new()))
            .build();

        assert_eq!(client.get(url).send().await?.status(), 200);

        // Works for a URL without credentials now
        assert_eq!(
            client.get(server.uri()).send().await?.status(),
            200,
            "Subsequent requests should not require credentials"
        );
        assert_eq!(
            client
                .get(format!("{}/foo", server.uri()))
                .send()
                .await?
                .status(),
            200,
            "Subsequent requests can be to different paths in the same realm"
        );

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_netrc_file_default_host() -> Result<(), Error> {
        let username = "user";
        let password = "password";

        let mut netrc_file = NamedTempFile::new()?;
        writeln!(netrc_file, "default login {username} password {password}")?;

        let server = start_test_server(username, password).await;
        let client = test_client_builder()
            .with(
                AuthMiddleware::new()
                    .with_authentication_store(AuthenticationStore::new())
                    .with_netrc(Netrc::from_file(netrc_file.path()).ok()),
            )
            .build();

        assert_eq!(
            client.get(server.uri()).send().await?.status(),
            200,
            "Credentials should be pulled from the netrc file"
        );

        let mut url = Url::parse(&server.uri())?;
        url.set_username(username).unwrap();
        url.set_password(Some("invalid")).unwrap();
        assert_eq!(
            client.get(url).send().await?.status(),
            401,
            "Credentials in the URL should take precedence and fail"
        );

        assert_eq!(
            client.get(server.uri()).send().await?.status(),
            200,
            "Subsequent requests should not use the invalid credentials"
        );

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_netrc_file_matching_host() -> Result<(), Error> {
        let username = "user";
        let password = "password";
        let server = start_test_server(username, password).await;
        let mut url = Url::parse(&server.uri())?;

        let mut netrc_file = NamedTempFile::new()?;
        writeln!(
            netrc_file,
            r#"machine {} login {username} password {password}"#,
            url.host_str().unwrap()
        )?;

        let client = test_client_builder()
            .with(
                AuthMiddleware::new()
                    .with_authentication_store(AuthenticationStore::new())
                    .with_netrc(Some(
                        Netrc::from_file(netrc_file.path()).expect("Test has valid netrc file"),
                    )),
            )
            .build();

        assert_eq!(
            client.get(server.uri()).send().await?.status(),
            200,
            "Credentials should be pulled from the netrc file"
        );

        url.set_username(username).unwrap();
        url.set_password(Some("invalid")).unwrap();
        assert_eq!(
            client.get(url).send().await?.status(),
            401,
            "Credentials in the URL should take precedence and fail"
        );

        assert_eq!(
            client.get(server.uri()).send().await?.status(),
            200,
            "Subsequent requests should not use the invalid credentials"
        );

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_netrc_file_mismatched_host() -> Result<(), Error> {
        let username = "user";
        let password = "password";
        let server = start_test_server(username, password).await;

        let mut netrc_file = NamedTempFile::new()?;
        writeln!(
            netrc_file,
            r#"machine example.com login {username} password {password}"#,
        )?;

        let client = test_client_builder()
            .with(
                AuthMiddleware::new()
                    .with_authentication_store(AuthenticationStore::new())
                    .with_netrc(Some(
                        Netrc::from_file(netrc_file.path()).expect("Test has valid netrc file"),
                    )),
            )
            .build();

        assert_eq!(
            client.get(server.uri()).send().await?.status(),
            401,
            "Credentials should not be pulled from the netrc file due to host mistmatch"
        );

        let mut url = Url::parse(&server.uri())?;
        url.set_username(username).unwrap();
        url.set_password(Some(password)).unwrap();
        assert_eq!(
            client.get(url).send().await?.status(),
            200,
            "Credentials in the URL should still work"
        );

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_netrc_file_mismatched_username() -> Result<(), Error> {
        let username = "user";
        let password = "password";
        let server = start_test_server(username, password).await;
        let url = Url::parse(&server.uri())?;

        let mut netrc_file = NamedTempFile::new()?;
        writeln!(
            netrc_file,
            r#"machine {} login {username} password {password}"#,
            url.host_str().unwrap()
        )?;

        let client = test_client_builder()
            .with(
                AuthMiddleware::new()
                    .with_authentication_store(AuthenticationStore::new())
                    .with_netrc(Some(
                        Netrc::from_file(netrc_file.path()).expect("Test has valid netrc file"),
                    )),
            )
            .build();

        let mut url = Url::parse(&server.uri())?;
        url.set_username("other-user").unwrap();
        assert_eq!(
            client.get(url).send().await?.status(),
            401,
            "The netrc password should not be used due to a username mismatch"
        );

        let mut url = Url::parse(&server.uri())?;
        url.set_username("other-user").unwrap();
        assert_eq!(
            client.get(url).send().await?.status(),
            // TODO(zanieb): This should be a 200
            // https://github.com/astral-sh/uv/issues/2563
            401,
            "The netrc password should be used"
        );

        Ok(())
    }
}

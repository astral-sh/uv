use std::sync::Arc;

use http::Extensions;

use netrc::Netrc;
use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next};
use tracing::{debug, trace};

use crate::{
    cache::CheckResponse, credentials::Credentials, CredentialsCache, KeyringProvider,
    CREDENTIALS_CACHE,
};

/// A middleware that adds basic authentication to requests based on the netrc file and the keyring.
///
/// Netrc support Based on: <https://github.com/gribouille/netrc>.
pub struct AuthMiddleware {
    netrc: Option<Netrc>,
    keyring: Option<KeyringProvider>,
    cache: Option<CredentialsCache>,
}

impl AuthMiddleware {
    pub fn new() -> Self {
        Self {
            netrc: Netrc::new().ok(),
            keyring: None,
            cache: None,
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
    pub fn with_keyring(mut self, keyring: Option<KeyringProvider>) -> Self {
        self.keyring = keyring;
        self
    }

    /// Configure the [`CredentialsCache`] to use.
    #[must_use]
    pub fn with_cache(mut self, cache: CredentialsCache) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Get the configured authentication store.
    ///
    /// If not set, the global store is used.
    fn cache(&self) -> &CredentialsCache {
        self.cache.as_ref().unwrap_or(&CREDENTIALS_CACHE)
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
        // Check for credentials attached to (1) the request itself
        let credentials = Credentials::from_request(&request);
        // In the middleware, existing credentials are already moved from the URL
        // to the headers so for display purposes we restore some information
        let url = if tracing::enabled!(tracing::Level::DEBUG) {
            let mut url = request.url().clone();
            if let Some(username) = credentials
                .as_ref()
                .and_then(|credentials| credentials.username())
            {
                let _ = url.set_username(username);
            };
            if credentials
                .as_ref()
                .and_then(|credentials| credentials.password())
                .is_some()
            {
                let _ = url.set_password(Some("****"));
            };
            url.to_string()
        } else {
            request.url().to_string()
        };
        trace!("Handling request for {url}");

        // Then check for credentials in (2) the cache
        let credentials = self.cache().check(request.url(), credentials);

        // Track credentials that we might want to insert into the cache
        let mut new_credentials = None;

        // If already authenticated (including a password), don't query other services
        if credentials.is_authenticated() {
            match credentials {
                // If we get credentials from the cache, update the request
                CheckResponse::Cached(credentials) => request = credentials.authenticate(request),
                // If we get credentials from the request, we should update the cache
                // but don't need to update the request
                CheckResponse::Uncached(credentials) => new_credentials = Some(credentials.clone()),
                CheckResponse::None => unreachable!("No credentials cannot be authenticated"),
            }
        // Otherwise, look for complete credentials in:
        // (3) The netrc file
        } else if let Some(credentials) = self.netrc.as_ref().and_then(|netrc| {
            trace!("Checking netrc for credentials for {url}");
            Credentials::from_netrc(
                netrc,
                request.url(),
                credentials
                    .get()
                    .and_then(|credentials| credentials.username()),
            )
        }) {
            debug!("Found credentials in netrc file for {url}");
            request = credentials.authenticate(request);
            new_credentials = Some(Arc::new(credentials));
        // (4) The keyring
        // N.B. The keyring provider performs lookups for the exact URL then
        //      falls back to the host, but we cache the result per host so if a keyring
        //      implementation returns different credentials for different URLs in the
        //      same realm we will use the wrong credentials.
        } else if let Some(credentials) = self.keyring.as_ref().and_then(|keyring| {
            if let Some(username) = credentials
                .get()
                .and_then(|credentials| credentials.username())
            {
                debug!("Checking keyring for credentials for {url}");
                keyring.fetch(request.url(), username)
            } else {
                trace!("Skipping keyring lookup for {url} with no username");
                None
            }
        }) {
            debug!("Found credentials in keyring for {url}");
            request = credentials.authenticate(request);
            new_credentials = Some(Arc::new(credentials));
        // No additional credentials were found
        } else {
            match credentials {
                CheckResponse::Cached(credentials) => request = credentials.authenticate(request),
                CheckResponse::Uncached(credentials) => new_credentials = Some(credentials.clone()),
                CheckResponse::None => {
                    debug!("No credentials found for {url}")
                }
            }
        }

        if let Some(credentials) = new_credentials {
            let url = request.url().clone();

            // Update the default credentials eagerly since requests are made concurrently
            // and we want to avoid expensive credential lookups
            self.cache().set_default(&url, credentials.clone());

            let result = next.run(request, extensions).await;

            // Only update the cache with new credentials on a successful request
            if result
                .as_ref()
                .is_ok_and(|response| response.error_for_status_ref().is_ok())
            {
                trace!("Updating cached credentials for {url}");
                self.cache().insert(&url, credentials)
            };
            result
        } else {
            next.run(request, extensions).await
        }
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
            .with(AuthMiddleware::new().with_cache(CredentialsCache::new()))
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
    async fn test_credentials_in_url() -> Result<(), Error> {
        let username = "user";
        let password = "password";

        let server = start_test_server(username, password).await;
        let client = test_client_builder()
            .with(AuthMiddleware::new().with_cache(CredentialsCache::new()))
            .build();

        let base_url = Url::parse(&server.uri())?;

        let mut url = base_url.clone();
        url.set_username(username).unwrap();
        url.set_password(Some(password)).unwrap();
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

        let mut url = base_url.clone();
        url.set_username(username).unwrap();
        url.set_password(Some("invalid")).unwrap();
        assert_eq!(
            client.get(url).send().await?.status(),
            401,
            "Credentials in the URL should take precedence and fail"
        );

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_credentials_in_url_username_only() -> Result<(), Error> {
        let username = "user";
        let password = "";

        let server = start_test_server(username, password).await;
        let client = test_client_builder()
            .with(AuthMiddleware::new().with_cache(CredentialsCache::new()))
            .build();

        let base_url = Url::parse(&server.uri())?;

        let mut url = base_url.clone();
        url.set_username(username).unwrap();
        url.set_password(None).unwrap();
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

        let mut url = base_url.clone();
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
    async fn test_netrc_file_default_host() -> Result<(), Error> {
        let username = "user";
        let password = "password";

        let mut netrc_file = NamedTempFile::new()?;
        writeln!(netrc_file, "default login {username} password {password}")?;

        let server = start_test_server(username, password).await;
        let client = test_client_builder()
            .with(
                AuthMiddleware::new()
                    .with_cache(CredentialsCache::new())
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
        let base_url = Url::parse(&server.uri())?;

        let mut netrc_file = NamedTempFile::new()?;
        writeln!(
            netrc_file,
            r#"machine {} login {username} password {password}"#,
            base_url.host_str().unwrap()
        )?;

        let client = test_client_builder()
            .with(
                AuthMiddleware::new()
                    .with_cache(CredentialsCache::new())
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

        let mut url = base_url.clone();
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
                    .with_cache(CredentialsCache::new())
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
        let base_url = Url::parse(&server.uri())?;

        let mut netrc_file = NamedTempFile::new()?;
        writeln!(
            netrc_file,
            r#"machine {} login {username} password {password}"#,
            base_url.host_str().unwrap()
        )?;

        let client = test_client_builder()
            .with(
                AuthMiddleware::new()
                    .with_cache(CredentialsCache::new())
                    .with_netrc(Some(
                        Netrc::from_file(netrc_file.path()).expect("Test has valid netrc file"),
                    )),
            )
            .build();

        let mut url = base_url.clone();
        url.set_username("other-user").unwrap();
        assert_eq!(
            client.get(url).send().await?.status(),
            401,
            "The netrc password should not be used due to a username mismatch"
        );

        let mut url = base_url.clone();
        url.set_username("user").unwrap();
        assert_eq!(
            client.get(url).send().await?.status(),
            200,
            "The netrc password should be used for a matching user"
        );

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_keyring() -> Result<(), Error> {
        let username = "user";
        let password = "password";
        let server = start_test_server(username, password).await;
        let base_url = Url::parse(&server.uri())?;

        let client = test_client_builder()
            .with(
                AuthMiddleware::new()
                    .with_cache(CredentialsCache::new())
                    .with_keyring(Some(KeyringProvider::dummy([(
                        (base_url.host_str().unwrap(), username),
                        password,
                    )]))),
            )
            .build();

        assert_eq!(
            client.get(server.uri()).send().await?.status(),
            401,
            "Credentials are not pulled from the keyring without a username"
        );

        let mut url = base_url.clone();
        url.set_username(username).unwrap();
        assert_eq!(
            client.get(url).send().await?.status(),
            200,
            "Credentials for the username should be pulled from the keyring"
        );

        let mut url = base_url.clone();
        url.set_username(username).unwrap();
        url.set_password(Some("invalid")).unwrap();
        assert_eq!(
            client.get(url).send().await?.status(),
            401,
            "Password in the URL should take precedence and fail"
        );

        let mut url = base_url.clone();
        url.set_username(username).unwrap();
        assert_eq!(
            client.get(url.clone()).send().await?.status(),
            200,
            "Subsequent requests should not use the invalid password"
        );

        let mut url = base_url.clone();
        url.set_username("other_user").unwrap();
        assert_eq!(
            client.get(url).send().await?.status(),
            401,
            "Credentials are not pulled from the keyring when given another username"
        );

        Ok(())
    }
}

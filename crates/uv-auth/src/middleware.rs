use netrc::Netrc;
use reqwest_middleware::{RequestBuilder, RequestInitialiser};
use std::path::Path;

use crate::{
    keyring::get_keyring_auth,
    store::{AuthenticationStore, Credential},
};

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

impl RequestInitialiser for AuthMiddleware {
    fn init(&self, req: RequestBuilder) -> RequestBuilder {
        match req.try_clone() {
            Some(nr) => req
                .try_clone()
                .unwrap() // Safe to unwrap because we just checked for Some
                .build()
                .ok()
                .and_then(|r| {
                    let url = r.url();
                    if let Some(auth) = AuthenticationStore::get(url) {
                        return auth.map(|auth| match auth {
                            Credential::Basic(_) => nr.basic_auth(auth.username(), auth.password()),
                            // Url must already have auth if before middleware runs - see `AuthenticationStore::with_url_encoded_auth`
                            Credential::UrlEncoded(_) => nr,
                        });
                    }
                    let nrc_auth = if let Some(nrc) = self.nrc.as_ref() {
                        url.host_str().and_then(|host| {
                            nrc.hosts.get(host).or_else(|| nrc.hosts.get("default"))
                        })
                    } else {
                        None
                    };
                    if let Some(auth) = nrc_auth {
                        // Netrc auth found - save it and return the request
                        let auth = Credential::from(auth);
                        let req = Some(nr.basic_auth(auth.username(), auth.password()));
                        AuthenticationStore::set(url, Some(auth));
                        return req;
                    };
                    if self.use_keyring {
                        if let Ok(auth) = get_keyring_auth(url) {
                            // Keyring auth found - save it and return the request
                            let req = Some(nr.basic_auth(auth.username(), auth.password()));
                            AuthenticationStore::set(url, Some(auth));
                            return req;
                        }
                    }
                    if !url.username().is_empty() {
                        AuthenticationStore::save_from_url(url);
                    } else {
                        AuthenticationStore::set(url, None);
                    }
                    None
                })
                .unwrap_or(req),
            None => req,
        }
    }
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
            .with_init(AuthMiddleware::from_netrc_file(netrc_file.path(), false))
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

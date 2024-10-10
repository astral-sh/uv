use std::io::Write;

use reqwest::Client;
use tempfile::NamedTempFile;
use test_log::test;

use url::Url;
use wiremock::matchers::{basic_auth, method, path_regex};
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

/// Without seeding the cache, authenticated requests are not cached
#[test(tokio::test)]
async fn test_credentials_in_url_no_seed() -> Result<(), Error> {
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
        "Requests can be to different paths in the same realm"
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
async fn test_credentials_in_url_seed() -> Result<(), Error> {
    let username = "user";
    let password = "password";

    let server = start_test_server(username, password).await;
    let base_url = Url::parse(&server.uri())?;
    let cache = CredentialsCache::new();
    cache.insert(
        &base_url,
        Arc::new(Credentials::new(
            Some(username.to_string()),
            Some(password.to_string()),
        )),
    );

    let client = test_client_builder()
        .with(AuthMiddleware::new().with_cache(cache))
        .build();

    let mut url = base_url.clone();
    url.set_username(username).unwrap();
    url.set_password(Some(password)).unwrap();
    assert_eq!(client.get(url).send().await?.status(), 200);

    // Works for a URL without credentials too
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
        "Requests can be to different paths in the same realm"
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
    let base_url = Url::parse(&server.uri())?;
    let cache = CredentialsCache::new();
    cache.insert(
        &base_url,
        Arc::new(Credentials::new(Some(username.to_string()), None)),
    );

    let client = test_client_builder()
        .with(AuthMiddleware::new().with_cache(cache))
        .build();

    let mut url = base_url.clone();
    url.set_username(username).unwrap();
    url.set_password(None).unwrap();
    assert_eq!(client.get(url).send().await?.status(), 200);

    // Works for a URL without credentials too
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
        "Requests can be to different paths in the same realm"
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
        "Credentials should not be pulled from the netrc file due to host mismatch"
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
                    (
                        format!(
                            "{}:{}",
                            base_url.host_str().unwrap(),
                            base_url.port().unwrap()
                        ),
                        username,
                    ),
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

/// We include ports in keyring requests, e.g., `localhost:8000` should be distinct from `localhost`,
/// unless the server is running on a default port, e.g., `localhost:80` is equivalent to `localhost`.
/// We don't unit test the latter case because it's possible to collide with a server a developer is
/// actually running.
#[test(tokio::test)]
async fn test_keyring_includes_non_standard_port() -> Result<(), Error> {
    let username = "user";
    let password = "password";
    let server = start_test_server(username, password).await;
    let base_url = Url::parse(&server.uri())?;

    let client = test_client_builder()
        .with(
            AuthMiddleware::new()
                .with_cache(CredentialsCache::new())
                .with_keyring(Some(KeyringProvider::dummy([(
                    // Omit the port from the keyring entry
                    (base_url.host_str().unwrap(), username),
                    password,
                )]))),
        )
        .build();

    let mut url = base_url.clone();
    url.set_username(username).unwrap();
    assert_eq!(
        client.get(url).send().await?.status(),
        401,
        "We should fail because the port is not present in the keyring entry"
    );

    Ok(())
}

#[test(tokio::test)]
async fn test_credentials_in_keyring_seed() -> Result<(), Error> {
    let username = "user";
    let password = "password";

    let server = start_test_server(username, password).await;
    let base_url = Url::parse(&server.uri())?;
    let cache = CredentialsCache::new();

    // Seed _just_ the username. This cache entry should be ignored and we should
    // still find a password via the keyring.
    cache.insert(
        &base_url,
        Arc::new(Credentials::new(Some(username.to_string()), None)),
    );
    let client =
        test_client_builder()
            .with(AuthMiddleware::new().with_cache(cache).with_keyring(Some(
                KeyringProvider::dummy([(
                    (
                        format!(
                            "{}:{}",
                            base_url.host_str().unwrap(),
                            base_url.port().unwrap()
                        ),
                        username,
                    ),
                    password,
                )]),
            )))
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

    Ok(())
}

#[test(tokio::test)]
async fn test_credentials_in_url_multiple_realms() -> Result<(), Error> {
    let username_1 = "user1";
    let password_1 = "password1";
    let server_1 = start_test_server(username_1, password_1).await;
    let base_url_1 = Url::parse(&server_1.uri())?;

    let username_2 = "user2";
    let password_2 = "password2";
    let server_2 = start_test_server(username_2, password_2).await;
    let base_url_2 = Url::parse(&server_2.uri())?;

    let cache = CredentialsCache::new();
    // Seed the cache with our credentials
    cache.insert(
        &base_url_1,
        Arc::new(Credentials::new(
            Some(username_1.to_string()),
            Some(password_1.to_string()),
        )),
    );
    cache.insert(
        &base_url_2,
        Arc::new(Credentials::new(
            Some(username_2.to_string()),
            Some(password_2.to_string()),
        )),
    );

    let client = test_client_builder()
        .with(AuthMiddleware::new().with_cache(cache))
        .build();

    // Both servers should work
    assert_eq!(
        client.get(server_1.uri()).send().await?.status(),
        200,
        "Requests should not require credentials"
    );
    assert_eq!(
        client.get(server_2.uri()).send().await?.status(),
        200,
        "Requests should not require credentials"
    );

    assert_eq!(
        client
            .get(format!("{}/foo", server_1.uri()))
            .send()
            .await?
            .status(),
        200,
        "Requests can be to different paths in the same realm"
    );
    assert_eq!(
        client
            .get(format!("{}/foo", server_2.uri()))
            .send()
            .await?
            .status(),
        200,
        "Requests can be to different paths in the same realm"
    );

    Ok(())
}

#[test(tokio::test)]
async fn test_credentials_from_keyring_multiple_realms() -> Result<(), Error> {
    let username_1 = "user1";
    let password_1 = "password1";
    let server_1 = start_test_server(username_1, password_1).await;
    let base_url_1 = Url::parse(&server_1.uri())?;

    let username_2 = "user2";
    let password_2 = "password2";
    let server_2 = start_test_server(username_2, password_2).await;
    let base_url_2 = Url::parse(&server_2.uri())?;

    let client = test_client_builder()
        .with(
            AuthMiddleware::new()
                .with_cache(CredentialsCache::new())
                .with_keyring(Some(KeyringProvider::dummy([
                    (
                        (
                            format!(
                                "{}:{}",
                                base_url_1.host_str().unwrap(),
                                base_url_1.port().unwrap()
                            ),
                            username_1,
                        ),
                        password_1,
                    ),
                    (
                        (
                            format!(
                                "{}:{}",
                                base_url_2.host_str().unwrap(),
                                base_url_2.port().unwrap()
                            ),
                            username_2,
                        ),
                        password_2,
                    ),
                ]))),
        )
        .build();

    // Both servers do not work without a username
    assert_eq!(
        client.get(server_1.uri()).send().await?.status(),
        401,
        "Requests should require a username"
    );
    assert_eq!(
        client.get(server_2.uri()).send().await?.status(),
        401,
        "Requests should require a username"
    );

    let mut url_1 = base_url_1.clone();
    url_1.set_username(username_1).unwrap();
    assert_eq!(
        client.get(url_1.clone()).send().await?.status(),
        200,
        "Requests with a username should succeed"
    );
    assert_eq!(
        client.get(server_2.uri()).send().await?.status(),
        401,
        "Credentials should not be re-used for the second server"
    );

    let mut url_2 = base_url_2.clone();
    url_2.set_username(username_2).unwrap();
    assert_eq!(
        client.get(url_2.clone()).send().await?.status(),
        200,
        "Requests with a username should succeed"
    );

    assert_eq!(
        client.get(format!("{url_1}/foo")).send().await?.status(),
        200,
        "Requests can be to different paths in the same realm"
    );
    assert_eq!(
        client.get(format!("{url_2}/foo")).send().await?.status(),
        200,
        "Requests can be to different paths in the same realm"
    );

    Ok(())
}

#[test(tokio::test)]
async fn test_credentials_in_url_mixed_authentication_in_realm() -> Result<(), Error> {
    let username_1 = "user1";
    let password_1 = "password1";
    let username_2 = "user2";
    let password_2 = "password2";

    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_regex("/prefix_1.*"))
        .and(basic_auth(username_1, password_1))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex("/prefix_2.*"))
        .and(basic_auth(username_2, password_2))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    // Create a third, public prefix
    // It will throw a 401 if it receives credentials
    Mock::given(method("GET"))
        .and(path_regex("/prefix_3.*"))
        .and(basic_auth(username_1, password_1))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/prefix_3.*"))
        .and(basic_auth(username_2, password_2))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/prefix_3.*"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let base_url = Url::parse(&server.uri())?;
    let base_url_1 = base_url.join("prefix_1")?;
    let base_url_2 = base_url.join("prefix_2")?;
    let base_url_3 = base_url.join("prefix_3")?;

    let cache = CredentialsCache::new();

    // Seed the cache with our credentials
    cache.insert(
        &base_url_1,
        Arc::new(Credentials::new(
            Some(username_1.to_string()),
            Some(password_1.to_string()),
        )),
    );
    cache.insert(
        &base_url_2,
        Arc::new(Credentials::new(
            Some(username_2.to_string()),
            Some(password_2.to_string()),
        )),
    );

    let client = test_client_builder()
        .with(AuthMiddleware::new().with_cache(cache))
        .build();

    // Both servers should work
    assert_eq!(
        client.get(base_url_1.clone()).send().await?.status(),
        200,
        "Requests should not require credentials"
    );
    assert_eq!(
        client.get(base_url_2.clone()).send().await?.status(),
        200,
        "Requests should not require credentials"
    );
    assert_eq!(
        client
            .get(base_url.join("prefix_1/foo")?)
            .send()
            .await?
            .status(),
        200,
        "Requests can be to different paths in the same realm"
    );
    assert_eq!(
        client
            .get(base_url.join("prefix_2/foo")?)
            .send()
            .await?
            .status(),
        200,
        "Requests can be to different paths in the same realm"
    );
    assert_eq!(
        client
            .get(base_url.join("prefix_1_foo")?)
            .send()
            .await?
            .status(),
        401,
        "Requests to paths with a matching prefix but different resource segments should fail"
    );

    assert_eq!(
        client.get(base_url_3.clone()).send().await?.status(),
        200,
        "Requests to the 'public' prefix should not use credentials"
    );

    Ok(())
}

#[test(tokio::test)]
async fn test_credentials_from_keyring_mixed_authentication_in_realm() -> Result<(), Error> {
    let username_1 = "user1";
    let password_1 = "password1";
    let username_2 = "user2";
    let password_2 = "password2";

    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_regex("/prefix_1.*"))
        .and(basic_auth(username_1, password_1))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex("/prefix_2.*"))
        .and(basic_auth(username_2, password_2))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    // Create a third, public prefix
    // It will throw a 401 if it receives credentials
    Mock::given(method("GET"))
        .and(path_regex("/prefix_3.*"))
        .and(basic_auth(username_1, password_1))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/prefix_3.*"))
        .and(basic_auth(username_2, password_2))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/prefix_3.*"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let base_url = Url::parse(&server.uri())?;
    let base_url_1 = base_url.join("prefix_1")?;
    let base_url_2 = base_url.join("prefix_2")?;
    let base_url_3 = base_url.join("prefix_3")?;

    let client = test_client_builder()
        .with(
            AuthMiddleware::new()
                .with_cache(CredentialsCache::new())
                .with_keyring(Some(KeyringProvider::dummy([
                    (
                        (
                            format!(
                                "{}:{}",
                                base_url_1.host_str().unwrap(),
                                base_url_1.port().unwrap()
                            ),
                            username_1,
                        ),
                        password_1,
                    ),
                    (
                        (
                            format!(
                                "{}:{}",
                                base_url_2.host_str().unwrap(),
                                base_url_2.port().unwrap()
                            ),
                            username_2,
                        ),
                        password_2,
                    ),
                ]))),
        )
        .build();

    // Both servers do not work without a username
    assert_eq!(
        client.get(base_url_1.clone()).send().await?.status(),
        401,
        "Requests should require a username"
    );
    assert_eq!(
        client.get(base_url_2.clone()).send().await?.status(),
        401,
        "Requests should require a username"
    );

    let mut url_1 = base_url_1.clone();
    url_1.set_username(username_1).unwrap();
    assert_eq!(
        client.get(url_1.clone()).send().await?.status(),
        200,
        "Requests with a username should succeed"
    );
    assert_eq!(
        client.get(base_url_2.clone()).send().await?.status(),
        401,
        "Credentials should not be re-used for the second prefix"
    );

    let mut url_2 = base_url_2.clone();
    url_2.set_username(username_2).unwrap();
    assert_eq!(
        client.get(url_2.clone()).send().await?.status(),
        200,
        "Requests with a username should succeed"
    );

    assert_eq!(
        client
            .get(base_url.join("prefix_1/foo")?)
            .send()
            .await?
            .status(),
        200,
        "Requests can be to different paths in the same prefix"
    );
    assert_eq!(
        client
            .get(base_url.join("prefix_2/foo")?)
            .send()
            .await?
            .status(),
        200,
        "Requests can be to different paths in the same prefix"
    );
    assert_eq!(
        client
            .get(base_url.join("prefix_1_foo")?)
            .send()
            .await?
            .status(),
        401,
        "Requests to paths with a matching prefix but different resource segments should fail"
    );
    assert_eq!(
        client.get(base_url_3.clone()).send().await?.status(),
        200,
        "Requests to the 'public' prefix should not use credentials"
    );

    Ok(())
}

/// Demonstrates "incorrect" behavior in our cache which avoids an expensive fetch of
/// credentials for _every_ request URL at the cost of inconsistent behavior when
/// credentials are not scoped to a realm.
#[test(tokio::test)]
async fn test_credentials_from_keyring_mixed_authentication_in_realm_same_username(
) -> Result<(), Error> {
    let username = "user";
    let password_1 = "password1";
    let password_2 = "password2";

    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_regex("/prefix_1.*"))
        .and(basic_auth(username, password_1))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex("/prefix_2.*"))
        .and(basic_auth(username, password_2))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let base_url = Url::parse(&server.uri())?;
    let base_url_1 = base_url.join("prefix_1")?;
    let base_url_2 = base_url.join("prefix_2")?;

    let client = test_client_builder()
        .with(
            AuthMiddleware::new()
                .with_cache(CredentialsCache::new())
                .with_keyring(Some(KeyringProvider::dummy([
                    ((base_url_1.clone(), username), password_1),
                    ((base_url_2.clone(), username), password_2),
                ]))),
        )
        .build();

    // Both servers do not work without a username
    assert_eq!(
        client.get(base_url_1.clone()).send().await?.status(),
        401,
        "Requests should require a username"
    );
    assert_eq!(
        client.get(base_url_2.clone()).send().await?.status(),
        401,
        "Requests should require a username"
    );

    let mut url_1 = base_url_1.clone();
    url_1.set_username(username).unwrap();
    assert_eq!(
        client.get(url_1.clone()).send().await?.status(),
        200,
        "The first request with a username will succeed"
    );
    assert_eq!(
        client.get(base_url_2.clone()).send().await?.status(),
        401,
        "Credentials should not be re-used for the second prefix"
    );
    assert_eq!(
        client
            .get(base_url.join("prefix_1/foo")?)
            .send()
            .await?
            .status(),
        200,
        "Subsequent requests can be to different paths in the same prefix"
    );

    let mut url_2 = base_url_2.clone();
    url_2.set_username(username).unwrap();
    assert_eq!(
             client.get(url_2.clone()).send().await?.status(),
             401, // INCORRECT BEHAVIOR
             "A request with the same username and realm for a URL that needs a different password will fail"
         );
    assert_eq!(
        client
            .get(base_url.join("prefix_2/foo")?)
            .send()
            .await?
            .status(),
        401, // INCORRECT BEHAVIOR
        "Requests to other paths in the failing prefix will also fail"
    );

    Ok(())
}

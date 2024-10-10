use super::*;
use futures::FutureExt;

#[tokio::test]
async fn fetch_url_no_host() {
    let url = Url::parse("file:/etc/bin/").unwrap();
    let keyring = KeyringProvider::empty();
    // Panics due to debug assertion; returns `None` in production
    let result = std::panic::AssertUnwindSafe(keyring.fetch(&url, "user"))
        .catch_unwind()
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn fetch_url_with_password() {
    let url = Url::parse("https://user:password@example.com").unwrap();
    let keyring = KeyringProvider::empty();
    // Panics due to debug assertion; returns `None` in production
    let result = std::panic::AssertUnwindSafe(keyring.fetch(&url, url.username()))
        .catch_unwind()
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn fetch_url_with_no_username() {
    let url = Url::parse("https://example.com").unwrap();
    let keyring = KeyringProvider::empty();
    // Panics due to debug assertion; returns `None` in production
    let result = std::panic::AssertUnwindSafe(keyring.fetch(&url, url.username()))
        .catch_unwind()
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn fetch_url_no_auth() {
    let url = Url::parse("https://example.com").unwrap();
    let keyring = KeyringProvider::empty();
    let credentials = keyring.fetch(&url, "user");
    assert!(credentials.await.is_none());
}

#[tokio::test]
async fn fetch_url() {
    let url = Url::parse("https://example.com").unwrap();
    let keyring = KeyringProvider::dummy([((url.host_str().unwrap(), "user"), "password")]);
    assert_eq!(
        keyring.fetch(&url, "user").await,
        Some(Credentials::new(
            Some("user".to_string()),
            Some("password".to_string())
        ))
    );
    assert_eq!(
        keyring.fetch(&url.join("test").unwrap(), "user").await,
        Some(Credentials::new(
            Some("user".to_string()),
            Some("password".to_string())
        ))
    );
}

#[tokio::test]
async fn fetch_url_no_match() {
    let url = Url::parse("https://example.com").unwrap();
    let keyring = KeyringProvider::dummy([(("other.com", "user"), "password")]);
    let credentials = keyring.fetch(&url, "user").await;
    assert_eq!(credentials, None);
}

#[tokio::test]
async fn fetch_url_prefers_url_to_host() {
    let url = Url::parse("https://example.com/").unwrap();
    let keyring = KeyringProvider::dummy([
        ((url.join("foo").unwrap().as_str(), "user"), "password"),
        ((url.host_str().unwrap(), "user"), "other-password"),
    ]);
    assert_eq!(
        keyring.fetch(&url.join("foo").unwrap(), "user").await,
        Some(Credentials::new(
            Some("user".to_string()),
            Some("password".to_string())
        ))
    );
    assert_eq!(
        keyring.fetch(&url, "user").await,
        Some(Credentials::new(
            Some("user".to_string()),
            Some("other-password".to_string())
        ))
    );
    assert_eq!(
        keyring.fetch(&url.join("bar").unwrap(), "user").await,
        Some(Credentials::new(
            Some("user".to_string()),
            Some("other-password".to_string())
        ))
    );
}

#[tokio::test]
async fn fetch_url_username() {
    let url = Url::parse("https://example.com").unwrap();
    let keyring = KeyringProvider::dummy([((url.host_str().unwrap(), "user"), "password")]);
    let credentials = keyring.fetch(&url, "user").await;
    assert_eq!(
        credentials,
        Some(Credentials::new(
            Some("user".to_string()),
            Some("password".to_string())
        ))
    );
}

#[tokio::test]
async fn fetch_url_username_no_match() {
    let url = Url::parse("https://example.com").unwrap();
    let keyring = KeyringProvider::dummy([((url.host_str().unwrap(), "foo"), "password")]);
    let credentials = keyring.fetch(&url, "bar").await;
    assert_eq!(credentials, None);

    // Still fails if we have `foo` in the URL itself
    let url = Url::parse("https://foo@example.com").unwrap();
    let credentials = keyring.fetch(&url, "bar").await;
    assert_eq!(credentials, None);
}

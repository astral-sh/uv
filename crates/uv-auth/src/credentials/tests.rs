use insta::assert_debug_snapshot;

use super::*;

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
    assert_eq!(credentials.username(), Some("user"));
    assert_eq!(credentials.password(), Some("password"));
}

#[test]
fn from_url_no_username() {
    let url = &Url::parse("https://example.com/simple/first/").unwrap();
    let mut auth_url = url.clone();
    auth_url.set_password(Some("password")).unwrap();
    let credentials = Credentials::from_url(&auth_url).unwrap();
    assert_eq!(credentials.username(), None);
    assert_eq!(credentials.password(), Some("password"));
}

#[test]
fn from_url_no_password() {
    let url = &Url::parse("https://example.com/simple/first/").unwrap();
    let mut auth_url = url.clone();
    auth_url.set_username("user").unwrap();
    let credentials = Credentials::from_url(&auth_url).unwrap();
    assert_eq!(credentials.username(), Some("user"));
    assert_eq!(credentials.password(), None);
}

#[test]
fn authenticated_request_from_url() {
    let url = Url::parse("https://example.com/simple/first/").unwrap();
    let mut auth_url = url.clone();
    auth_url.set_username("user").unwrap();
    auth_url.set_password(Some("password")).unwrap();
    let credentials = Credentials::from_url(&auth_url).unwrap();

    let mut request = reqwest::Request::new(reqwest::Method::GET, url);
    request = credentials.authenticate(request);

    let mut header = request
        .headers()
        .get(reqwest::header::AUTHORIZATION)
        .expect("Authorization header should be set")
        .clone();
    header.set_sensitive(false);

    assert_debug_snapshot!(header, @r###""Basic dXNlcjpwYXNzd29yZA==""###);
    assert_eq!(Credentials::from_header_value(&header), Some(credentials));
}

#[test]
fn authenticated_request_from_url_with_percent_encoded_user() {
    let url = Url::parse("https://example.com/simple/first/").unwrap();
    let mut auth_url = url.clone();
    auth_url.set_username("user@domain").unwrap();
    auth_url.set_password(Some("password")).unwrap();
    let credentials = Credentials::from_url(&auth_url).unwrap();

    let mut request = reqwest::Request::new(reqwest::Method::GET, url);
    request = credentials.authenticate(request);

    let mut header = request
        .headers()
        .get(reqwest::header::AUTHORIZATION)
        .expect("Authorization header should be set")
        .clone();
    header.set_sensitive(false);

    assert_debug_snapshot!(header, @r###""Basic dXNlckBkb21haW46cGFzc3dvcmQ=""###);
    assert_eq!(Credentials::from_header_value(&header), Some(credentials));
}

#[test]
fn authenticated_request_from_url_with_percent_encoded_password() {
    let url = Url::parse("https://example.com/simple/first/").unwrap();
    let mut auth_url = url.clone();
    auth_url.set_username("user").unwrap();
    auth_url.set_password(Some("password==")).unwrap();
    let credentials = Credentials::from_url(&auth_url).unwrap();

    let mut request = reqwest::Request::new(reqwest::Method::GET, url);
    request = credentials.authenticate(request);

    let mut header = request
        .headers()
        .get(reqwest::header::AUTHORIZATION)
        .expect("Authorization header should be set")
        .clone();
    header.set_sensitive(false);

    assert_debug_snapshot!(header, @r###""Basic dXNlcjpwYXNzd29yZD09""###);
    assert_eq!(Credentials::from_header_value(&header), Some(credentials));
}

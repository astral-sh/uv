use url::{ParseError, Url};

use crate::Realm;

#[test]
fn test_should_retain_auth() -> Result<(), ParseError> {
    // Exact match (https)
    assert_eq!(
        Realm::from(&Url::parse("https://example.com")?),
        Realm::from(&Url::parse("https://example.com")?)
    );

    // Exact match (with port)
    assert_eq!(
        Realm::from(&Url::parse("https://example.com:1234")?),
        Realm::from(&Url::parse("https://example.com:1234")?)
    );

    // Exact match (http)
    assert_eq!(
        Realm::from(&Url::parse("http://example.com")?),
        Realm::from(&Url::parse("http://example.com")?)
    );

    // Okay, path differs
    assert_eq!(
        Realm::from(&Url::parse("http://example.com/foo")?),
        Realm::from(&Url::parse("http://example.com/bar")?)
    );

    // Okay, default port differs (https)
    assert_eq!(
        Realm::from(&Url::parse("https://example.com:443")?),
        Realm::from(&Url::parse("https://example.com")?)
    );

    // Okay, default port differs (http)
    assert_eq!(
        Realm::from(&Url::parse("http://example.com:80")?),
        Realm::from(&Url::parse("http://example.com")?)
    );

    // Mismatched scheme
    assert_ne!(
        Realm::from(&Url::parse("https://example.com")?),
        Realm::from(&Url::parse("http://example.com")?)
    );

    // Mismatched scheme, we explicitly do not allow upgrade to https
    assert_ne!(
        Realm::from(&Url::parse("http://example.com")?),
        Realm::from(&Url::parse("https://example.com")?)
    );

    // Mismatched host
    assert_ne!(
        Realm::from(&Url::parse("https://foo.com")?),
        Realm::from(&Url::parse("https://bar.com")?)
    );

    // Mismatched port
    assert_ne!(
        Realm::from(&Url::parse("https://example.com:1234")?),
        Realm::from(&Url::parse("https://example.com:5678")?)
    );

    // Mismatched port, with one as default for scheme
    assert_ne!(
        Realm::from(&Url::parse("https://example.com:443")?),
        Realm::from(&Url::parse("https://example.com:5678")?)
    );
    assert_ne!(
        Realm::from(&Url::parse("https://example.com:1234")?),
        Realm::from(&Url::parse("https://example.com:443")?)
    );

    // Mismatched port, with default for a different scheme
    assert_ne!(
        Realm::from(&Url::parse("https://example.com:80")?),
        Realm::from(&Url::parse("https://example.com")?)
    );

    Ok(())
}

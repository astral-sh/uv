#[test]
fn parse() {
    assert_eq!(
        "*".parse::<super::TrustedHost>().unwrap(),
        super::TrustedHost::Wildcard
    );

    assert_eq!(
        "example.com".parse::<super::TrustedHost>().unwrap(),
        super::TrustedHost::Host {
            scheme: None,
            host: "example.com".to_string(),
            port: None
        }
    );

    assert_eq!(
        "example.com:8080".parse::<super::TrustedHost>().unwrap(),
        super::TrustedHost::Host {
            scheme: None,
            host: "example.com".to_string(),
            port: Some(8080)
        }
    );

    assert_eq!(
        "https://example.com".parse::<super::TrustedHost>().unwrap(),
        super::TrustedHost::Host {
            scheme: Some("https".to_string()),
            host: "example.com".to_string(),
            port: None
        }
    );

    assert_eq!(
        "https://example.com/hello/world"
            .parse::<super::TrustedHost>()
            .unwrap(),
        super::TrustedHost::Host {
            scheme: Some("https".to_string()),
            host: "example.com".to_string(),
            port: None
        }
    );
}

#[test]
fn parse() {
    assert_eq!(
        "example.com".parse::<super::TrustedHost>().unwrap(),
        super::TrustedHost {
            scheme: None,
            host: "example.com".to_string(),
            port: None
        }
    );

    assert_eq!(
        "example.com:8080".parse::<super::TrustedHost>().unwrap(),
        super::TrustedHost {
            scheme: None,
            host: "example.com".to_string(),
            port: Some(8080)
        }
    );

    assert_eq!(
        "https://example.com".parse::<super::TrustedHost>().unwrap(),
        super::TrustedHost {
            scheme: Some("https".to_string()),
            host: "example.com".to_string(),
            port: None
        }
    );

    assert_eq!(
        "https://example.com/hello/world"
            .parse::<super::TrustedHost>()
            .unwrap(),
        super::TrustedHost {
            scheme: Some("https".to_string()),
            host: "example.com".to_string(),
            port: None
        }
    );
}

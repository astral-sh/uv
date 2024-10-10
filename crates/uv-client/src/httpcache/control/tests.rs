use super::*;

#[test]
fn cache_control_token() {
    let cc: CacheControl = CacheControlParser::new(["no-cache"]).collect();
    assert!(cc.no_cache);
    assert!(!cc.must_revalidate);
}

#[test]
fn cache_control_max_age() {
    let cc: CacheControl = CacheControlParser::new(["max-age=60"]).collect();
    assert_eq!(Some(60), cc.max_age_seconds);
    assert!(!cc.must_revalidate);
}

// [RFC 9111 S5.2.1.1] says that client MUST NOT quote max-age, but we
// support parsing it that way anyway.
//
// [RFC 9111 S5.2.1.1]: https://www.rfc-editor.org/rfc/rfc9111.html#section-5.2.1.1
#[test]
fn cache_control_max_age_quoted() {
    let cc: CacheControl = CacheControlParser::new([r#"max-age="60""#]).collect();
    assert_eq!(Some(60), cc.max_age_seconds);
    assert!(!cc.must_revalidate);
}

#[test]
fn cache_control_max_age_invalid() {
    let cc: CacheControl = CacheControlParser::new(["max-age=6a0"]).collect();
    assert_eq!(None, cc.max_age_seconds);
    assert!(cc.must_revalidate);
}

#[test]
fn cache_control_immutable() {
    let cc: CacheControl = CacheControlParser::new(["max-age=31536000, immutable"]).collect();
    assert_eq!(Some(31_536_000), cc.max_age_seconds);
    assert!(cc.immutable);
    assert!(!cc.must_revalidate);
}

#[test]
fn cache_control_unrecognized() {
    let cc: CacheControl = CacheControlParser::new(["lion,max-age=60,zebra"]).collect();
    assert_eq!(Some(60), cc.max_age_seconds);
}

#[test]
fn cache_control_invalid_squashes_remainder() {
    let cc: CacheControl = CacheControlParser::new(["no-cache,\x00,max-age=60"]).collect();
    // The invalid data doesn't impact things before it.
    assert!(cc.no_cache);
    // The invalid data precludes parsing anything after.
    assert_eq!(None, cc.max_age_seconds);
    // The invalid contents should force revalidation.
    assert!(cc.must_revalidate);
}

#[test]
fn cache_control_invalid_squashes_remainder_but_not_other_header_values() {
    let cc: CacheControl =
        CacheControlParser::new(["no-cache,\x00,max-age=60", "max-stale=30"]).collect();
    // The invalid data doesn't impact things before it.
    assert!(cc.no_cache);
    // The invalid data precludes parsing anything after
    // in the same header value, but not in other
    // header values.
    assert_eq!(Some(30), cc.max_stale_seconds);
    // The invalid contents should force revalidation.
    assert!(cc.must_revalidate);
}

#[test]
fn cache_control_parse_token() {
    let directives = CacheControlParser::new(["no-cache"]).collect::<Vec<_>>();
    assert_eq!(
        directives,
        vec![CacheControlDirective {
            name: "no-cache".to_string(),
            value: vec![]
        }]
    );
}

#[test]
fn cache_control_parse_token_to_token_value() {
    let directives = CacheControlParser::new(["max-age=60"]).collect::<Vec<_>>();
    assert_eq!(
        directives,
        vec![CacheControlDirective {
            name: "max-age".to_string(),
            value: b"60".to_vec(),
        }]
    );
}

#[test]
fn cache_control_parse_token_to_quoted_string() {
    let directives =
        CacheControlParser::new([r#"private="cookie,x-something-else""#]).collect::<Vec<_>>();
    assert_eq!(
        directives,
        vec![CacheControlDirective {
            name: "private".to_string(),
            value: b"cookie,x-something-else".to_vec(),
        }]
    );
}

#[test]
fn cache_control_parse_token_to_quoted_string_with_escape() {
    let directives = CacheControlParser::new([r#"private="something\"crazy""#]).collect::<Vec<_>>();
    assert_eq!(
        directives,
        vec![CacheControlDirective {
            name: "private".to_string(),
            value: br#"something"crazy"#.to_vec(),
        }]
    );
}

#[test]
fn cache_control_parse_multiple_directives() {
    let header = r#"max-age=60, no-cache, private="cookie", no-transform"#;
    let directives = CacheControlParser::new([header]).collect::<Vec<_>>();
    assert_eq!(
        directives,
        vec![
            CacheControlDirective {
                name: "max-age".to_string(),
                value: b"60".to_vec(),
            },
            CacheControlDirective {
                name: "no-cache".to_string(),
                value: vec![]
            },
            CacheControlDirective {
                name: "private".to_string(),
                value: b"cookie".to_vec(),
            },
            CacheControlDirective {
                name: "no-transform".to_string(),
                value: vec![]
            },
        ]
    );
}

#[test]
fn cache_control_parse_multiple_directives_across_multiple_header_values() {
    let headers = [
        r"max-age=60, no-cache",
        r#"private="cookie""#,
        r"no-transform",
    ];
    let directives = CacheControlParser::new(headers).collect::<Vec<_>>();
    assert_eq!(
        directives,
        vec![
            CacheControlDirective {
                name: "max-age".to_string(),
                value: b"60".to_vec(),
            },
            CacheControlDirective {
                name: "no-cache".to_string(),
                value: vec![]
            },
            CacheControlDirective {
                name: "private".to_string(),
                value: b"cookie".to_vec(),
            },
            CacheControlDirective {
                name: "no-transform".to_string(),
                value: vec![]
            },
        ]
    );
}

#[test]
fn cache_control_parse_one_header_invalid() {
    let headers = [
        r"max-age=60, no-cache",
        r#", private="cookie""#,
        r"no-transform",
    ];
    let directives = CacheControlParser::new(headers).collect::<Vec<_>>();
    assert_eq!(
        directives,
        vec![
            CacheControlDirective {
                name: "max-age".to_string(),
                value: b"60".to_vec(),
            },
            CacheControlDirective {
                name: "no-cache".to_string(),
                value: vec![]
            },
            CacheControlDirective {
                name: "must-revalidate".to_string(),
                value: vec![]
            },
            CacheControlDirective {
                name: "no-transform".to_string(),
                value: vec![]
            },
        ]
    );
}

#[test]
fn cache_control_parse_invalid_directive_drops_remainder() {
    let header = r#"max-age=60, no-cache, ="cookie", no-transform"#;
    let directives = CacheControlParser::new([header]).collect::<Vec<_>>();
    assert_eq!(
        directives,
        vec![
            CacheControlDirective {
                name: "max-age".to_string(),
                value: b"60".to_vec(),
            },
            CacheControlDirective {
                name: "no-cache".to_string(),
                value: vec![]
            },
            CacheControlDirective {
                name: "must-revalidate".to_string(),
                value: vec![]
            },
        ]
    );
}

#[test]
fn cache_control_parse_name_normalized() {
    let header = r"MAX-AGE=60";
    let directives = CacheControlParser::new([header]).collect::<Vec<_>>();
    assert_eq!(
        directives,
        vec![CacheControlDirective {
            name: "max-age".to_string(),
            value: b"60".to_vec(),
        },]
    );
}

// When a duplicate directive is found, we keep the first one
// and add in a `must-revalidate` directive to indicate that
// things are stale and the client should do a re-check.
#[test]
fn cache_control_parse_duplicate_directives() {
    let header = r"max-age=60, no-cache, max-age=30";
    let directives = CacheControlParser::new([header]).collect::<Vec<_>>();
    assert_eq!(
        directives,
        vec![
            CacheControlDirective {
                name: "max-age".to_string(),
                value: b"60".to_vec(),
            },
            CacheControlDirective {
                name: "no-cache".to_string(),
                value: vec![]
            },
            CacheControlDirective {
                name: "must-revalidate".to_string(),
                value: vec![]
            },
        ]
    );
}

#[test]
fn cache_control_parse_duplicate_directives_across_headers() {
    let headers = [r"max-age=60, no-cache", r"max-age=30"];
    let directives = CacheControlParser::new(headers).collect::<Vec<_>>();
    assert_eq!(
        directives,
        vec![
            CacheControlDirective {
                name: "max-age".to_string(),
                value: b"60".to_vec(),
            },
            CacheControlDirective {
                name: "no-cache".to_string(),
                value: vec![]
            },
            CacheControlDirective {
                name: "must-revalidate".to_string(),
                value: vec![]
            },
        ]
    );
}

// Tests that we don't emit must-revalidate multiple times
// even when something is duplicated multiple times.
#[test]
fn cache_control_parse_duplicate_redux() {
    let header = r"max-age=60, no-cache, no-cache, max-age=30";
    let directives = CacheControlParser::new([header]).collect::<Vec<_>>();
    assert_eq!(
        directives,
        vec![
            CacheControlDirective {
                name: "max-age".to_string(),
                value: b"60".to_vec(),
            },
            CacheControlDirective {
                name: "no-cache".to_string(),
                value: vec![]
            },
            CacheControlDirective {
                name: "must-revalidate".to_string(),
                value: vec![]
            },
        ]
    );
}

use std::str::FromStr;

use crate::VersionSpecifier;

use super::*;

/// <https://github.com/pypa/packaging/blob/237ff3aa348486cf835a980592af3a59fccd6101/tests/test_version.py#L24-L81>
#[test]
fn test_packaging_versions() {
    let versions = [
        // Implicit epoch of 0
        ("1.0.dev456", Version::new([1, 0]).with_dev(Some(456))),
        (
            "1.0a1",
            Version::new([1, 0]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 1,
            })),
        ),
        (
            "1.0a2.dev456",
            Version::new([1, 0])
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Alpha,
                    number: 2,
                }))
                .with_dev(Some(456)),
        ),
        (
            "1.0a12.dev456",
            Version::new([1, 0])
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Alpha,
                    number: 12,
                }))
                .with_dev(Some(456)),
        ),
        (
            "1.0a12",
            Version::new([1, 0]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 12,
            })),
        ),
        (
            "1.0b1.dev456",
            Version::new([1, 0])
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Beta,
                    number: 1,
                }))
                .with_dev(Some(456)),
        ),
        (
            "1.0b2",
            Version::new([1, 0]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Beta,
                number: 2,
            })),
        ),
        (
            "1.0b2.post345.dev456",
            Version::new([1, 0])
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Beta,
                    number: 2,
                }))
                .with_dev(Some(456))
                .with_post(Some(345)),
        ),
        (
            "1.0b2.post345",
            Version::new([1, 0])
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Beta,
                    number: 2,
                }))
                .with_post(Some(345)),
        ),
        (
            "1.0b2-346",
            Version::new([1, 0])
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Beta,
                    number: 2,
                }))
                .with_post(Some(346)),
        ),
        (
            "1.0c1.dev456",
            Version::new([1, 0])
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Rc,
                    number: 1,
                }))
                .with_dev(Some(456)),
        ),
        (
            "1.0c1",
            Version::new([1, 0]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Rc,
                number: 1,
            })),
        ),
        (
            "1.0rc2",
            Version::new([1, 0]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Rc,
                number: 2,
            })),
        ),
        (
            "1.0c3",
            Version::new([1, 0]).with_pre(Some(Prerelease {
                kind: PrereleaseKind::Rc,
                number: 3,
            })),
        ),
        ("1.0", Version::new([1, 0])),
        (
            "1.0.post456.dev34",
            Version::new([1, 0]).with_post(Some(456)).with_dev(Some(34)),
        ),
        ("1.0.post456", Version::new([1, 0]).with_post(Some(456))),
        ("1.1.dev1", Version::new([1, 1]).with_dev(Some(1))),
        (
            "1.2+123abc",
            Version::new([1, 2]).with_local(vec![LocalSegment::String("123abc".to_string())]),
        ),
        (
            "1.2+123abc456",
            Version::new([1, 2]).with_local(vec![LocalSegment::String("123abc456".to_string())]),
        ),
        (
            "1.2+abc",
            Version::new([1, 2]).with_local(vec![LocalSegment::String("abc".to_string())]),
        ),
        (
            "1.2+abc123",
            Version::new([1, 2]).with_local(vec![LocalSegment::String("abc123".to_string())]),
        ),
        (
            "1.2+abc123def",
            Version::new([1, 2]).with_local(vec![LocalSegment::String("abc123def".to_string())]),
        ),
        (
            "1.2+1234.abc",
            Version::new([1, 2]).with_local(vec![
                LocalSegment::Number(1234),
                LocalSegment::String("abc".to_string()),
            ]),
        ),
        (
            "1.2+123456",
            Version::new([1, 2]).with_local(vec![LocalSegment::Number(123_456)]),
        ),
        (
            "1.2.r32+123456",
            Version::new([1, 2])
                .with_post(Some(32))
                .with_local(vec![LocalSegment::Number(123_456)]),
        ),
        (
            "1.2.rev33+123456",
            Version::new([1, 2])
                .with_post(Some(33))
                .with_local(vec![LocalSegment::Number(123_456)]),
        ),
        // Explicit epoch of 1
        (
            "1!1.0.dev456",
            Version::new([1, 0]).with_epoch(1).with_dev(Some(456)),
        ),
        (
            "1!1.0a1",
            Version::new([1, 0])
                .with_epoch(1)
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Alpha,
                    number: 1,
                })),
        ),
        (
            "1!1.0a2.dev456",
            Version::new([1, 0])
                .with_epoch(1)
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Alpha,
                    number: 2,
                }))
                .with_dev(Some(456)),
        ),
        (
            "1!1.0a12.dev456",
            Version::new([1, 0])
                .with_epoch(1)
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Alpha,
                    number: 12,
                }))
                .with_dev(Some(456)),
        ),
        (
            "1!1.0a12",
            Version::new([1, 0])
                .with_epoch(1)
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Alpha,
                    number: 12,
                })),
        ),
        (
            "1!1.0b1.dev456",
            Version::new([1, 0])
                .with_epoch(1)
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Beta,
                    number: 1,
                }))
                .with_dev(Some(456)),
        ),
        (
            "1!1.0b2",
            Version::new([1, 0])
                .with_epoch(1)
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Beta,
                    number: 2,
                })),
        ),
        (
            "1!1.0b2.post345.dev456",
            Version::new([1, 0])
                .with_epoch(1)
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Beta,
                    number: 2,
                }))
                .with_post(Some(345))
                .with_dev(Some(456)),
        ),
        (
            "1!1.0b2.post345",
            Version::new([1, 0])
                .with_epoch(1)
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Beta,
                    number: 2,
                }))
                .with_post(Some(345)),
        ),
        (
            "1!1.0b2-346",
            Version::new([1, 0])
                .with_epoch(1)
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Beta,
                    number: 2,
                }))
                .with_post(Some(346)),
        ),
        (
            "1!1.0c1.dev456",
            Version::new([1, 0])
                .with_epoch(1)
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Rc,
                    number: 1,
                }))
                .with_dev(Some(456)),
        ),
        (
            "1!1.0c1",
            Version::new([1, 0])
                .with_epoch(1)
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Rc,
                    number: 1,
                })),
        ),
        (
            "1!1.0rc2",
            Version::new([1, 0])
                .with_epoch(1)
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Rc,
                    number: 2,
                })),
        ),
        (
            "1!1.0c3",
            Version::new([1, 0])
                .with_epoch(1)
                .with_pre(Some(Prerelease {
                    kind: PrereleaseKind::Rc,
                    number: 3,
                })),
        ),
        ("1!1.0", Version::new([1, 0]).with_epoch(1)),
        (
            "1!1.0.post456.dev34",
            Version::new([1, 0])
                .with_epoch(1)
                .with_post(Some(456))
                .with_dev(Some(34)),
        ),
        (
            "1!1.0.post456",
            Version::new([1, 0]).with_epoch(1).with_post(Some(456)),
        ),
        (
            "1!1.1.dev1",
            Version::new([1, 1]).with_epoch(1).with_dev(Some(1)),
        ),
        (
            "1!1.2+123abc",
            Version::new([1, 2])
                .with_epoch(1)
                .with_local(vec![LocalSegment::String("123abc".to_string())]),
        ),
        (
            "1!1.2+123abc456",
            Version::new([1, 2])
                .with_epoch(1)
                .with_local(vec![LocalSegment::String("123abc456".to_string())]),
        ),
        (
            "1!1.2+abc",
            Version::new([1, 2])
                .with_epoch(1)
                .with_local(vec![LocalSegment::String("abc".to_string())]),
        ),
        (
            "1!1.2+abc123",
            Version::new([1, 2])
                .with_epoch(1)
                .with_local(vec![LocalSegment::String("abc123".to_string())]),
        ),
        (
            "1!1.2+abc123def",
            Version::new([1, 2])
                .with_epoch(1)
                .with_local(vec![LocalSegment::String("abc123def".to_string())]),
        ),
        (
            "1!1.2+1234.abc",
            Version::new([1, 2]).with_epoch(1).with_local(vec![
                LocalSegment::Number(1234),
                LocalSegment::String("abc".to_string()),
            ]),
        ),
        (
            "1!1.2+123456",
            Version::new([1, 2])
                .with_epoch(1)
                .with_local(vec![LocalSegment::Number(123_456)]),
        ),
        (
            "1!1.2.r32+123456",
            Version::new([1, 2])
                .with_epoch(1)
                .with_post(Some(32))
                .with_local(vec![LocalSegment::Number(123_456)]),
        ),
        (
            "1!1.2.rev33+123456",
            Version::new([1, 2])
                .with_epoch(1)
                .with_post(Some(33))
                .with_local(vec![LocalSegment::Number(123_456)]),
        ),
        (
            "98765!1.2.rev33+123456",
            Version::new([1, 2])
                .with_epoch(98765)
                .with_post(Some(33))
                .with_local(vec![LocalSegment::Number(123_456)]),
        ),
    ];
    for (string, structured) in versions {
        match Version::from_str(string) {
            Err(err) => {
                unreachable!(
                    "expected {string:?} to parse as {structured:?}, but got {err:?}",
                    structured = structured.as_bloated_debug(),
                )
            }
            Ok(v) => assert!(
                v == structured,
                "for {string:?}, expected {structured:?} but got {v:?}",
                structured = structured.as_bloated_debug(),
                v = v.as_bloated_debug(),
            ),
        }
        let spec = format!("=={string}");
        match VersionSpecifier::from_str(&spec) {
            Err(err) => {
                unreachable!(
                    "expected version in {spec:?} to parse as {structured:?}, but got {err:?}",
                    structured = structured.as_bloated_debug(),
                )
            }
            Ok(v) => assert!(
                v.version() == &structured,
                "for {string:?}, expected {structured:?} but got {v:?}",
                structured = structured.as_bloated_debug(),
                v = v.version.as_bloated_debug(),
            ),
        }
    }
}

/// <https://github.com/pypa/packaging/blob/237ff3aa348486cf835a980592af3a59fccd6101/tests/test_version.py#L91-L100>
#[test]
fn test_packaging_failures() {
    let versions = [
        // Versions with invalid local versions
        "1.0+a+",
        "1.0++",
        "1.0+_foobar",
        "1.0+foo&asd",
        "1.0+1+1",
        // Nonsensical versions should also be invalid
        "french toast",
        "==french toast",
    ];
    for version in versions {
        assert!(Version::from_str(version).is_err());
        assert!(VersionSpecifier::from_str(&format!("=={version}")).is_err());
    }
}

#[test]
fn test_equality_and_normalization() {
    let versions = [
        // Various development release incarnations
        ("1.0dev", "1.0.dev0"),
        ("1.0.dev", "1.0.dev0"),
        ("1.0dev1", "1.0.dev1"),
        ("1.0dev", "1.0.dev0"),
        ("1.0-dev", "1.0.dev0"),
        ("1.0-dev1", "1.0.dev1"),
        ("1.0DEV", "1.0.dev0"),
        ("1.0.DEV", "1.0.dev0"),
        ("1.0DEV1", "1.0.dev1"),
        ("1.0DEV", "1.0.dev0"),
        ("1.0.DEV1", "1.0.dev1"),
        ("1.0-DEV", "1.0.dev0"),
        ("1.0-DEV1", "1.0.dev1"),
        // Various alpha incarnations
        ("1.0a", "1.0a0"),
        ("1.0.a", "1.0a0"),
        ("1.0.a1", "1.0a1"),
        ("1.0-a", "1.0a0"),
        ("1.0-a1", "1.0a1"),
        ("1.0alpha", "1.0a0"),
        ("1.0.alpha", "1.0a0"),
        ("1.0.alpha1", "1.0a1"),
        ("1.0-alpha", "1.0a0"),
        ("1.0-alpha1", "1.0a1"),
        ("1.0A", "1.0a0"),
        ("1.0.A", "1.0a0"),
        ("1.0.A1", "1.0a1"),
        ("1.0-A", "1.0a0"),
        ("1.0-A1", "1.0a1"),
        ("1.0ALPHA", "1.0a0"),
        ("1.0.ALPHA", "1.0a0"),
        ("1.0.ALPHA1", "1.0a1"),
        ("1.0-ALPHA", "1.0a0"),
        ("1.0-ALPHA1", "1.0a1"),
        // Various beta incarnations
        ("1.0b", "1.0b0"),
        ("1.0.b", "1.0b0"),
        ("1.0.b1", "1.0b1"),
        ("1.0-b", "1.0b0"),
        ("1.0-b1", "1.0b1"),
        ("1.0beta", "1.0b0"),
        ("1.0.beta", "1.0b0"),
        ("1.0.beta1", "1.0b1"),
        ("1.0-beta", "1.0b0"),
        ("1.0-beta1", "1.0b1"),
        ("1.0B", "1.0b0"),
        ("1.0.B", "1.0b0"),
        ("1.0.B1", "1.0b1"),
        ("1.0-B", "1.0b0"),
        ("1.0-B1", "1.0b1"),
        ("1.0BETA", "1.0b0"),
        ("1.0.BETA", "1.0b0"),
        ("1.0.BETA1", "1.0b1"),
        ("1.0-BETA", "1.0b0"),
        ("1.0-BETA1", "1.0b1"),
        // Various release candidate incarnations
        ("1.0c", "1.0rc0"),
        ("1.0.c", "1.0rc0"),
        ("1.0.c1", "1.0rc1"),
        ("1.0-c", "1.0rc0"),
        ("1.0-c1", "1.0rc1"),
        ("1.0rc", "1.0rc0"),
        ("1.0.rc", "1.0rc0"),
        ("1.0.rc1", "1.0rc1"),
        ("1.0-rc", "1.0rc0"),
        ("1.0-rc1", "1.0rc1"),
        ("1.0C", "1.0rc0"),
        ("1.0.C", "1.0rc0"),
        ("1.0.C1", "1.0rc1"),
        ("1.0-C", "1.0rc0"),
        ("1.0-C1", "1.0rc1"),
        ("1.0RC", "1.0rc0"),
        ("1.0.RC", "1.0rc0"),
        ("1.0.RC1", "1.0rc1"),
        ("1.0-RC", "1.0rc0"),
        ("1.0-RC1", "1.0rc1"),
        // Various post release incarnations
        ("1.0post", "1.0.post0"),
        ("1.0.post", "1.0.post0"),
        ("1.0post1", "1.0.post1"),
        ("1.0post", "1.0.post0"),
        ("1.0-post", "1.0.post0"),
        ("1.0-post1", "1.0.post1"),
        ("1.0POST", "1.0.post0"),
        ("1.0.POST", "1.0.post0"),
        ("1.0POST1", "1.0.post1"),
        ("1.0POST", "1.0.post0"),
        ("1.0r", "1.0.post0"),
        ("1.0rev", "1.0.post0"),
        ("1.0.POST1", "1.0.post1"),
        ("1.0.r1", "1.0.post1"),
        ("1.0.rev1", "1.0.post1"),
        ("1.0-POST", "1.0.post0"),
        ("1.0-POST1", "1.0.post1"),
        ("1.0-5", "1.0.post5"),
        ("1.0-r5", "1.0.post5"),
        ("1.0-rev5", "1.0.post5"),
        // Local version case insensitivity
        ("1.0+AbC", "1.0+abc"),
        // Integer Normalization
        ("1.01", "1.1"),
        ("1.0a05", "1.0a5"),
        ("1.0b07", "1.0b7"),
        ("1.0c056", "1.0rc56"),
        ("1.0rc09", "1.0rc9"),
        ("1.0.post000", "1.0.post0"),
        ("1.1.dev09000", "1.1.dev9000"),
        ("00!1.2", "1.2"),
        ("0100!0.0", "100!0.0"),
        // Various other normalizations
        ("v1.0", "1.0"),
        ("   v1.0\t\n", "1.0"),
    ];
    for (version_str, normalized_str) in versions {
        let version = Version::from_str(version_str).unwrap();
        let normalized = Version::from_str(normalized_str).unwrap();
        // Just test version parsing again
        assert_eq!(version, normalized, "{version_str} {normalized_str}");
        // Test version normalization
        assert_eq!(
            version.to_string(),
            normalized.to_string(),
            "{version_str} {normalized_str}"
        );
    }
}

/// <https://github.com/pypa/packaging/blob/237ff3aa348486cf835a980592af3a59fccd6101/tests/test_version.py#L229-L277>
#[test]
fn test_equality_and_normalization2() {
    let versions = [
        ("1.0.dev456", "1.0.dev456"),
        ("1.0a1", "1.0a1"),
        ("1.0a2.dev456", "1.0a2.dev456"),
        ("1.0a12.dev456", "1.0a12.dev456"),
        ("1.0a12", "1.0a12"),
        ("1.0b1.dev456", "1.0b1.dev456"),
        ("1.0b2", "1.0b2"),
        ("1.0b2.post345.dev456", "1.0b2.post345.dev456"),
        ("1.0b2.post345", "1.0b2.post345"),
        ("1.0rc1.dev456", "1.0rc1.dev456"),
        ("1.0rc1", "1.0rc1"),
        ("1.0", "1.0"),
        ("1.0.post456.dev34", "1.0.post456.dev34"),
        ("1.0.post456", "1.0.post456"),
        ("1.0.1", "1.0.1"),
        ("0!1.0.2", "1.0.2"),
        ("1.0.3+7", "1.0.3+7"),
        ("0!1.0.4+8.0", "1.0.4+8.0"),
        ("1.0.5+9.5", "1.0.5+9.5"),
        ("1.2+1234.abc", "1.2+1234.abc"),
        ("1.2+123456", "1.2+123456"),
        ("1.2+123abc", "1.2+123abc"),
        ("1.2+123abc456", "1.2+123abc456"),
        ("1.2+abc", "1.2+abc"),
        ("1.2+abc123", "1.2+abc123"),
        ("1.2+abc123def", "1.2+abc123def"),
        ("1.1.dev1", "1.1.dev1"),
        ("7!1.0.dev456", "7!1.0.dev456"),
        ("7!1.0a1", "7!1.0a1"),
        ("7!1.0a2.dev456", "7!1.0a2.dev456"),
        ("7!1.0a12.dev456", "7!1.0a12.dev456"),
        ("7!1.0a12", "7!1.0a12"),
        ("7!1.0b1.dev456", "7!1.0b1.dev456"),
        ("7!1.0b2", "7!1.0b2"),
        ("7!1.0b2.post345.dev456", "7!1.0b2.post345.dev456"),
        ("7!1.0b2.post345", "7!1.0b2.post345"),
        ("7!1.0rc1.dev456", "7!1.0rc1.dev456"),
        ("7!1.0rc1", "7!1.0rc1"),
        ("7!1.0", "7!1.0"),
        ("7!1.0.post456.dev34", "7!1.0.post456.dev34"),
        ("7!1.0.post456", "7!1.0.post456"),
        ("7!1.0.1", "7!1.0.1"),
        ("7!1.0.2", "7!1.0.2"),
        ("7!1.0.3+7", "7!1.0.3+7"),
        ("7!1.0.4+8.0", "7!1.0.4+8.0"),
        ("7!1.0.5+9.5", "7!1.0.5+9.5"),
        ("7!1.1.dev1", "7!1.1.dev1"),
    ];
    for (version_str, normalized_str) in versions {
        let version = Version::from_str(version_str).unwrap();
        let normalized = Version::from_str(normalized_str).unwrap();
        assert_eq!(version, normalized, "{version_str} {normalized_str}");
        // Test version normalization
        assert_eq!(
            version.to_string(),
            normalized_str,
            "{version_str} {normalized_str}"
        );
        // Since we're already at it
        assert_eq!(
            version.to_string(),
            normalized.to_string(),
            "{version_str} {normalized_str}"
        );
    }
}

#[test]
fn test_star_fixed_version() {
    let result = Version::from_str("0.9.1.*");
    assert_eq!(result.unwrap_err(), ErrorKind::Wildcard.into());
}

#[test]
fn test_invalid_word() {
    let result = Version::from_str("blergh");
    assert_eq!(result.unwrap_err(), ErrorKind::NoLeadingNumber.into());
}

#[test]
fn test_from_version_star() {
    let p = |s: &str| -> Result<VersionPattern, _> { s.parse() };
    assert!(!p("1.2.3").unwrap().is_wildcard());
    assert!(p("1.2.3.*").unwrap().is_wildcard());
    assert_eq!(
        p("1.2.*.4.*").unwrap_err(),
        PatternErrorKind::WildcardNotTrailing.into(),
    );
    assert_eq!(
        p("1.0-dev1.*").unwrap_err(),
        ErrorKind::UnexpectedEnd {
            version: "1.0-dev1".to_string(),
            remaining: ".*".to_string()
        }
        .into(),
    );
    assert_eq!(
        p("1.0a1.*").unwrap_err(),
        ErrorKind::UnexpectedEnd {
            version: "1.0a1".to_string(),
            remaining: ".*".to_string()
        }
        .into(),
    );
    assert_eq!(
        p("1.0.post1.*").unwrap_err(),
        ErrorKind::UnexpectedEnd {
            version: "1.0.post1".to_string(),
            remaining: ".*".to_string()
        }
        .into(),
    );
    assert_eq!(
        p("1.0+lolwat.*").unwrap_err(),
        ErrorKind::LocalEmpty { precursor: '.' }.into(),
    );
}

// Tests the valid cases of our version parser. These were written
// in tandem with the parser.
//
// They are meant to be additional (but in some cases likely redundant)
// with some of the above tests.
#[test]
fn parse_version_valid() {
    let p = |s: &str| match Parser::new(s.as_bytes()).parse() {
        Ok(v) => v,
        Err(err) => unreachable!("expected valid version, but got error: {err:?}"),
    };

    // release-only tests
    assert_eq!(p("5"), Version::new([5]));
    assert_eq!(p("5.6"), Version::new([5, 6]));
    assert_eq!(p("5.6.7"), Version::new([5, 6, 7]));
    assert_eq!(p("512.623.734"), Version::new([512, 623, 734]));
    assert_eq!(p("1.2.3.4"), Version::new([1, 2, 3, 4]));
    assert_eq!(p("1.2.3.4.5"), Version::new([1, 2, 3, 4, 5]));

    // epoch tests
    assert_eq!(p("4!5"), Version::new([5]).with_epoch(4));
    assert_eq!(p("4!5.6"), Version::new([5, 6]).with_epoch(4));

    // pre-release tests
    assert_eq!(
        p("5a1"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Alpha,
            number: 1
        }))
    );
    assert_eq!(
        p("5alpha1"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Alpha,
            number: 1
        }))
    );
    assert_eq!(
        p("5b1"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Beta,
            number: 1
        }))
    );
    assert_eq!(
        p("5beta1"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Beta,
            number: 1
        }))
    );
    assert_eq!(
        p("5rc1"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Rc,
            number: 1
        }))
    );
    assert_eq!(
        p("5c1"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Rc,
            number: 1
        }))
    );
    assert_eq!(
        p("5preview1"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Rc,
            number: 1
        }))
    );
    assert_eq!(
        p("5pre1"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Rc,
            number: 1
        }))
    );
    assert_eq!(
        p("5.6.7pre1"),
        Version::new([5, 6, 7]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Rc,
            number: 1
        }))
    );
    assert_eq!(
        p("5alpha789"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Alpha,
            number: 789
        }))
    );
    assert_eq!(
        p("5.alpha789"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Alpha,
            number: 789
        }))
    );
    assert_eq!(
        p("5-alpha789"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Alpha,
            number: 789
        }))
    );
    assert_eq!(
        p("5_alpha789"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Alpha,
            number: 789
        }))
    );
    assert_eq!(
        p("5alpha.789"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Alpha,
            number: 789
        }))
    );
    assert_eq!(
        p("5alpha-789"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Alpha,
            number: 789
        }))
    );
    assert_eq!(
        p("5alpha_789"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Alpha,
            number: 789
        }))
    );
    assert_eq!(
        p("5ALPHA789"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Alpha,
            number: 789
        }))
    );
    assert_eq!(
        p("5aLpHa789"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Alpha,
            number: 789
        }))
    );
    assert_eq!(
        p("5alpha"),
        Version::new([5]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Alpha,
            number: 0
        }))
    );

    // post-release tests
    assert_eq!(p("5post2"), Version::new([5]).with_post(Some(2)));
    assert_eq!(p("5rev2"), Version::new([5]).with_post(Some(2)));
    assert_eq!(p("5r2"), Version::new([5]).with_post(Some(2)));
    assert_eq!(p("5.post2"), Version::new([5]).with_post(Some(2)));
    assert_eq!(p("5-post2"), Version::new([5]).with_post(Some(2)));
    assert_eq!(p("5_post2"), Version::new([5]).with_post(Some(2)));
    assert_eq!(p("5.post.2"), Version::new([5]).with_post(Some(2)));
    assert_eq!(p("5.post-2"), Version::new([5]).with_post(Some(2)));
    assert_eq!(p("5.post_2"), Version::new([5]).with_post(Some(2)));
    assert_eq!(
        p("5.6.7.post_2"),
        Version::new([5, 6, 7]).with_post(Some(2))
    );
    assert_eq!(p("5-2"), Version::new([5]).with_post(Some(2)));
    assert_eq!(p("5.6.7-2"), Version::new([5, 6, 7]).with_post(Some(2)));
    assert_eq!(p("5POST2"), Version::new([5]).with_post(Some(2)));
    assert_eq!(p("5PoSt2"), Version::new([5]).with_post(Some(2)));
    assert_eq!(p("5post"), Version::new([5]).with_post(Some(0)));

    // dev-release tests
    assert_eq!(p("5dev2"), Version::new([5]).with_dev(Some(2)));
    assert_eq!(p("5.dev2"), Version::new([5]).with_dev(Some(2)));
    assert_eq!(p("5-dev2"), Version::new([5]).with_dev(Some(2)));
    assert_eq!(p("5_dev2"), Version::new([5]).with_dev(Some(2)));
    assert_eq!(p("5.dev.2"), Version::new([5]).with_dev(Some(2)));
    assert_eq!(p("5.dev-2"), Version::new([5]).with_dev(Some(2)));
    assert_eq!(p("5.dev_2"), Version::new([5]).with_dev(Some(2)));
    assert_eq!(p("5.6.7.dev_2"), Version::new([5, 6, 7]).with_dev(Some(2)));
    assert_eq!(p("5DEV2"), Version::new([5]).with_dev(Some(2)));
    assert_eq!(p("5dEv2"), Version::new([5]).with_dev(Some(2)));
    assert_eq!(p("5DeV2"), Version::new([5]).with_dev(Some(2)));
    assert_eq!(p("5dev"), Version::new([5]).with_dev(Some(0)));

    // local tests
    assert_eq!(
        p("5+2"),
        Version::new([5]).with_local(vec![LocalSegment::Number(2)])
    );
    assert_eq!(
        p("5+a"),
        Version::new([5]).with_local(vec![LocalSegment::String("a".to_string())])
    );
    assert_eq!(
        p("5+abc.123"),
        Version::new([5]).with_local(vec![
            LocalSegment::String("abc".to_string()),
            LocalSegment::Number(123),
        ])
    );
    assert_eq!(
        p("5+123.abc"),
        Version::new([5]).with_local(vec![
            LocalSegment::Number(123),
            LocalSegment::String("abc".to_string()),
        ])
    );
    assert_eq!(
        p("5+18446744073709551615.abc"),
        Version::new([5]).with_local(vec![
            LocalSegment::Number(18_446_744_073_709_551_615),
            LocalSegment::String("abc".to_string()),
        ])
    );
    assert_eq!(
        p("5+18446744073709551616.abc"),
        Version::new([5]).with_local(vec![
            LocalSegment::String("18446744073709551616".to_string()),
            LocalSegment::String("abc".to_string()),
        ])
    );
    assert_eq!(
        p("5+ABC.123"),
        Version::new([5]).with_local(vec![
            LocalSegment::String("abc".to_string()),
            LocalSegment::Number(123),
        ])
    );
    assert_eq!(
        p("5+ABC-123.4_5_xyz-MNO"),
        Version::new([5]).with_local(vec![
            LocalSegment::String("abc".to_string()),
            LocalSegment::Number(123),
            LocalSegment::Number(4),
            LocalSegment::Number(5),
            LocalSegment::String("xyz".to_string()),
            LocalSegment::String("mno".to_string()),
        ])
    );
    assert_eq!(
        p("5.6.7+abc-00123"),
        Version::new([5, 6, 7]).with_local(vec![
            LocalSegment::String("abc".to_string()),
            LocalSegment::Number(123),
        ])
    );
    assert_eq!(
        p("5.6.7+abc-foo00123"),
        Version::new([5, 6, 7]).with_local(vec![
            LocalSegment::String("abc".to_string()),
            LocalSegment::String("foo00123".to_string()),
        ])
    );
    assert_eq!(
        p("5.6.7+abc-00123a"),
        Version::new([5, 6, 7]).with_local(vec![
            LocalSegment::String("abc".to_string()),
            LocalSegment::String("00123a".to_string()),
        ])
    );

    // {pre-release, post-release} tests
    assert_eq!(
        p("5a2post3"),
        Version::new([5])
            .with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 2
            }))
            .with_post(Some(3))
    );
    assert_eq!(
        p("5.a-2_post-3"),
        Version::new([5])
            .with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 2
            }))
            .with_post(Some(3))
    );
    assert_eq!(
        p("5a2-3"),
        Version::new([5])
            .with_pre(Some(Prerelease {
                kind: PrereleaseKind::Alpha,
                number: 2
            }))
            .with_post(Some(3))
    );

    // Ignoring a no-op 'v' prefix.
    assert_eq!(p("v5"), Version::new([5]));
    assert_eq!(p("V5"), Version::new([5]));
    assert_eq!(p("v5.6.7"), Version::new([5, 6, 7]));

    // Ignoring leading and trailing whitespace.
    assert_eq!(p("  v5  "), Version::new([5]));
    assert_eq!(p("  5  "), Version::new([5]));
    assert_eq!(
        p("  5.6.7+abc.123.xyz  "),
        Version::new([5, 6, 7]).with_local(vec![
            LocalSegment::String("abc".to_string()),
            LocalSegment::Number(123),
            LocalSegment::String("xyz".to_string())
        ])
    );
    assert_eq!(p("  \n5\n \t"), Version::new([5]));

    // min tests
    assert!(Parser::new("1.min0".as_bytes()).parse().is_err());
}

// Tests the error cases of our version parser.
//
// I wrote these with the intent to cover every possible error
// case.
//
// They are meant to be additional (but in some cases likely redundant)
// with some of the above tests.
#[test]
fn parse_version_invalid() {
    let p = |s: &str| match Parser::new(s.as_bytes()).parse() {
        Err(err) => err,
        Ok(v) => unreachable!(
            "expected version parser error, but got: {v:?}",
            v = v.as_bloated_debug()
        ),
    };

    assert_eq!(p(""), ErrorKind::NoLeadingNumber.into());
    assert_eq!(p("a"), ErrorKind::NoLeadingNumber.into());
    assert_eq!(p("v 5"), ErrorKind::NoLeadingNumber.into());
    assert_eq!(p("V 5"), ErrorKind::NoLeadingNumber.into());
    assert_eq!(p("x 5"), ErrorKind::NoLeadingNumber.into());
    assert_eq!(
        p("18446744073709551616"),
        ErrorKind::NumberTooBig {
            bytes: b"18446744073709551616".to_vec()
        }
        .into()
    );
    assert_eq!(p("5!"), ErrorKind::NoLeadingReleaseNumber.into());
    assert_eq!(
        p("5.6./"),
        ErrorKind::UnexpectedEnd {
            version: "5.6".to_string(),
            remaining: "./".to_string()
        }
        .into()
    );
    assert_eq!(
        p("5.6.-alpha2"),
        ErrorKind::UnexpectedEnd {
            version: "5.6".to_string(),
            remaining: ".-alpha2".to_string()
        }
        .into()
    );
    assert_eq!(
        p("1.2.3a18446744073709551616"),
        ErrorKind::NumberTooBig {
            bytes: b"18446744073709551616".to_vec()
        }
        .into()
    );
    assert_eq!(p("5+"), ErrorKind::LocalEmpty { precursor: '+' }.into());
    assert_eq!(p("5+ "), ErrorKind::LocalEmpty { precursor: '+' }.into());
    assert_eq!(p("5+abc."), ErrorKind::LocalEmpty { precursor: '.' }.into());
    assert_eq!(p("5+abc-"), ErrorKind::LocalEmpty { precursor: '-' }.into());
    assert_eq!(p("5+abc_"), ErrorKind::LocalEmpty { precursor: '_' }.into());
    assert_eq!(
        p("5+abc. "),
        ErrorKind::LocalEmpty { precursor: '.' }.into()
    );
    assert_eq!(
        p("5.6-"),
        ErrorKind::UnexpectedEnd {
            version: "5.6".to_string(),
            remaining: "-".to_string()
        }
        .into()
    );
}

#[test]
fn parse_version_pattern_valid() {
    let p = |s: &str| match Parser::new(s.as_bytes()).parse_pattern() {
        Ok(v) => v,
        Err(err) => unreachable!("expected valid version, but got error: {err:?}"),
    };

    assert_eq!(p("5.*"), VersionPattern::wildcard(Version::new([5])));
    assert_eq!(p("5.6.*"), VersionPattern::wildcard(Version::new([5, 6])));
    assert_eq!(
        p("2!5.6.*"),
        VersionPattern::wildcard(Version::new([5, 6]).with_epoch(2))
    );
}

#[test]
fn parse_version_pattern_invalid() {
    let p = |s: &str| match Parser::new(s.as_bytes()).parse_pattern() {
        Err(err) => err,
        Ok(vpat) => unreachable!("expected version pattern parser error, but got: {vpat:?}"),
    };

    assert_eq!(p("*"), ErrorKind::NoLeadingNumber.into());
    assert_eq!(p("2!*"), ErrorKind::NoLeadingReleaseNumber.into());
}

// Tests that the ordering between versions is correct.
//
// The ordering example used here was taken from PEP 440:
// https://packaging.python.org/en/latest/specifications/version-specifiers/#summary-of-permitted-suffixes-and-relative-ordering
#[test]
fn ordering() {
    let versions = &[
        "1.dev0",
        "1.0.dev456",
        "1.0a1",
        "1.0a2.dev456",
        "1.0a12.dev456",
        "1.0a12",
        "1.0b1.dev456",
        "1.0b2",
        "1.0b2.post345.dev456",
        "1.0b2.post345",
        "1.0rc1.dev456",
        "1.0rc1",
        "1.0",
        "1.0+abc.5",
        "1.0+abc.7",
        "1.0+5",
        "1.0.post456.dev34",
        "1.0.post456",
        "1.0.15",
        "1.1.dev1",
    ];
    for (i, v1) in versions.iter().enumerate() {
        for v2 in &versions[i + 1..] {
            let less = v1.parse::<Version>().unwrap();
            let greater = v2.parse::<Version>().unwrap();
            assert_eq!(
                less.cmp(&greater),
                Ordering::Less,
                "less: {:?}\ngreater: {:?}",
                less.as_bloated_debug(),
                greater.as_bloated_debug()
            );
        }
    }
}

#[test]
fn min_version() {
    // Ensure that the `.min` suffix precedes all other suffixes.
    let less = Version::new([1, 0]).with_min(Some(0));

    let versions = &[
        "1.dev0",
        "1.0.dev456",
        "1.0a1",
        "1.0a2.dev456",
        "1.0a12.dev456",
        "1.0a12",
        "1.0b1.dev456",
        "1.0b2",
        "1.0b2.post345.dev456",
        "1.0b2.post345",
        "1.0rc1.dev456",
        "1.0rc1",
        "1.0",
        "1.0+abc.5",
        "1.0+abc.7",
        "1.0+5",
        "1.0.post456.dev34",
        "1.0.post456",
        "1.0.15",
        "1.1.dev1",
    ];

    for greater in versions {
        let greater = greater.parse::<Version>().unwrap();
        assert_eq!(
            less.cmp(&greater),
            Ordering::Less,
            "less: {:?}\ngreater: {:?}",
            less.as_bloated_debug(),
            greater.as_bloated_debug()
        );
    }
}

#[test]
fn max_version() {
    // Ensure that the `.max` suffix succeeds all other suffixes.
    let greater = Version::new([1, 0]).with_max(Some(0));

    let versions = &[
        "1.dev0",
        "1.0.dev456",
        "1.0a1",
        "1.0a2.dev456",
        "1.0a12.dev456",
        "1.0a12",
        "1.0b1.dev456",
        "1.0b2",
        "1.0b2.post345.dev456",
        "1.0b2.post345",
        "1.0rc1.dev456",
        "1.0rc1",
        "1.0",
        "1.0+abc.5",
        "1.0+abc.7",
        "1.0+5",
        "1.0.post456.dev34",
        "1.0.post456",
        "1.0",
    ];

    for less in versions {
        let less = less.parse::<Version>().unwrap();
        assert_eq!(
            less.cmp(&greater),
            Ordering::Less,
            "less: {:?}\ngreater: {:?}",
            less.as_bloated_debug(),
            greater.as_bloated_debug()
        );
    }

    // Ensure that the `.max` suffix plays nicely with pre-release versions.
    let greater = Version::new([1, 0])
        .with_pre(Some(Prerelease {
            kind: PrereleaseKind::Alpha,
            number: 1,
        }))
        .with_max(Some(0));

    let versions = &["1.0a1", "1.0a1+local", "1.0a1.post1"];

    for less in versions {
        let less = less.parse::<Version>().unwrap();
        assert_eq!(
            less.cmp(&greater),
            Ordering::Less,
            "less: {:?}\ngreater: {:?}",
            less.as_bloated_debug(),
            greater.as_bloated_debug()
        );
    }

    // Ensure that the `.max` suffix plays nicely with pre-release versions.
    let less = Version::new([1, 0])
        .with_pre(Some(Prerelease {
            kind: PrereleaseKind::Alpha,
            number: 1,
        }))
        .with_max(Some(0));

    let versions = &["1.0b1", "1.0b1+local", "1.0b1.post1", "1.0"];

    for greater in versions {
        let greater = greater.parse::<Version>().unwrap();
        assert_eq!(
            less.cmp(&greater),
            Ordering::Less,
            "less: {:?}\ngreater: {:?}",
            less.as_bloated_debug(),
            greater.as_bloated_debug()
        );
    }
}

// Tests our bespoke u64 decimal integer parser.
#[test]
fn parse_number_u64() {
    let p = |s: &str| parse_u64(s.as_bytes());
    assert_eq!(p("0"), Ok(0));
    assert_eq!(p("00"), Ok(0));
    assert_eq!(p("1"), Ok(1));
    assert_eq!(p("01"), Ok(1));
    assert_eq!(p("9"), Ok(9));
    assert_eq!(p("10"), Ok(10));
    assert_eq!(p("18446744073709551615"), Ok(18_446_744_073_709_551_615));
    assert_eq!(p("018446744073709551615"), Ok(18_446_744_073_709_551_615));
    assert_eq!(
        p("000000018446744073709551615"),
        Ok(18_446_744_073_709_551_615)
    );

    assert_eq!(p("10a"), Err(ErrorKind::InvalidDigit { got: b'a' }.into()));
    assert_eq!(p("10["), Err(ErrorKind::InvalidDigit { got: b'[' }.into()));
    assert_eq!(p("10/"), Err(ErrorKind::InvalidDigit { got: b'/' }.into()));
    assert_eq!(
        p("18446744073709551616"),
        Err(ErrorKind::NumberTooBig {
            bytes: b"18446744073709551616".to_vec()
        }
        .into())
    );
    assert_eq!(
        p("18446744073799551615abc"),
        Err(ErrorKind::NumberTooBig {
            bytes: b"18446744073799551615abc".to_vec()
        }
        .into())
    );
    assert_eq!(
        parse_u64(b"18446744073799551615\xFF"),
        Err(ErrorKind::NumberTooBig {
            bytes: b"18446744073799551615\xFF".to_vec()
        }
        .into())
    );
}

/// Wraps a `Version` and provides a more "bloated" debug but standard
/// representation.
///
/// We don't do this by default because it takes up a ton of space, and
/// just printing out the display version of the version is quite a bit
/// simpler.
///
/// Nevertheless, when *testing* version parsing, you really want to
/// be able to peek at all of its constituent parts. So we use this in
/// assertion failure messages.
struct VersionBloatedDebug<'a>(&'a Version);

impl<'a> std::fmt::Debug for VersionBloatedDebug<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Version")
            .field("epoch", &self.0.epoch())
            .field("release", &self.0.release())
            .field("pre", &self.0.pre())
            .field("post", &self.0.post())
            .field("dev", &self.0.dev())
            .field("local", &self.0.local())
            .field("min", &self.0.min())
            .field("max", &self.0.max())
            .finish()
    }
}

impl Version {
    pub(crate) fn as_bloated_debug(&self) -> impl std::fmt::Debug + '_ {
        VersionBloatedDebug(self)
    }
}

use std::{cmp::Ordering, str::FromStr};

use indoc::indoc;

use crate::LocalSegment;

use super::*;

/// <https://peps.python.org/pep-0440/#version-matching>
#[test]
fn test_equal() {
    let version = Version::from_str("1.1.post1").unwrap();

    assert!(!VersionSpecifier::from_str("== 1.1")
        .unwrap()
        .contains(&version));
    assert!(VersionSpecifier::from_str("== 1.1.post1")
        .unwrap()
        .contains(&version));
    assert!(VersionSpecifier::from_str("== 1.1.*")
        .unwrap()
        .contains(&version));
}

const VERSIONS_ALL: &[&str] = &[
    // Implicit epoch of 0
    "1.0.dev456",
    "1.0a1",
    "1.0a2.dev456",
    "1.0a12.dev456",
    "1.0a12",
    "1.0b1.dev456",
    "1.0b2",
    "1.0b2.post345.dev456",
    "1.0b2.post345",
    "1.0b2-346",
    "1.0c1.dev456",
    "1.0c1",
    "1.0rc2",
    "1.0c3",
    "1.0",
    "1.0.post456.dev34",
    "1.0.post456",
    "1.1.dev1",
    "1.2+123abc",
    "1.2+123abc456",
    "1.2+abc",
    "1.2+abc123",
    "1.2+abc123def",
    "1.2+1234.abc",
    "1.2+123456",
    "1.2.r32+123456",
    "1.2.rev33+123456",
    // Explicit epoch of 1
    "1!1.0.dev456",
    "1!1.0a1",
    "1!1.0a2.dev456",
    "1!1.0a12.dev456",
    "1!1.0a12",
    "1!1.0b1.dev456",
    "1!1.0b2",
    "1!1.0b2.post345.dev456",
    "1!1.0b2.post345",
    "1!1.0b2-346",
    "1!1.0c1.dev456",
    "1!1.0c1",
    "1!1.0rc2",
    "1!1.0c3",
    "1!1.0",
    "1!1.0.post456.dev34",
    "1!1.0.post456",
    "1!1.1.dev1",
    "1!1.2+123abc",
    "1!1.2+123abc456",
    "1!1.2+abc",
    "1!1.2+abc123",
    "1!1.2+abc123def",
    "1!1.2+1234.abc",
    "1!1.2+123456",
    "1!1.2.r32+123456",
    "1!1.2.rev33+123456",
];

/// <https://github.com/pypa/packaging/blob/237ff3aa348486cf835a980592af3a59fccd6101/tests/test_version.py#L666-L707>
/// <https://github.com/pypa/packaging/blob/237ff3aa348486cf835a980592af3a59fccd6101/tests/test_version.py#L709-L750>
///
/// These tests are a lot shorter than the pypa/packaging version since we implement all
/// comparisons through one method
#[test]
fn test_operators_true() {
    let versions: Vec<Version> = VERSIONS_ALL
        .iter()
        .map(|version| Version::from_str(version).unwrap())
        .collect();

    // Below we'll generate every possible combination of VERSIONS_ALL that
    // should be true for the given operator
    let operations = [
        // Verify that the less than (<) operator works correctly
        versions
            .iter()
            .enumerate()
            .flat_map(|(i, x)| {
                versions[i + 1..]
                    .iter()
                    .map(move |y| (x, y, Ordering::Less))
            })
            .collect::<Vec<_>>(),
        // Verify that the equal (==) operator works correctly
        versions
            .iter()
            .map(move |x| (x, x, Ordering::Equal))
            .collect::<Vec<_>>(),
        // Verify that the greater than (>) operator works correctly
        versions
            .iter()
            .enumerate()
            .flat_map(|(i, x)| versions[..i].iter().map(move |y| (x, y, Ordering::Greater)))
            .collect::<Vec<_>>(),
    ]
    .into_iter()
    .flatten();

    for (a, b, ordering) in operations {
        assert_eq!(a.cmp(b), ordering, "{a} {ordering:?} {b}");
    }
}

const VERSIONS_0: &[&str] = &[
    "1.0.dev456",
    "1.0a1",
    "1.0a2.dev456",
    "1.0a12.dev456",
    "1.0a12",
    "1.0b1.dev456",
    "1.0b2",
    "1.0b2.post345.dev456",
    "1.0b2.post345",
    "1.0b2-346",
    "1.0c1.dev456",
    "1.0c1",
    "1.0rc2",
    "1.0c3",
    "1.0",
    "1.0.post456.dev34",
    "1.0.post456",
    "1.1.dev1",
    "1.2+123abc",
    "1.2+123abc456",
    "1.2+abc",
    "1.2+abc123",
    "1.2+abc123def",
    "1.2+1234.abc",
    "1.2+123456",
    "1.2.r32+123456",
    "1.2.rev33+123456",
];

const SPECIFIERS_OTHER: &[&str] = &[
    "== 1.*", "== 1.0.*", "== 1.1.*", "== 1.2.*", "== 2.*", "~= 1.0", "~= 1.0b1", "~= 1.1",
    "~= 1.2", "~= 2.0",
];

const EXPECTED_OTHER: &[[bool; 10]] = &[
    [
        true, true, false, false, false, false, false, false, false, false,
    ],
    [
        true, true, false, false, false, false, false, false, false, false,
    ],
    [
        true, true, false, false, false, false, false, false, false, false,
    ],
    [
        true, true, false, false, false, false, false, false, false, false,
    ],
    [
        true, true, false, false, false, false, false, false, false, false,
    ],
    [
        true, true, false, false, false, false, false, false, false, false,
    ],
    [
        true, true, false, false, false, false, true, false, false, false,
    ],
    [
        true, true, false, false, false, false, true, false, false, false,
    ],
    [
        true, true, false, false, false, false, true, false, false, false,
    ],
    [
        true, true, false, false, false, false, true, false, false, false,
    ],
    [
        true, true, false, false, false, false, true, false, false, false,
    ],
    [
        true, true, false, false, false, false, true, false, false, false,
    ],
    [
        true, true, false, false, false, false, true, false, false, false,
    ],
    [
        true, true, false, false, false, false, true, false, false, false,
    ],
    [
        true, true, false, false, false, true, true, false, false, false,
    ],
    [
        true, true, false, false, false, true, true, false, false, false,
    ],
    [
        true, true, false, false, false, true, true, false, false, false,
    ],
    [
        true, false, true, false, false, true, true, false, false, false,
    ],
    [
        true, false, false, true, false, true, true, true, true, false,
    ],
    [
        true, false, false, true, false, true, true, true, true, false,
    ],
    [
        true, false, false, true, false, true, true, true, true, false,
    ],
    [
        true, false, false, true, false, true, true, true, true, false,
    ],
    [
        true, false, false, true, false, true, true, true, true, false,
    ],
    [
        true, false, false, true, false, true, true, true, true, false,
    ],
    [
        true, false, false, true, false, true, true, true, true, false,
    ],
    [
        true, false, false, true, false, true, true, true, true, false,
    ],
    [
        true, false, false, true, false, true, true, true, true, false,
    ],
];

/// Test for tilde equal (~=) and star equal (== x.y.*) recorded from pypa/packaging
///
/// Well, except for <https://github.com/pypa/packaging/issues/617>
#[test]
fn test_operators_other() {
    let versions = VERSIONS_0
        .iter()
        .map(|version| Version::from_str(version).unwrap());
    let specifiers: Vec<_> = SPECIFIERS_OTHER
        .iter()
        .map(|specifier| VersionSpecifier::from_str(specifier).unwrap())
        .collect();

    for (version, expected) in versions.zip(EXPECTED_OTHER) {
        let actual = specifiers
            .iter()
            .map(|specifier| specifier.contains(&version));
        for ((actual, expected), _specifier) in actual.zip(expected).zip(SPECIFIERS_OTHER) {
            assert_eq!(actual, *expected);
        }
    }
}

#[test]
fn test_arbitrary_equality() {
    assert!(VersionSpecifier::from_str("=== 1.2a1")
        .unwrap()
        .contains(&Version::from_str("1.2a1").unwrap()));
    assert!(!VersionSpecifier::from_str("=== 1.2a1")
        .unwrap()
        .contains(&Version::from_str("1.2a1+local").unwrap()));
}

#[test]
fn test_specifiers_true() {
    let pairs = [
        // Test the equality operation
        ("2.0", "==2"),
        ("2.0", "==2.0"),
        ("2.0", "==2.0.0"),
        ("2.0+deadbeef", "==2"),
        ("2.0+deadbeef", "==2.0"),
        ("2.0+deadbeef", "==2.0.0"),
        ("2.0+deadbeef", "==2+deadbeef"),
        ("2.0+deadbeef", "==2.0+deadbeef"),
        ("2.0+deadbeef", "==2.0.0+deadbeef"),
        ("2.0+deadbeef.0", "==2.0.0+deadbeef.00"),
        // Test the equality operation with a prefix
        ("2.dev1", "==2.*"),
        ("2a1", "==2.*"),
        ("2a1.post1", "==2.*"),
        ("2b1", "==2.*"),
        ("2b1.dev1", "==2.*"),
        ("2c1", "==2.*"),
        ("2c1.post1.dev1", "==2.*"),
        ("2c1.post1.dev1", "==2.0.*"),
        ("2rc1", "==2.*"),
        ("2rc1", "==2.0.*"),
        ("2", "==2.*"),
        ("2", "==2.0.*"),
        ("2", "==0!2.*"),
        ("0!2", "==2.*"),
        ("2.0", "==2.*"),
        ("2.0.0", "==2.*"),
        ("2.1+local.version", "==2.1.*"),
        // Test the in-equality operation
        ("2.1", "!=2"),
        ("2.1", "!=2.0"),
        ("2.0.1", "!=2"),
        ("2.0.1", "!=2.0"),
        ("2.0.1", "!=2.0.0"),
        ("2.0", "!=2.0+deadbeef"),
        // Test the in-equality operation with a prefix
        ("2.0", "!=3.*"),
        ("2.1", "!=2.0.*"),
        // Test the greater than equal operation
        ("2.0", ">=2"),
        ("2.0", ">=2.0"),
        ("2.0", ">=2.0.0"),
        ("2.0.post1", ">=2"),
        ("2.0.post1.dev1", ">=2"),
        ("3", ">=2"),
        // Test the less than equal operation
        ("2.0", "<=2"),
        ("2.0", "<=2.0"),
        ("2.0", "<=2.0.0"),
        ("2.0.dev1", "<=2"),
        ("2.0a1", "<=2"),
        ("2.0a1.dev1", "<=2"),
        ("2.0b1", "<=2"),
        ("2.0b1.post1", "<=2"),
        ("2.0c1", "<=2"),
        ("2.0c1.post1.dev1", "<=2"),
        ("2.0rc1", "<=2"),
        ("1", "<=2"),
        // Test the greater than operation
        ("3", ">2"),
        ("2.1", ">2.0"),
        ("2.0.1", ">2"),
        ("2.1.post1", ">2"),
        ("2.1+local.version", ">2"),
        // Test the less than operation
        ("1", "<2"),
        ("2.0", "<2.1"),
        ("2.0.dev0", "<2.1"),
        // Test the compatibility operation
        ("1", "~=1.0"),
        ("1.0.1", "~=1.0"),
        ("1.1", "~=1.0"),
        ("1.9999999", "~=1.0"),
        ("1.1", "~=1.0a1"),
        ("2022.01.01", "~=2022.01.01"),
        // Test that epochs are handled sanely
        ("2!1.0", "~=2!1.0"),
        ("2!1.0", "==2!1.*"),
        ("2!1.0", "==2!1.0"),
        ("2!1.0", "!=1.0"),
        ("1.0", "!=2!1.0"),
        ("1.0", "<=2!0.1"),
        ("2!1.0", ">=2.0"),
        ("1.0", "<2!0.1"),
        ("2!1.0", ">2.0"),
        // Test some normalization rules
        ("2.0.5", ">2.0dev"),
    ];

    for (s_version, s_spec) in pairs {
        let version = s_version.parse::<Version>().unwrap();
        let spec = s_spec.parse::<VersionSpecifier>().unwrap();
        assert!(
            spec.contains(&version),
            "{s_version} {s_spec}\nversion repr: {:?}\nspec version repr: {:?}",
            version.as_bloated_debug(),
            spec.version.as_bloated_debug(),
        );
    }
}

#[test]
fn test_specifier_false() {
    let pairs = [
        // Test the equality operation
        ("2.1", "==2"),
        ("2.1", "==2.0"),
        ("2.1", "==2.0.0"),
        ("2.0", "==2.0+deadbeef"),
        // Test the equality operation with a prefix
        ("2.0", "==3.*"),
        ("2.1", "==2.0.*"),
        // Test the in-equality operation
        ("2.0", "!=2"),
        ("2.0", "!=2.0"),
        ("2.0", "!=2.0.0"),
        ("2.0+deadbeef", "!=2"),
        ("2.0+deadbeef", "!=2.0"),
        ("2.0+deadbeef", "!=2.0.0"),
        ("2.0+deadbeef", "!=2+deadbeef"),
        ("2.0+deadbeef", "!=2.0+deadbeef"),
        ("2.0+deadbeef", "!=2.0.0+deadbeef"),
        ("2.0+deadbeef.0", "!=2.0.0+deadbeef.00"),
        // Test the in-equality operation with a prefix
        ("2.dev1", "!=2.*"),
        ("2a1", "!=2.*"),
        ("2a1.post1", "!=2.*"),
        ("2b1", "!=2.*"),
        ("2b1.dev1", "!=2.*"),
        ("2c1", "!=2.*"),
        ("2c1.post1.dev1", "!=2.*"),
        ("2c1.post1.dev1", "!=2.0.*"),
        ("2rc1", "!=2.*"),
        ("2rc1", "!=2.0.*"),
        ("2", "!=2.*"),
        ("2", "!=2.0.*"),
        ("2.0", "!=2.*"),
        ("2.0.0", "!=2.*"),
        // Test the greater than equal operation
        ("2.0.dev1", ">=2"),
        ("2.0a1", ">=2"),
        ("2.0a1.dev1", ">=2"),
        ("2.0b1", ">=2"),
        ("2.0b1.post1", ">=2"),
        ("2.0c1", ">=2"),
        ("2.0c1.post1.dev1", ">=2"),
        ("2.0rc1", ">=2"),
        ("1", ">=2"),
        // Test the less than equal operation
        ("2.0.post1", "<=2"),
        ("2.0.post1.dev1", "<=2"),
        ("3", "<=2"),
        // Test the greater than operation
        ("1", ">2"),
        ("2.0.dev1", ">2"),
        ("2.0a1", ">2"),
        ("2.0a1.post1", ">2"),
        ("2.0b1", ">2"),
        ("2.0b1.dev1", ">2"),
        ("2.0c1", ">2"),
        ("2.0c1.post1.dev1", ">2"),
        ("2.0rc1", ">2"),
        ("2.0", ">2"),
        ("2.0.post1", ">2"),
        ("2.0.post1.dev1", ">2"),
        ("2.0+local.version", ">2"),
        // Test the less than operation
        ("2.0.dev1", "<2"),
        ("2.0a1", "<2"),
        ("2.0a1.post1", "<2"),
        ("2.0b1", "<2"),
        ("2.0b2.dev1", "<2"),
        ("2.0c1", "<2"),
        ("2.0c1.post1.dev1", "<2"),
        ("2.0rc1", "<2"),
        ("2.0", "<2"),
        ("2.post1", "<2"),
        ("2.post1.dev1", "<2"),
        ("3", "<2"),
        // Test the compatibility operation
        ("2.0", "~=1.0"),
        ("1.1.0", "~=1.0.0"),
        ("1.1.post1", "~=1.0.0"),
        // Test that epochs are handled sanely
        ("1.0", "~=2!1.0"),
        ("2!1.0", "~=1.0"),
        ("2!1.0", "==1.0"),
        ("1.0", "==2!1.0"),
        ("2!1.0", "==1.*"),
        ("1.0", "==2!1.*"),
        ("2!1.0", "!=2!1.0"),
    ];
    for (version, specifier) in pairs {
        assert!(
            !VersionSpecifier::from_str(specifier)
                .unwrap()
                .contains(&Version::from_str(version).unwrap()),
            "{version} {specifier}"
        );
    }
}

#[test]
fn test_parse_version_specifiers() {
    let result = VersionSpecifiers::from_str("~= 0.9, >= 1.0, != 1.3.4.*, < 2.0").unwrap();
    assert_eq!(
        result.0,
        [
            VersionSpecifier {
                operator: Operator::TildeEqual,
                version: Version::new([0, 9]),
            },
            VersionSpecifier {
                operator: Operator::GreaterThanEqual,
                version: Version::new([1, 0]),
            },
            VersionSpecifier {
                operator: Operator::NotEqualStar,
                version: Version::new([1, 3, 4]),
            },
            VersionSpecifier {
                operator: Operator::LessThan,
                version: Version::new([2, 0]),
            }
        ]
    );
}

#[test]
fn test_parse_error() {
    let result = VersionSpecifiers::from_str("~= 0.9, %‍= 1.0, != 1.3.4.*");
    assert_eq!(
        result.unwrap_err().to_string(),
        indoc! {r"
            Failed to parse version: Unexpected end of version specifier, expected operator:
            ~= 0.9, %‍= 1.0, != 1.3.4.*
                   ^^^^^^^
        "}
    );
}

#[test]
fn test_non_star_after_star() {
    let result = VersionSpecifiers::from_str("== 0.9.*.1");
    assert_eq!(
        result.unwrap_err().inner.err,
        ParseErrorKind::InvalidVersion(version::PatternErrorKind::WildcardNotTrailing.into())
            .into(),
    );
}

#[test]
fn test_star_wrong_operator() {
    let result = VersionSpecifiers::from_str(">= 0.9.1.*");
    assert_eq!(
        result.unwrap_err().inner.err,
        ParseErrorKind::InvalidSpecifier(
            BuildErrorKind::OperatorWithStar {
                operator: Operator::GreaterThanEqual,
            }
            .into()
        )
        .into(),
    );
}

#[test]
fn test_invalid_word() {
    let result = VersionSpecifiers::from_str("blergh");
    assert_eq!(
        result.unwrap_err().inner.err,
        ParseErrorKind::MissingOperator.into(),
    );
}

/// <https://github.com/pypa/packaging/blob/e184feef1a28a5c574ec41f5c263a3a573861f5a/tests/test_specifiers.py#L44-L84>
#[test]
fn test_invalid_specifier() {
    let specifiers = [
        // Operator-less specifier
        ("2.0", ParseErrorKind::MissingOperator.into()),
        // Invalid operator
        (
            "=>2.0",
            ParseErrorKind::InvalidOperator(OperatorParseError {
                got: "=>".to_string(),
            })
            .into(),
        ),
        // Version-less specifier
        ("==", ParseErrorKind::MissingVersion.into()),
        // Local segment on operators which don't support them
        (
            "~=1.0+5",
            ParseErrorKind::InvalidSpecifier(
                BuildErrorKind::OperatorLocalCombo {
                    operator: Operator::TildeEqual,
                    version: Version::new([1, 0]).with_local(vec![LocalSegment::Number(5)]),
                }
                .into(),
            )
            .into(),
        ),
        (
            ">=1.0+deadbeef",
            ParseErrorKind::InvalidSpecifier(
                BuildErrorKind::OperatorLocalCombo {
                    operator: Operator::GreaterThanEqual,
                    version: Version::new([1, 0])
                        .with_local(vec![LocalSegment::String("deadbeef".to_string())]),
                }
                .into(),
            )
            .into(),
        ),
        (
            "<=1.0+abc123",
            ParseErrorKind::InvalidSpecifier(
                BuildErrorKind::OperatorLocalCombo {
                    operator: Operator::LessThanEqual,
                    version: Version::new([1, 0])
                        .with_local(vec![LocalSegment::String("abc123".to_string())]),
                }
                .into(),
            )
            .into(),
        ),
        (
            ">1.0+watwat",
            ParseErrorKind::InvalidSpecifier(
                BuildErrorKind::OperatorLocalCombo {
                    operator: Operator::GreaterThan,
                    version: Version::new([1, 0])
                        .with_local(vec![LocalSegment::String("watwat".to_string())]),
                }
                .into(),
            )
            .into(),
        ),
        (
            "<1.0+1.0",
            ParseErrorKind::InvalidSpecifier(
                BuildErrorKind::OperatorLocalCombo {
                    operator: Operator::LessThan,
                    version: Version::new([1, 0])
                        .with_local(vec![LocalSegment::Number(1), LocalSegment::Number(0)]),
                }
                .into(),
            )
            .into(),
        ),
        // Prefix matching on operators which don't support them
        (
            "~=1.0.*",
            ParseErrorKind::InvalidSpecifier(
                BuildErrorKind::OperatorWithStar {
                    operator: Operator::TildeEqual,
                }
                .into(),
            )
            .into(),
        ),
        (
            ">=1.0.*",
            ParseErrorKind::InvalidSpecifier(
                BuildErrorKind::OperatorWithStar {
                    operator: Operator::GreaterThanEqual,
                }
                .into(),
            )
            .into(),
        ),
        (
            "<=1.0.*",
            ParseErrorKind::InvalidSpecifier(
                BuildErrorKind::OperatorWithStar {
                    operator: Operator::LessThanEqual,
                }
                .into(),
            )
            .into(),
        ),
        (
            ">1.0.*",
            ParseErrorKind::InvalidSpecifier(
                BuildErrorKind::OperatorWithStar {
                    operator: Operator::GreaterThan,
                }
                .into(),
            )
            .into(),
        ),
        (
            "<1.0.*",
            ParseErrorKind::InvalidSpecifier(
                BuildErrorKind::OperatorWithStar {
                    operator: Operator::LessThan,
                }
                .into(),
            )
            .into(),
        ),
        // Combination of local and prefix matching on operators which do
        // support one or the other
        (
            "==1.0.*+5",
            ParseErrorKind::InvalidVersion(version::PatternErrorKind::WildcardNotTrailing.into())
                .into(),
        ),
        (
            "!=1.0.*+deadbeef",
            ParseErrorKind::InvalidVersion(version::PatternErrorKind::WildcardNotTrailing.into())
                .into(),
        ),
        // Prefix matching cannot be used with a pre-release, post-release,
        // dev or local version
        (
            "==2.0a1.*",
            ParseErrorKind::InvalidVersion(
                version::ErrorKind::UnexpectedEnd {
                    version: "2.0a1".to_string(),
                    remaining: ".*".to_string(),
                }
                .into(),
            )
            .into(),
        ),
        (
            "!=2.0a1.*",
            ParseErrorKind::InvalidVersion(
                version::ErrorKind::UnexpectedEnd {
                    version: "2.0a1".to_string(),
                    remaining: ".*".to_string(),
                }
                .into(),
            )
            .into(),
        ),
        (
            "==2.0.post1.*",
            ParseErrorKind::InvalidVersion(
                version::ErrorKind::UnexpectedEnd {
                    version: "2.0.post1".to_string(),
                    remaining: ".*".to_string(),
                }
                .into(),
            )
            .into(),
        ),
        (
            "!=2.0.post1.*",
            ParseErrorKind::InvalidVersion(
                version::ErrorKind::UnexpectedEnd {
                    version: "2.0.post1".to_string(),
                    remaining: ".*".to_string(),
                }
                .into(),
            )
            .into(),
        ),
        (
            "==2.0.dev1.*",
            ParseErrorKind::InvalidVersion(
                version::ErrorKind::UnexpectedEnd {
                    version: "2.0.dev1".to_string(),
                    remaining: ".*".to_string(),
                }
                .into(),
            )
            .into(),
        ),
        (
            "!=2.0.dev1.*",
            ParseErrorKind::InvalidVersion(
                version::ErrorKind::UnexpectedEnd {
                    version: "2.0.dev1".to_string(),
                    remaining: ".*".to_string(),
                }
                .into(),
            )
            .into(),
        ),
        (
            "==1.0+5.*",
            ParseErrorKind::InvalidVersion(
                version::ErrorKind::LocalEmpty { precursor: '.' }.into(),
            )
            .into(),
        ),
        (
            "!=1.0+deadbeef.*",
            ParseErrorKind::InvalidVersion(
                version::ErrorKind::LocalEmpty { precursor: '.' }.into(),
            )
            .into(),
        ),
        // Prefix matching must appear at the end
        (
            "==1.0.*.5",
            ParseErrorKind::InvalidVersion(version::PatternErrorKind::WildcardNotTrailing.into())
                .into(),
        ),
        // Compatible operator requires 2 digits in the release operator
        (
            "~=1",
            ParseErrorKind::InvalidSpecifier(BuildErrorKind::CompatibleRelease.into()).into(),
        ),
        // Cannot use a prefix matching after a .devN version
        (
            "==1.0.dev1.*",
            ParseErrorKind::InvalidVersion(
                version::ErrorKind::UnexpectedEnd {
                    version: "1.0.dev1".to_string(),
                    remaining: ".*".to_string(),
                }
                .into(),
            )
            .into(),
        ),
        (
            "!=1.0.dev1.*",
            ParseErrorKind::InvalidVersion(
                version::ErrorKind::UnexpectedEnd {
                    version: "1.0.dev1".to_string(),
                    remaining: ".*".to_string(),
                }
                .into(),
            )
            .into(),
        ),
    ];
    for (specifier, error) in specifiers {
        assert_eq!(VersionSpecifier::from_str(specifier).unwrap_err(), error);
    }
}

#[test]
fn test_display_start() {
    assert_eq!(
        VersionSpecifier::from_str("==     1.1.*")
            .unwrap()
            .to_string(),
        "==1.1.*"
    );
    assert_eq!(
        VersionSpecifier::from_str("!=     1.1.*")
            .unwrap()
            .to_string(),
        "!=1.1.*"
    );
}

#[test]
fn test_version_specifiers_str() {
    assert_eq!(
        VersionSpecifiers::from_str(">= 3.7").unwrap().to_string(),
        ">=3.7"
    );
    assert_eq!(
        VersionSpecifiers::from_str(">=3.7, <      4.0, != 3.9.0")
            .unwrap()
            .to_string(),
        ">=3.7, !=3.9.0, <4.0"
    );
}

/// These occur in the simple api, e.g.
/// <https://pypi.org/simple/geopandas/?format=application/vnd.pypi.simple.v1+json>
#[test]
fn test_version_specifiers_empty() {
    assert_eq!(VersionSpecifiers::from_str("").unwrap().to_string(), "");
}

/// All non-ASCII version specifiers are invalid, but the user can still
/// attempt to parse a non-ASCII string as a version specifier. This
/// ensures no panics occur and that the error reported has correct info.
#[test]
fn non_ascii_version_specifier() {
    let s = "💩";
    let err = s.parse::<VersionSpecifiers>().unwrap_err();
    assert_eq!(err.inner.start, 0);
    assert_eq!(err.inner.end, 4);

    // The first test here is plain ASCII and it gives the
    // expected result: the error starts at codepoint 12,
    // which is the start of `>5.%`.
    let s = ">=3.7, <4.0,>5.%";
    let err = s.parse::<VersionSpecifiers>().unwrap_err();
    assert_eq!(err.inner.start, 12);
    assert_eq!(err.inner.end, 16);
    // In this case, we replace a single ASCII codepoint
    // with U+3000 IDEOGRAPHIC SPACE. Its *visual* width is
    // 2 despite it being a single codepoint. This causes
    // the offsets in the error reporting logic to become
    // incorrect.
    //
    // ... it did. This bug was fixed by switching to byte
    // offsets.
    let s = ">=3.7,\u{3000}<4.0,>5.%";
    let err = s.parse::<VersionSpecifiers>().unwrap_err();
    assert_eq!(err.inner.start, 14);
    assert_eq!(err.inner.end, 18);
}

/// Tests the human readable error messages generated from an invalid
/// sequence of version specifiers.
#[test]
fn error_message_version_specifiers_parse_error() {
    let specs = ">=1.2.3, 5.4.3, >=3.4.5";
    let err = VersionSpecifierParseError {
        kind: Box::new(ParseErrorKind::MissingOperator),
    };
    let inner = Box::new(VersionSpecifiersParseErrorInner {
        err,
        line: specs.to_string(),
        start: 8,
        end: 14,
    });
    let err = VersionSpecifiersParseError { inner };
    assert_eq!(err, VersionSpecifiers::from_str(specs).unwrap_err());
    assert_eq!(
        err.to_string(),
        "\
Failed to parse version: Unexpected end of version specifier, expected operator:
>=1.2.3, 5.4.3, >=3.4.5
        ^^^^^^
"
    );
}

/// Tests the human readable error messages generated when building an
/// invalid version specifier.
#[test]
fn error_message_version_specifier_build_error() {
    let err = VersionSpecifierBuildError {
        kind: Box::new(BuildErrorKind::CompatibleRelease),
    };
    let op = Operator::TildeEqual;
    let v = Version::new([5]);
    let vpat = VersionPattern::verbatim(v);
    assert_eq!(err, VersionSpecifier::from_pattern(op, vpat).unwrap_err());
    assert_eq!(
        err.to_string(),
        "The ~= operator requires at least two segments in the release version"
    );
}

/// Tests the human readable error messages generated from parsing invalid
/// version specifier.
#[test]
fn error_message_version_specifier_parse_error() {
    let err = VersionSpecifierParseError {
        kind: Box::new(ParseErrorKind::InvalidSpecifier(
            VersionSpecifierBuildError {
                kind: Box::new(BuildErrorKind::CompatibleRelease),
            },
        )),
    };
    assert_eq!(err, VersionSpecifier::from_str("~=5").unwrap_err());
    assert_eq!(
        err.to_string(),
        "The ~= operator requires at least two segments in the release version"
    );
}

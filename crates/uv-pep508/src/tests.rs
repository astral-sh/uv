//! Half of these tests are copied from <https://github.com/pypa/packaging/pull/624>

use std::env;
use std::str::FromStr;

use insta::assert_snapshot;
use url::Url;

use uv_normalize::{ExtraName, InvalidNameError, PackageName};
use uv_pep440::{Operator, Version, VersionPattern, VersionSpecifier};

use crate::cursor::Cursor;
use crate::marker::{parse, MarkerExpression, MarkerTree, MarkerValueVersion};
use crate::{
    MarkerOperator, MarkerValueString, Requirement, TracingReporter, VerbatimUrl, VersionOrUrl,
};

fn parse_pep508_err(input: &str) -> String {
    Requirement::<VerbatimUrl>::from_str(input)
        .unwrap_err()
        .to_string()
}

#[cfg(feature = "non-pep508-extensions")]
fn parse_unnamed_err(input: &str) -> String {
    crate::UnnamedRequirement::<VerbatimUrl>::from_str(input)
        .unwrap_err()
        .to_string()
}

#[cfg(windows)]
#[test]
fn test_preprocess_url_windows() {
    use std::path::PathBuf;

    let actual = crate::parse_url::<VerbatimUrl>(
        &mut Cursor::new("file:///C:/Users/ferris/wheel-0.42.0.tar.gz"),
        None,
    )
    .unwrap()
    .to_file_path();
    let expected = PathBuf::from(r"C:\Users\ferris\wheel-0.42.0.tar.gz");
    assert_eq!(actual, Ok(expected));
}

#[test]
fn error_empty() {
    assert_snapshot!(
        parse_pep508_err(""),
        @r"
    Empty field is not allowed for PEP508

    ^"
    );
}

#[test]
fn error_start() {
    assert_snapshot!(
        parse_pep508_err("_name"),
        @"
        Expected package name starting with an alphanumeric character, found `_`
        _name
        ^"
    );
}

#[test]
fn error_end() {
    assert_snapshot!(
        parse_pep508_err("name_"),
        @"
        Package name must end with an alphanumeric character, not '_'
        name_
            ^"
    );
}

#[test]
fn basic_examples() {
    let input = r"requests[security,tests]==2.8.*,>=2.8.1 ; python_full_version < '2.7'";
    let requests = Requirement::<Url>::from_str(input).unwrap();
    assert_eq!(input, requests.to_string());
    let expected = Requirement {
        name: PackageName::from_str("requests").unwrap(),
        extras: vec![
            ExtraName::from_str("security").unwrap(),
            ExtraName::from_str("tests").unwrap(),
        ],
        version_or_url: Some(VersionOrUrl::VersionSpecifier(
            [
                VersionSpecifier::from_pattern(
                    Operator::Equal,
                    VersionPattern::wildcard(Version::new([2, 8])),
                )
                .unwrap(),
                VersionSpecifier::from_pattern(
                    Operator::GreaterThanEqual,
                    VersionPattern::verbatim(Version::new([2, 8, 1])),
                )
                .unwrap(),
            ]
            .into_iter()
            .collect(),
        )),
        marker: MarkerTree::expression(MarkerExpression::Version {
            key: MarkerValueVersion::PythonFullVersion,
            specifier: VersionSpecifier::from_pattern(
                uv_pep440::Operator::LessThan,
                "2.7".parse().unwrap(),
            )
            .unwrap(),
        }),
        origin: None,
    };
    assert_eq!(requests, expected);
}

#[test]
fn parenthesized_single() {
    let numpy = Requirement::<Url>::from_str("numpy ( >=1.19 )").unwrap();
    assert_eq!(numpy.name.as_ref(), "numpy");
}

#[test]
fn parenthesized_double() {
    let numpy = Requirement::<Url>::from_str("numpy ( >=1.19, <2.0 )").unwrap();
    assert_eq!(numpy.name.as_ref(), "numpy");
}

#[test]
fn versions_single() {
    let numpy = Requirement::<Url>::from_str("numpy >=1.19 ").unwrap();
    assert_eq!(numpy.name.as_ref(), "numpy");
}

#[test]
fn versions_double() {
    let numpy = Requirement::<Url>::from_str("numpy >=1.19, <2.0 ").unwrap();
    assert_eq!(numpy.name.as_ref(), "numpy");
}

#[test]
#[cfg(feature = "non-pep508-extensions")]
fn direct_url_no_extras() {
    let numpy = crate::UnnamedRequirement::<VerbatimUrl>::from_str("https://files.pythonhosted.org/packages/28/4a/46d9e65106879492374999e76eb85f87b15328e06bd1550668f79f7b18c6/numpy-1.26.4-cp312-cp312-win32.whl").unwrap();
    assert_eq!(numpy.url.to_string(), "https://files.pythonhosted.org/packages/28/4a/46d9e65106879492374999e76eb85f87b15328e06bd1550668f79f7b18c6/numpy-1.26.4-cp312-cp312-win32.whl");
    assert_eq!(numpy.extras, vec![]);
}

#[test]
#[cfg(all(unix, feature = "non-pep508-extensions"))]
fn direct_url_extras() {
    let numpy = crate::UnnamedRequirement::<VerbatimUrl>::from_str(
        "/path/to/numpy-1.26.4-cp312-cp312-win32.whl[dev]",
    )
    .unwrap();
    assert_eq!(
        numpy.url.to_string(),
        "file:///path/to/numpy-1.26.4-cp312-cp312-win32.whl"
    );
    assert_eq!(numpy.extras, vec![ExtraName::from_str("dev").unwrap()]);
}

#[test]
#[cfg(all(windows, feature = "non-pep508-extensions"))]
fn direct_url_extras() {
    let numpy = crate::UnnamedRequirement::<VerbatimUrl>::from_str(
        "C:\\path\\to\\numpy-1.26.4-cp312-cp312-win32.whl[dev]",
    )
    .unwrap();
    assert_eq!(
        numpy.url.to_string(),
        "file:///C:/path/to/numpy-1.26.4-cp312-cp312-win32.whl"
    );
    assert_eq!(numpy.extras, vec![ExtraName::from_str("dev").unwrap()]);
}

#[test]
fn error_extras_eof1() {
    assert_snapshot!(
        parse_pep508_err("black["),
        @r#"
    Missing closing bracket (expected ']', found end of dependency specification)
    black[
         ^
    "#
    );
}

#[test]
fn error_extras_eof2() {
    assert_snapshot!(
        parse_pep508_err("black[d"),
        @r#"
    Missing closing bracket (expected ']', found end of dependency specification)
    black[d
         ^
    "#
    );
}

#[test]
fn error_extras_eof3() {
    assert_snapshot!(
        parse_pep508_err("black[d,"),
        @r#"
    Missing closing bracket (expected ']', found end of dependency specification)
    black[d,
         ^
    "#
    );
}

#[test]
fn error_extras_illegal_start1() {
    assert_snapshot!(
        parse_pep508_err("black[ö]"),
        @r#"
    Expected an alphanumeric character starting the extra name, found `ö`
    black[ö]
          ^
    "#
    );
}

#[test]
fn error_extras_illegal_start2() {
    assert_snapshot!(
        parse_pep508_err("black[_d]"),
        @r#"
    Expected an alphanumeric character starting the extra name, found `_`
    black[_d]
          ^
    "#
    );
}

#[test]
fn error_extras_illegal_start3() {
    assert_snapshot!(
        parse_pep508_err("black[,]"),
        @r#"
    Expected either alphanumerical character (starting the extra name) or `]` (ending the extras section), found `,`
    black[,]
          ^
    "#
    );
}

#[test]
fn error_extras_illegal_character() {
    assert_snapshot!(
        parse_pep508_err("black[jüpyter]"),
        @r#"
    Invalid character in extras name, expected an alphanumeric character, `-`, `_`, `.`, `,` or `]`, found `ü`
    black[jüpyter]
           ^
    "#
    );
}

#[test]
fn error_extras1() {
    let numpy = Requirement::<Url>::from_str("black[d]").unwrap();
    assert_eq!(numpy.extras, vec![ExtraName::from_str("d").unwrap()]);
}

#[test]
fn error_extras2() {
    let numpy = Requirement::<Url>::from_str("black[d,jupyter]").unwrap();
    assert_eq!(
        numpy.extras,
        vec![
            ExtraName::from_str("d").unwrap(),
            ExtraName::from_str("jupyter").unwrap(),
        ]
    );
}

#[test]
fn empty_extras() {
    let black = Requirement::<Url>::from_str("black[]").unwrap();
    assert_eq!(black.extras, vec![]);
}

#[test]
fn empty_extras_with_spaces() {
    let black = Requirement::<Url>::from_str("black[  ]").unwrap();
    assert_eq!(black.extras, vec![]);
}

#[test]
fn error_extra_with_trailing_comma() {
    assert_snapshot!(
        parse_pep508_err("black[d,]"),
        @"
        Expected an alphanumeric character starting the extra name, found `]`
        black[d,]
                ^"
    );
}

#[test]
fn error_parenthesized_pep440() {
    assert_snapshot!(
        parse_pep508_err("numpy ( ><1.19 )"),
        @"
        no such comparison operator \"><\", must be one of ~= == != <= >= < > ===
        numpy ( ><1.19 )
                ^^^^^^^"
    );
}

#[test]
fn error_parenthesized_parenthesis() {
    assert_snapshot!(
        parse_pep508_err("numpy ( >=1.19"),
        @r#"
    Missing closing parenthesis (expected ')', found end of dependency specification)
    numpy ( >=1.19
          ^
    "#
    );
}

#[test]
fn error_whats_that() {
    assert_snapshot!(
        parse_pep508_err("numpy % 1.16"),
        @r#"
    Expected one of `@`, `(`, `<`, `=`, `>`, `~`, `!`, `;`, found `%`
    numpy % 1.16
          ^
    "#
    );
}

#[test]
fn url() {
    let pip_url =
        Requirement::from_str("pip @ https://github.com/pypa/pip/archive/1.3.1.zip#sha1=da9234ee9982d4bbb3c72346a6de940a148ea686")
            .unwrap();
    let url = "https://github.com/pypa/pip/archive/1.3.1.zip#sha1=da9234ee9982d4bbb3c72346a6de940a148ea686";
    let expected = Requirement {
        name: PackageName::from_str("pip").unwrap(),
        extras: vec![],
        marker: MarkerTree::TRUE,
        version_or_url: Some(VersionOrUrl::Url(Url::parse(url).unwrap())),
        origin: None,
    };
    assert_eq!(pip_url, expected);
}

#[test]
fn test_marker_parsing() {
    let marker = r#"python_version == "2.7" and (sys_platform == "win32" or (os_name == "linux" and implementation_name == 'cpython'))"#;
    let actual =
        parse::parse_markers_cursor::<VerbatimUrl>(&mut Cursor::new(marker), &mut TracingReporter)
            .unwrap()
            .unwrap();

    let mut a = MarkerTree::expression(MarkerExpression::Version {
        key: MarkerValueVersion::PythonVersion,
        specifier: VersionSpecifier::from_pattern(
            uv_pep440::Operator::Equal,
            "2.7".parse().unwrap(),
        )
        .unwrap(),
    });
    let mut b = MarkerTree::expression(MarkerExpression::String {
        key: MarkerValueString::SysPlatform,
        operator: MarkerOperator::Equal,
        value: "win32".to_string(),
    });
    let mut c = MarkerTree::expression(MarkerExpression::String {
        key: MarkerValueString::OsName,
        operator: MarkerOperator::Equal,
        value: "linux".to_string(),
    });
    let d = MarkerTree::expression(MarkerExpression::String {
        key: MarkerValueString::ImplementationName,
        operator: MarkerOperator::Equal,
        value: "cpython".to_string(),
    });

    c.and(d);
    b.or(c);
    a.and(b);

    assert_eq!(a, actual);
}

#[test]
fn name_and_marker() {
    Requirement::<Url>::from_str(r#"numpy; sys_platform == "win32" or (os_name == "linux" and implementation_name == 'cpython')"#).unwrap();
}

#[test]
fn error_marker_incomplete1() {
    assert_snapshot!(
        parse_pep508_err(r"numpy; sys_platform"),
        @r#"
    Expected a valid marker operator (such as `>=` or `not in`), found ``
    numpy; sys_platform
                       ^
    "#
    );
}

#[test]
fn error_marker_incomplete2() {
    assert_snapshot!(
        parse_pep508_err(r"numpy; sys_platform =="),
        @r#"
    Expected marker value, found end of dependency specification
    numpy; sys_platform ==
                          ^
    "#
    );
}

#[test]
fn error_marker_incomplete3() {
    assert_snapshot!(
        parse_pep508_err(r#"numpy; sys_platform == "win32" or"#),
        @r#"
    Expected marker value, found end of dependency specification
    numpy; sys_platform == "win32" or
                                     ^
    "#
    );
}

#[test]
fn error_marker_incomplete4() {
    assert_snapshot!(
        parse_pep508_err(r#"numpy; sys_platform == "win32" or (os_name == "linux""#),
        @r#"
    Expected ')', found end of dependency specification
    numpy; sys_platform == "win32" or (os_name == "linux"
                                      ^
    "#
    );
}

#[test]
fn error_marker_incomplete5() {
    assert_snapshot!(
        parse_pep508_err(r#"numpy; sys_platform == "win32" or (os_name == "linux" and"#),
        @r#"
    Expected marker value, found end of dependency specification
    numpy; sys_platform == "win32" or (os_name == "linux" and
                                                             ^
    "#
    );
}

#[test]
fn error_pep440() {
    assert_snapshot!(
        parse_pep508_err(r"numpy >=1.1.*"),
        @r#"
    Operator >= cannot be used with a wildcard version specifier
    numpy >=1.1.*
          ^^^^^^^
    "#
    );
}

#[test]
fn error_no_name() {
    assert_snapshot!(
        parse_pep508_err(r"==0.0"),
        @r"
    Expected package name starting with an alphanumeric character, found `=`
    ==0.0
    ^
    "
    );
}

#[test]
fn error_unnamedunnamed_url() {
    assert_snapshot!(
        parse_pep508_err(r"git+https://github.com/pallets/flask.git"),
        @"
        URL requirement must be preceded by a package name. Add the name of the package before the URL (e.g., `package_name @ https://...`).
        git+https://github.com/pallets/flask.git
        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^"
    );
}

#[test]
fn error_unnamed_file_path() {
    assert_snapshot!(
        parse_pep508_err(r"/path/to/flask.tar.gz"),
        @r###"
    URL requirement must be preceded by a package name. Add the name of the package before the URL (e.g., `package_name @ /path/to/file`).
    /path/to/flask.tar.gz
    ^^^^^^^^^^^^^^^^^^^^^
    "###
    );
}

#[test]
fn error_no_comma_between_extras() {
    assert_snapshot!(
        parse_pep508_err(r"name[bar baz]"),
        @r#"
    Expected either `,` (separating extras) or `]` (ending the extras section), found `b`
    name[bar baz]
             ^
    "#
    );
}

#[test]
fn error_extra_comma_after_extras() {
    assert_snapshot!(
        parse_pep508_err(r"name[bar, baz,]"),
        @r#"
    Expected an alphanumeric character starting the extra name, found `]`
    name[bar, baz,]
                  ^
    "#
    );
}

#[test]
fn error_extras_not_closed() {
    assert_snapshot!(
        parse_pep508_err(r"name[bar, baz >= 1.0"),
        @r#"
    Expected either `,` (separating extras) or `]` (ending the extras section), found `>`
    name[bar, baz >= 1.0
                  ^
    "#
    );
}

#[test]
fn error_no_space_after_url() {
    assert_snapshot!(
        parse_pep508_err(r"name @ https://example.com/; extra == 'example'"),
        @r#"
    Missing space before ';', the end of the URL is ambiguous
    name @ https://example.com/; extra == 'example'
                               ^
    "#
    );
}

#[test]
fn error_name_at_nothing() {
    assert_snapshot!(
        parse_pep508_err(r"name @"),
        @r#"
    Expected URL
    name @
          ^
    "#
    );
}

#[test]
fn test_error_invalid_marker_key() {
    assert_snapshot!(
        parse_pep508_err(r"name; invalid_name"),
        @r#"
    Expected a quoted string or a valid marker name, found `invalid_name`
    name; invalid_name
          ^^^^^^^^^^^^
    "#
    );
}

#[test]
fn error_markers_invalid_order() {
    assert_snapshot!(
        parse_pep508_err("name; '3.7' <= invalid_name"),
        @r#"
    Expected a quoted string or a valid marker name, found `invalid_name`
    name; '3.7' <= invalid_name
                   ^^^^^^^^^^^^
    "#
    );
}

#[test]
fn error_markers_notin() {
    assert_snapshot!(
        parse_pep508_err("name; '3.7' notin python_version"),
        @"
        Expected a valid marker operator (such as `>=` or `not in`), found `notin`
        name; '3.7' notin python_version
                    ^^^^^"
    );
}

#[test]
fn error_missing_quote() {
    assert_snapshot!(
        parse_pep508_err("name; python_version == 3.10"),
        @"
        Expected a quoted string or a valid marker name, found `3.10`
        name; python_version == 3.10
                                ^^^^
        "
    );
}

#[test]
fn error_markers_inpython_version() {
    assert_snapshot!(
        parse_pep508_err("name; '3.6'inpython_version"),
        @r#"
    Expected a valid marker operator (such as `>=` or `not in`), found `inpython_version`
    name; '3.6'inpython_version
               ^^^^^^^^^^^^^^^^
    "#
    );
}

#[test]
fn error_markers_not_python_version() {
    assert_snapshot!(
        parse_pep508_err("name; '3.7' not python_version"),
        @"
        Expected `i`, found `p`
        name; '3.7' not python_version
                        ^"
    );
}

#[test]
fn error_markers_invalid_operator() {
    assert_snapshot!(
        parse_pep508_err("name; '3.7' ~ python_version"),
        @"
        Expected a valid marker operator (such as `>=` or `not in`), found `~`
        name; '3.7' ~ python_version
                    ^"
    );
}

#[test]
fn error_invalid_prerelease() {
    assert_snapshot!(
        parse_pep508_err("name==1.0.org1"),
        @r###"
    after parsing `1.0`, found `.org1`, which is not part of a valid version
    name==1.0.org1
        ^^^^^^^^^^
    "###
    );
}

#[test]
fn error_no_version_value() {
    assert_snapshot!(
        parse_pep508_err("name=="),
        @"
        Unexpected end of version specifier, expected version
        name==
            ^^"
    );
}

#[test]
fn error_no_version_operator() {
    assert_snapshot!(
        parse_pep508_err("name 1.0"),
        @r#"
    Expected one of `@`, `(`, `<`, `=`, `>`, `~`, `!`, `;`, found `1`
    name 1.0
         ^
    "#
    );
}

#[test]
fn error_random_char() {
    assert_snapshot!(
        parse_pep508_err("name >= 1.0 #"),
        @r##"
    Trailing `#` is not allowed
    name >= 1.0 #
         ^^^^^^^^
    "##
    );
}

#[test]
#[cfg(feature = "non-pep508-extensions")]
fn error_invalid_extra_unnamed_url() {
    assert_snapshot!(
        parse_unnamed_err("/foo-3.0.0-py3-none-any.whl[d,]"),
        @r#"
    Expected an alphanumeric character starting the extra name, found `]`
    /foo-3.0.0-py3-none-any.whl[d,]
                                  ^
    "#
    );
}

/// Check that the relative path support feature toggle works.
#[test]
fn non_pep508_paths() {
    let requirements = &[
        "foo @ file://./foo",
        "foo @ file://foo-3.0.0-py3-none-any.whl",
        "foo @ file:foo-3.0.0-py3-none-any.whl",
        "foo @ ./foo-3.0.0-py3-none-any.whl",
    ];
    let cwd = env::current_dir().unwrap();

    for requirement in requirements {
        assert_eq!(
            Requirement::<VerbatimUrl>::parse(requirement, &cwd).is_ok(),
            cfg!(feature = "non-pep508-extensions"),
            "{}: {:?}",
            requirement,
            Requirement::<VerbatimUrl>::parse(requirement, &cwd)
        );
    }
}

#[test]
fn no_space_after_operator() {
    let requirement = Requirement::<Url>::from_str("pytest;python_version<='4.0'").unwrap();
    assert_eq!(
        requirement.to_string(),
        "pytest ; python_full_version < '4.1'"
    );

    let requirement = Requirement::<Url>::from_str("pytest;'4.0'>=python_version").unwrap();
    assert_eq!(
        requirement.to_string(),
        "pytest ; python_full_version < '4.1'"
    );
}

#[test]
fn path_with_fragment() {
    let requirements = if cfg!(windows) {
        &[
            "wheel @ file:///C:/Users/ferris/wheel-0.42.0.whl#hash=somehash",
            "wheel @ C:/Users/ferris/wheel-0.42.0.whl#hash=somehash",
        ]
    } else {
        &[
            "wheel @ file:///Users/ferris/wheel-0.42.0.whl#hash=somehash",
            "wheel @ /Users/ferris/wheel-0.42.0.whl#hash=somehash",
        ]
    };

    for requirement in requirements {
        // Extract the URL.
        let Some(VersionOrUrl::Url(url)) = Requirement::<VerbatimUrl>::from_str(requirement)
            .unwrap()
            .version_or_url
        else {
            unreachable!("Expected a URL")
        };

        // Assert that the fragment and path have been separated correctly.
        assert_eq!(url.fragment(), Some("hash=somehash"));
        assert!(
            url.path().ends_with("/Users/ferris/wheel-0.42.0.whl"),
            "Expected the path to end with `/Users/ferris/wheel-0.42.0.whl`, found `{}`",
            url.path()
        );
    }
}

#[test]
fn add_extra_marker() -> Result<(), InvalidNameError> {
    let requirement = Requirement::<Url>::from_str("pytest").unwrap();
    let expected = Requirement::<Url>::from_str("pytest; extra == 'dotenv'").unwrap();
    let actual = requirement.with_extra_marker(&ExtraName::from_str("dotenv")?);
    assert_eq!(actual, expected);

    let requirement = Requirement::<Url>::from_str("pytest; '4.0' >= python_version").unwrap();
    let expected =
        Requirement::from_str("pytest; '4.0' >= python_version and extra == 'dotenv'").unwrap();
    let actual = requirement.with_extra_marker(&ExtraName::from_str("dotenv")?);
    assert_eq!(actual, expected);

    let requirement =
        Requirement::<Url>::from_str("pytest; '4.0' >= python_version or sys_platform == 'win32'")
            .unwrap();
    let expected = Requirement::from_str(
        "pytest; ('4.0' >= python_version or sys_platform == 'win32') and extra == 'dotenv'",
    )
    .unwrap();
    let actual = requirement.with_extra_marker(&ExtraName::from_str("dotenv")?);
    assert_eq!(actual, expected);

    Ok(())
}

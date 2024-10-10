use std::str::FromStr;

use uv_pep440::VersionSpecifiers;
use uv_pep508::Requirement;

use crate::LenientVersionSpecifiers;

use super::LenientRequirement;

#[test]
fn requirement_missing_comma() {
    let actual: Requirement = LenientRequirement::from_str("elasticsearch-dsl (>=7.2.0<8.0.0)")
        .unwrap()
        .into();
    let expected: Requirement =
        Requirement::from_str("elasticsearch-dsl (>=7.2.0,<8.0.0)").unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn requirement_not_equal_tile() {
    let actual: Requirement = LenientRequirement::from_str("jupyter-core (!=~5.0,>=4.12)")
        .unwrap()
        .into();
    let expected: Requirement = Requirement::from_str("jupyter-core (!=5.0.*,>=4.12)").unwrap();
    assert_eq!(actual, expected);

    let actual: Requirement = LenientRequirement::from_str("jupyter-core (!=~5,>=4.12)")
        .unwrap()
        .into();
    let expected: Requirement = Requirement::from_str("jupyter-core (!=5.*,>=4.12)").unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn requirement_greater_than_star() {
    let actual: Requirement = LenientRequirement::from_str("torch (>=1.9.*)")
        .unwrap()
        .into();
    let expected: Requirement = Requirement::from_str("torch (>=1.9)").unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn requirement_missing_dot() {
    let actual: Requirement = LenientRequirement::from_str("pyzmq (>=2.7,!=3.0*,!=3.1*,!=3.2*)")
        .unwrap()
        .into();
    let expected: Requirement =
        Requirement::from_str("pyzmq (>=2.7,!=3.0.*,!=3.1.*,!=3.2.*)").unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn requirement_trailing_comma() {
    let actual: Requirement = LenientRequirement::from_str("pyzmq >=3.6,").unwrap().into();
    let expected: Requirement = Requirement::from_str("pyzmq >=3.6").unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn specifier_missing_comma() {
    let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=7.2.0<8.0.0")
        .unwrap()
        .into();
    let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=7.2.0,<8.0.0").unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn specifier_not_equal_tile() {
    let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str("!=~5.0,>=4.12")
        .unwrap()
        .into();
    let expected: VersionSpecifiers = VersionSpecifiers::from_str("!=5.0.*,>=4.12").unwrap();
    assert_eq!(actual, expected);

    let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str("!=~5,>=4.12")
        .unwrap()
        .into();
    let expected: VersionSpecifiers = VersionSpecifiers::from_str("!=5.*,>=4.12").unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn specifier_greater_than_star() {
    let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=1.9.*")
        .unwrap()
        .into();
    let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=1.9").unwrap();
    assert_eq!(actual, expected);

    let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=1.*").unwrap().into();
    let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=1").unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn specifier_missing_dot() {
    let actual: VersionSpecifiers =
        LenientVersionSpecifiers::from_str(">=2.7,!=3.0*,!=3.1*,!=3.2*")
            .unwrap()
            .into();
    let expected: VersionSpecifiers =
        VersionSpecifiers::from_str(">=2.7,!=3.0.*,!=3.1.*,!=3.2.*").unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn specifier_trailing_comma() {
    let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=3.6,").unwrap().into();
    let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=3.6").unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn specifier_trailing_comma_trailing_space() {
    let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=3.6, ")
        .unwrap()
        .into();
    let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=3.6").unwrap();
    assert_eq!(actual, expected);
}

/// <https://pypi.org/simple/shellingham/?format=application/vnd.pypi.simple.v1+json>
#[test]
fn specifier_invalid_single_quotes() {
    let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">= '2.7'")
        .unwrap()
        .into();
    let expected: VersionSpecifiers = VersionSpecifiers::from_str(">= 2.7").unwrap();
    assert_eq!(actual, expected);
}

/// <https://pypi.org/simple/tensorflowonspark/?format=application/vnd.pypi.simple.v1+json>
#[test]
fn specifier_invalid_double_quotes() {
    let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=\"3.6\"")
        .unwrap()
        .into();
    let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=3.6").unwrap();
    assert_eq!(actual, expected);
}

/// <https://pypi.org/simple/celery/?format=application/vnd.pypi.simple.v1+json>
#[test]
fn specifier_multi_fix() {
    let actual: VersionSpecifiers =
        LenientVersionSpecifiers::from_str(">=2.7, !=3.0.*, !=3.1.*, !=3.2.*, !=3.3.*, !=3.4.*,")
            .unwrap()
            .into();
    let expected: VersionSpecifiers =
        VersionSpecifiers::from_str(">=2.7, !=3.0.*, !=3.1.*, !=3.2.*, !=3.3.*, !=3.4.*").unwrap();
    assert_eq!(actual, expected);
}

/// <https://pypi.org/simple/wincertstore/?format=application/vnd.pypi.simple.v1+json>
#[test]
fn smaller_than_star() {
    let actual: VersionSpecifiers =
        LenientVersionSpecifiers::from_str(">=2.7,!=3.0.*,!=3.1.*,<3.4.*")
            .unwrap()
            .into();
    let expected: VersionSpecifiers =
        VersionSpecifiers::from_str(">=2.7,!=3.0.*,!=3.1.*,<3.4").unwrap();
    assert_eq!(actual, expected);
}

/// <https://pypi.org/simple/algoliasearch/?format=application/vnd.pypi.simple.v1+json>
/// <https://pypi.org/simple/okta/?format=application/vnd.pypi.simple.v1+json>
#[test]
fn stray_quote() {
    let actual: VersionSpecifiers =
        LenientVersionSpecifiers::from_str(">=2.7, !=3.0.*, !=3.1.*', !=3.2.*, !=3.3.*'")
            .unwrap()
            .into();
    let expected: VersionSpecifiers =
        VersionSpecifiers::from_str(">=2.7, !=3.0.*, !=3.1.*, !=3.2.*, !=3.3.*").unwrap();
    assert_eq!(actual, expected);
    let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=3.6'").unwrap().into();
    let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=3.6").unwrap();
    assert_eq!(actual, expected);
}

/// <https://files.pythonhosted.org/packages/74/49/7349527cea7f708e7d3253ab6b32c9b5bdf84a57dde8fc265a33e6a4e662/boto3-1.2.0-py2.py3-none-any.whl>
#[test]
fn trailing_comma_after_quote() {
    let actual: Requirement = LenientRequirement::from_str("botocore>=1.3.0,<1.4.0',")
        .unwrap()
        .into();
    let expected: Requirement = Requirement::from_str("botocore>=1.3.0,<1.4.0").unwrap();
    assert_eq!(actual, expected);
}

/// <https://github.com/celery/celery/blob/6215f34d2675441ef2177bd850bf5f4b442e944c/requirements/default.txt#L1>
#[test]
fn greater_than_dev() {
    let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">dev").unwrap().into();
    let expected: VersionSpecifiers = VersionSpecifiers::from_str(">0.0.0dev").unwrap();
    assert_eq!(actual, expected);
}

/// <https://github.com/astral-sh/uv/issues/1798>
#[test]
fn trailing_alpha_zero() {
    let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=9.0.0a1.0")
        .unwrap()
        .into();
    let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=9.0.0a1").unwrap();
    assert_eq!(actual, expected);

    let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=9.0a1.0")
        .unwrap()
        .into();
    let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=9.0a1").unwrap();
    assert_eq!(actual, expected);

    let actual: VersionSpecifiers = LenientVersionSpecifiers::from_str(">=9a1.0")
        .unwrap()
        .into();
    let expected: VersionSpecifiers = VersionSpecifiers::from_str(">=9a1").unwrap();
    assert_eq!(actual, expected);
}

/// <https://github.com/astral-sh/uv/issues/2551>
#[test]
fn stray_quote_preserve_marker() {
    let actual: Requirement =
        LenientRequirement::from_str("numpy >=1.19; python_version >= \"3.7\"")
            .unwrap()
            .into();
    let expected: Requirement =
        Requirement::from_str("numpy >=1.19; python_version >= \"3.7\"").unwrap();
    assert_eq!(actual, expected);

    let actual: Requirement =
        LenientRequirement::from_str("numpy \">=1.19\"; python_version >= \"3.7\"")
            .unwrap()
            .into();
    let expected: Requirement =
        Requirement::from_str("numpy >=1.19; python_version >= \"3.7\"").unwrap();
    assert_eq!(actual, expected);

    let actual: Requirement =
        LenientRequirement::from_str("'numpy' >=1.19\"; python_version >= \"3.7\"")
            .unwrap()
            .into();
    let expected: Requirement =
        Requirement::from_str("numpy >=1.19; python_version >= \"3.7\"").unwrap();
    assert_eq!(actual, expected);
}

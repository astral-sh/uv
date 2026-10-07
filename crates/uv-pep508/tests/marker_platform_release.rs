use std::error::Error;

use uv_pep440::VersionParseError;
use uv_pep508::{
    MarkerEnvironment, MarkerEnvironmentBuilder, MarkerExpression, MarkerTree, MarkerValueString,
};

fn environment(
    sys_platform: &str,
    platform_release: &str,
    python_version: &str,
) -> Result<MarkerEnvironment, VersionParseError> {
    MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
        implementation_name: "cpython",
        implementation_version: python_version,
        os_name: "posix",
        platform_machine: "aarch64",
        platform_python_implementation: "CPython",
        platform_release,
        platform_system: "",
        platform_version: "",
        python_full_version: python_version,
        python_version,
        sys_platform,
    })
}

fn assert_roundtrip(
    marker: MarkerTree,
    environment: &MarkerEnvironment,
    expected: bool,
) -> Result<(), Box<dyn Error>> {
    for (marker, expected) in [(marker, expected), (marker.negate(), !expected)] {
        assert_eq!(marker.evaluate(environment, &[]), expected);

        let printed = marker.try_to_pep508()?.ok_or("missing marker contents")?;
        let roundtrip: MarkerTree = printed.parse()?;
        assert_eq!(roundtrip, marker);
        assert_eq!(roundtrip.evaluate(environment, &[]), expected);

        let serialized = serde_json::to_string(&marker.contents())?;
        let roundtrip: MarkerTree = serde_json::from_str(&serialized)?;
        assert_eq!(roundtrip, marker);
        assert_eq!(roundtrip.evaluate(environment, &[]), expected);
    }
    Ok(())
}

#[test]
fn darwin_release_exact_equality() -> Result<(), Box<dyn Error>> {
    let equality: MarkerTree = "platform_release == '24'".parse()?;
    let inequality: MarkerTree = "platform_release != '24'".parse()?;
    assert_eq!(equality.negate(), inequality);

    for (sys_platform, release, expected) in [
        ("darwin", "", false),
        ("darwin", "not-a-version", false),
        ("darwin", "24", true),
        ("darwin", "24.0.0", true),
        ("darwin", "25", false),
        ("linux", "", false),
        ("linux", "24", true),
        ("linux", "24.0.0", false),
    ] {
        let environment = environment(sys_platform, release, "3.13")?;
        assert_roundtrip(equality, &environment, expected)?;
        assert_eq!(inequality.evaluate(&environment, &[]), !expected);
    }
    Ok(())
}

#[test]
fn darwin_release_exact_equality_sets() -> Result<(), Box<dyn Error>> {
    let included: MarkerTree = "platform_release == '24' or platform_release == '25'".parse()?;
    let excluded: MarkerTree = "platform_release != '24' and platform_release != '25'".parse()?;
    assert_eq!(included.negate(), excluded);

    for (release, expected) in [
        ("", false),
        ("not-a-version", false),
        ("23", false),
        ("24", true),
        ("24.0.0", true),
        ("24.1", false),
        ("25", true),
        ("26", false),
    ] {
        let environment = environment("darwin", release, "3.13")?;
        assert_roundtrip(included, &environment, expected)?;
        assert_eq!(excluded.evaluate(&environment, &[]), !expected);
    }
    Ok(())
}

#[test]
fn darwin_release_exact_equality_python_branches() -> Result<(), Box<dyn Error>> {
    let marker: MarkerTree = "(platform_release == '24' and python_version >= '3.13') or \
         (platform_release != '24' and python_version < '3.13')"
        .parse()?;

    // Releases other than 24 select the Python <3.13 branch, including opaque releases.
    for (python_version, equal, other) in [("3.12", false, true), ("3.13", true, false)] {
        for (release, expected) in [
            ("", other),
            ("not-a-version", other),
            ("24", equal),
            ("24.0.0", equal),
            ("25", other),
        ] {
            let environment = environment("darwin", release, python_version)?;
            assert_roundtrip(marker, &environment, expected)?;
        }
    }
    Ok(())
}

fn assert_extended_evaluation(
    marker: MarkerTree,
    environment: &MarkerEnvironment,
    expected: bool,
) -> Result<(), Box<dyn Error>> {
    for (marker, expected) in [(marker, expected), (marker.negate(), !expected)] {
        assert_eq!(marker.evaluate(environment, &[]), expected);
        if let Some(printed) = marker.to_extended_string() {
            let roundtrip = MarkerTree::parse_extended(&printed)?;
            assert_eq!(roundtrip.evaluate(environment, &[]), expected, "{printed}");
        } else {
            assert!(marker.is_true());
        }
    }
    Ok(())
}

#[test]
fn darwin_release_comparison_fallback() -> Result<(), Box<dyn Error>> {
    for (release, constant, expected) in [
        ("23", "24", [false, true, true, true, false, false]),
        ("24", "24", [true, false, false, true, false, true]),
        ("24.0.0", "24", [true, false, false, true, false, true]),
        ("25", "24", [false, true, false, false, true, true]),
        ("24.10", "24.9", [false, true, false, false, true, true]),
        ("", "24", [false, true, false, false, false, false]),
        (
            "not-a-version",
            "24",
            [false, true, false, false, false, false],
        ),
        ("24", "opaque", [false, true, false, false, false, false]),
        ("opaque", "opaque", [true, false, false, true, false, true]),
        (
            "different",
            "opaque",
            [false, true, false, false, false, false],
        ),
        ("", "", [true, false, false, true, false, true]),
        ("24.10", "24.*", [true, false, false, false, false, false]),
        ("25", "24.*", [false, true, false, false, false, false]),
        ("24.*", "24.*", [true, false, false, true, false, true]),
    ] {
        let environment = environment("darwin", release, "3.13")?;
        for (operator, expected) in ["==", "!=", "<", "<=", ">", ">="].into_iter().zip(expected) {
            let expression =
                format!("sys_platform == 'darwin' and platform_release {operator} '{constant}'");
            let marker: MarkerTree = expression.parse()?;
            assert_eq!(
                marker.evaluate(&environment, &[]),
                expected,
                "{expression}, release={release}"
            );
            assert_extended_evaluation(marker, &environment, expected)?;

            let printed = marker.try_to_pep508()?.ok_or("missing marker contents")?;
            let roundtrip: MarkerTree = printed.parse()?;
            assert_eq!(
                roundtrip.evaluate(&environment, &[]),
                expected,
                "{printed}, release={release}"
            );
        }
    }
    Ok(())
}

#[test]
fn darwin_release_composition_retains_opaque_domain() -> Result<(), Box<dyn Error>> {
    for (left, right) in [
        ("platform_release >= '9'", "platform_release < '24'"),
        ("platform_release < '24'", "platform_release > '24'"),
    ] {
        let left: MarkerTree = left.parse()?;
        let right: MarkerTree = right.parse()?;
        for release in ["", "not-a-version", "3-invalid", "9", "24", "25"] {
            let environment = environment("darwin", release, "3.13")?;
            let left_value = left.evaluate(&environment, &[]);
            let right_value = right.evaluate(&environment, &[]);
            assert_extended_evaluation(left.or(right), &environment, left_value || right_value)?;
            assert_extended_evaluation(left.and(right), &environment, left_value && right_value)?;
        }
        for release in ["", "not-a-version", "3-invalid"] {
            let environment = environment("darwin", release, "3.13")?;
            assert!(!left.or(right).evaluate(&environment, &[]));
        }
    }
    Ok(())
}

#[test]
fn darwin_release_negated_ordering_serialization() -> Result<(), Box<dyn Error>> {
    let darwin: MarkerTree = "sys_platform == 'darwin'".parse()?;
    let lower: MarkerTree = "platform_release < '24'".parse()?;
    let upper: MarkerTree = "platform_release >= '24'".parse()?;
    let complement = darwin.and(lower.negate());
    let opaque = darwin.and(lower.or(upper).negate());

    assert!(complement.try_to_pep508().is_err());
    assert!(opaque.try_to_pep508().is_err());
    for (release, complement_expected, opaque_expected) in [
        ("", true, true),
        ("not-a-version", true, true),
        ("23", false, false),
        ("24", true, false),
        ("25", true, false),
    ] {
        let environment = environment("darwin", release, "3.13")?;
        assert_extended_evaluation(complement, &environment, complement_expected)?;
        assert_extended_evaluation(opaque, &environment, opaque_expected)?;
    }
    Ok(())
}

#[test]
fn logical_negation_requires_extended_parser() -> Result<(), Box<dyn Error>> {
    let darwin: MarkerTree = "sys_platform == 'darwin'".parse()?;
    let lower: MarkerTree = "platform_release < '24'".parse()?;
    let upper: MarkerTree = "platform_release >= '24'".parse()?;
    let equality: MarkerTree = "platform_release == '25'".parse()?;
    for (expression, expected) in [
        ("not (sys_platform == 'darwin')", darwin.negate()),
        ("not (not (platform_release < '24'))", lower),
        (
            "not (platform_release < '24' or platform_release >= '24')",
            lower.or(upper).negate(),
        ),
        (
            "sys_platform == 'darwin' and not (platform_release < '24') or platform_release == '25'",
            darwin.and(lower.negate()).or(equality),
        ),
    ] {
        assert!(expression.parse::<MarkerTree>().is_err());
        assert_eq!(MarkerTree::parse_extended(expression)?, expected);
    }
    for expression in [
        "not platform_release < '24'",
        "not ()",
        "not (platform_release < '24'",
        "notx (platform_release < '24')",
    ] {
        assert!(MarkerTree::parse_extended(expression).is_err());
    }
    let membership = "platform_release not in '24'";
    assert_eq!(MarkerTree::parse_extended(membership)?, membership.parse()?);
    Ok(())
}

#[test]
fn release_domain_expression_roundtrip() -> Result<(), Box<dyn Error>> {
    for valid in [false, true] {
        let expression = MarkerExpression::VersionStringDomain {
            key: MarkerValueString::PlatformRelease,
            valid,
        };
        let marker = MarkerTree::expression(expression.clone());
        assert_eq!(MarkerTree::parse_extended(&expression.to_string())?, marker);
        let printed = marker
            .to_extended_string()
            .ok_or("missing marker contents")?;
        assert_eq!(MarkerTree::parse_extended(&printed)?, marker);

        for (sys_platform, release, numeric_domain) in [
            ("darwin", "24", true),
            ("darwin", "not-a-version", false),
            ("linux", "24", true),
            ("linux", "not-a-version", true),
        ] {
            let environment = environment(sys_platform, release, "3.13")?;
            assert_eq!(marker.evaluate(&environment, &[]), valid == numeric_domain);
        }
    }
    Ok(())
}

#[test]
fn darwin_release_compound_standard_serialization() -> Result<(), Box<dyn Error>> {
    for expression in [
        "python_version < '3.13' or platform_release < '24'",
        "platform_release < '24' or (platform_release != '25' and python_version >= '3.13')",
        "platform_release != '0' and platform_release != '24.*'",
        "platform_release != '25' and platform_release != '24.*'",
        "platform_release != '24.0.*' and platform_release != '25'",
        "platform_release != '24.*' or platform_release >= '24.*'",
        "(platform_release != '24.*' or platform_release <= '24') and platform_release != '9'",
        "(platform_release != '24.*' or platform_release >= '24.*') and \
         (platform_release != '25.*' or platform_release >= '25.*')",
        "(platform_release != '24.1.*' or platform_release >= '24.1.*') and \
         (platform_release != '24.2.*' or platform_release >= '24.2.*')",
    ] {
        for expression in [
            expression.to_string(),
            format!("sys_platform == 'darwin' and ({expression})"),
        ] {
            let marker: MarkerTree = expression.parse()?;
            let printed = marker.try_to_pep508()?.ok_or("missing marker contents")?;
            let roundtrip: MarkerTree = printed.parse()?;
            assert_eq!(roundtrip, marker, "{expression}: {printed}");

            let serialized = serde_json::to_string(&marker.contents())?;
            assert_eq!(serde_json::from_str::<MarkerTree>(&serialized)?, marker);
        }
    }
    Ok(())
}

#[test]
fn darwin_release_standard_serialization_expansion_limit() -> Result<(), Box<dyn Error>> {
    let marker = MarkerTree::parse_extended(
        "sys_platform == 'darwin' and \
         (platform_release < '24' or platform_release >= '1000000' or \
          not (platform_release < '0' or platform_release >= '0'))",
    )?;
    assert!(marker.try_to_pep508().is_err());
    let printed = marker
        .to_extended_string()
        .ok_or("missing marker contents")?;
    assert_eq!(MarkerTree::parse_extended(&printed)?, marker);
    Ok(())
}

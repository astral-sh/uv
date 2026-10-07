use std::error::Error;

use uv_pep440::VersionParseError;
use uv_pep508::{MarkerEnvironment, MarkerEnvironmentBuilder, MarkerTree};

fn environment(
    sys_platform: &str,
    platform_release: &str,
) -> Result<MarkerEnvironment, VersionParseError> {
    MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
        implementation_name: "cpython",
        implementation_version: "3.13",
        os_name: "posix",
        platform_machine: "aarch64",
        platform_python_implementation: "CPython",
        platform_release,
        platform_system: "",
        platform_version: "",
        python_full_version: "3.13",
        python_version: "3.13",
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

        if let Some(printed) = marker.try_to_string() {
            let roundtrip: MarkerTree = printed.parse()?;
            assert_eq!(roundtrip, marker);
            assert_eq!(roundtrip.evaluate(environment, &[]), expected);
        } else {
            assert!(marker.is_true());
        }

        let serialized = serde_json::to_string(&marker.contents())?;
        let roundtrip: Option<MarkerTree> = serde_json::from_str(&serialized)?;
        let roundtrip = roundtrip.unwrap_or_default();
        assert_eq!(roundtrip, marker);
        assert_eq!(roundtrip.evaluate(environment, &[]), expected);
    }
    Ok(())
}

#[test]
fn darwin_invalid_release_negation() -> Result<(), Box<dyn Error>> {
    let marker: MarkerTree = "sys_platform == 'darwin' and platform_release == '24'".parse()?;
    let inequality: MarkerTree = "sys_platform == 'darwin' and platform_release != '24'".parse()?;

    for release in ["not-a-version", ""] {
        let environment = environment("darwin", release)?;
        assert!(!marker.evaluate(&environment, &[]));
        assert!(marker.negate().evaluate(&environment, &[]));
        assert!(inequality.evaluate(&environment, &[]));
    }
    Ok(())
}

#[test]
fn darwin_invalid_release_union() -> Result<(), Box<dyn Error>> {
    let environment = environment("darwin", "3-invalid")?;
    let lower: MarkerTree = "sys_platform == 'darwin' and platform_release >= '9'".parse()?;
    let upper: MarkerTree = "sys_platform == 'darwin' and platform_release < '24'".parse()?;
    let union: MarkerTree =
        "sys_platform == 'darwin' and (platform_release >= '9' or platform_release < '24')"
            .parse()?;

    // The numeric ranges cover all versions, but their lexical ranges leave a gap.
    assert!(!lower.evaluate(&environment, &[]));
    assert!(!upper.evaluate(&environment, &[]));
    assert!(!lower.or(upper).evaluate(&environment, &[]));
    assert_roundtrip(union, &environment, false)?;
    Ok(())
}

#[test]
fn darwin_release_disjointness_includes_lexical_ranges() -> Result<(), Box<dyn Error>> {
    let lower: MarkerTree = "sys_platform == 'darwin' and platform_release >= '25'".parse()?;
    let upper: MarkerTree = "sys_platform == 'darwin' and platform_release < '24'".parse()?;
    assert!(lower.is_disjoint(upper));

    let lower: MarkerTree = "sys_platform == 'darwin' and platform_release >= '24'".parse()?;
    let upper: MarkerTree = "sys_platform == 'darwin' and platform_release < '9'".parse()?;
    // These numeric ranges are disjoint, but both lexical comparisons accept this release.
    assert!(!lower.is_disjoint(upper));
    let environment = environment("darwin", "3-invalid")?;
    assert!(lower.evaluate(&environment, &[]));
    assert!(upper.evaluate(&environment, &[]));
    assert!(lower.and(upper).evaluate(&environment, &[]));
    Ok(())
}

#[test]
fn darwin_release_restriction_preserves_assumption() -> Result<(), Box<dyn Error>> {
    let required: MarkerTree = "sys_platform == 'darwin' and platform_release == '24'".parse()?;
    let coverage: MarkerTree = "sys_platform == 'darwin' and platform_release >= '25'".parse()?;
    let assumption = required.or(coverage);
    let darwin: MarkerTree = "sys_platform == 'darwin'".parse()?;
    let split = required.restrict(assumption).and(darwin);

    for release in ["24", "24.0", "24.1", "25", "25-invalid", ""] {
        let environment = environment("darwin", release)?;
        assert_roundtrip(split, &environment, split.evaluate(&environment, &[]))?;
        if assumption.evaluate(&environment, &[]) {
            assert_eq!(
                split.evaluate(&environment, &[]),
                required.evaluate(&environment, &[])
            );
        }
    }
    Ok(())
}

#[test]
fn darwin_release_numeric_and_lexical_comparisons() -> Result<(), Box<dyn Error>> {
    for (sys_platform, release, constant, expected) in [
        (
            "darwin",
            "23",
            "24",
            [false, true, true, true, false, false],
        ),
        (
            "darwin",
            "24.0.0",
            "24",
            [true, false, false, true, false, true],
        ),
        (
            "darwin",
            "24.10",
            "24.9",
            [false, true, false, false, true, true],
        ),
        ("darwin", "", "24", [false, true, true, true, false, false]),
        (
            "darwin",
            "not-a-version",
            "24",
            [false, true, false, false, true, true],
        ),
        (
            "darwin",
            "24",
            "opaque",
            [false, true, true, true, false, false],
        ),
        (
            "darwin",
            "opaque",
            "opaque",
            [true, false, false, true, false, true],
        ),
        (
            "linux",
            "24.10",
            "24.9",
            [false, true, true, true, false, false],
        ),
        (
            "linux",
            "24.0.0",
            "24",
            [false, true, false, false, true, true],
        ),
    ] {
        let environment = environment(sys_platform, release)?;
        for (operator, expected) in ["==", "!=", "<", "<=", ">", ">="].into_iter().zip(expected) {
            let marker: MarkerTree = format!("platform_release {operator} '{constant}'").parse()?;
            assert_roundtrip(marker, &environment, expected)?;
        }
    }
    Ok(())
}

#[test]
fn darwin_release_composition_matches_individual_comparisons() -> Result<(), Box<dyn Error>> {
    let lower: MarkerTree = "platform_release < '24'".parse()?;
    let upper: MarkerTree = "platform_release > '24'".parse()?;
    let outside: MarkerTree = "platform_release != '24'".parse()?;
    for sys_platform in ["darwin", "linux"] {
        for release in ["", "not-a-version", "24", "24.0", "25"] {
            let environment = environment(sys_platform, release)?;
            let lower_value = lower.evaluate(&environment, &[]);
            let upper_value = upper.evaluate(&environment, &[]);
            assert_eq!(
                outside.evaluate(&environment, &[]),
                lower_value || upper_value
            );
            assert_roundtrip(lower.or(upper), &environment, lower_value || upper_value)?;
            assert_roundtrip(lower.and(upper), &environment, lower_value && upper_value)?;
        }
    }
    Ok(())
}

#[test]
fn darwin_release_roundtrip_retains_lexical_threshold() -> Result<(), Box<dyn Error>> {
    let environment = environment("darwin", "24-invalid")?;

    // These thresholds denote the same version but have different lexical ordering.
    for (expression, expected) in [
        (
            "sys_platform == 'darwin' and platform_release >= '24'",
            true,
        ),
        (
            "sys_platform == 'darwin' and platform_release >= '24.0'",
            false,
        ),
    ] {
        let marker: MarkerTree = expression.parse()?;
        assert_roundtrip(marker, &environment, expected)?;
    }
    Ok(())
}

#[test]
fn darwin_release_wildcard_roundtrip() -> Result<(), Box<dyn Error>> {
    let marker: MarkerTree = "sys_platform == 'darwin' and platform_release == '24.*'".parse()?;
    for (release, expected) in [
        ("24.10", true),
        ("24.*", true),
        ("24-invalid", false),
        ("", false),
    ] {
        let environment = environment("darwin", release)?;
        assert_roundtrip(marker, &environment, expected)?;
    }
    Ok(())
}

#[test]
fn darwin_release_collective_contradictions() -> Result<(), Box<dyn Error>> {
    let darwin: MarkerTree = "sys_platform == 'darwin'".parse()?;
    for expressions in [
        [
            "platform_release != '24'",
            "platform_release <= '24'",
            "platform_release >= '24'",
        ],
        [
            "platform_release <= '9'",
            "platform_release >= '24'",
            "platform_release <= '100'",
        ],
    ] {
        let markers = expressions
            .map(str::parse::<MarkerTree>)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        // Each pair has a numeric or lexical witness, but all three constraints cannot hold.
        for [first, second, third] in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let pair = darwin.and(markers[first]).and(markers[second]);
            assert!(!pair.is_false());
            assert!(pair.is_disjoint(markers[third]));
            assert!(pair.and(markers[third]).is_false());
        }
    }
    Ok(())
}

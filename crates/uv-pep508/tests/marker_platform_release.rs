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

        let printed = marker.try_to_string().ok_or("missing marker contents")?;
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
fn darwin_release_restriction_uses_wheel_boundary() -> Result<(), Box<dyn Error>> {
    let required: MarkerTree = "sys_platform == 'darwin' and platform_release == '24'".parse()?;
    let coverage: MarkerTree = "sys_platform == 'darwin' and platform_release >= '25'".parse()?;
    let assumption = required.or(coverage);
    let darwin: MarkerTree = "sys_platform == 'darwin'".parse()?;
    let split = required.restrict(assumption).and(darwin);

    for (release, expected) in [
        ("24", true),
        ("24.1", true),
        ("25", false),
        ("25-invalid", false),
    ] {
        let environment = environment("darwin", release)?;
        assert_roundtrip(split, &environment, expected)?;
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
fn darwin_release_numeric_and_lexical_ordering() -> Result<(), Box<dyn Error>> {
    let marker: MarkerTree = "platform_release >= '24.9'".parse()?;

    for (sys_platform, release, expected) in [
        ("darwin", "24.10", true),
        ("darwin", "24.8", false),
        ("darwin", "25-invalid", true),
        ("darwin", "", false),
        ("linux", "24.10", false),
        ("linux", "25-invalid", true),
    ] {
        let environment = environment(sys_platform, release)?;
        assert_eq!(marker.evaluate(&environment, &[]), expected);
        assert_eq!(marker.negate().evaluate(&environment, &[]), !expected);
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

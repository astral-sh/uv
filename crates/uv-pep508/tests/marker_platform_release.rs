use std::error::Error;

use uv_pep440::VersionParseError;
use uv_pep508::{MarkerEnvironment, MarkerEnvironmentBuilder, MarkerTree};

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

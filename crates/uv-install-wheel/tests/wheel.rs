use anyhow::{Context, Result};
use indoc::indoc;

use uv_install_wheel::WheelFile;

#[test]
fn wheel_header_values() -> Result<()> {
    let source = indoc! {"
        Wheel-Version: 1.0
        Wheel-Version: 2.0
        Tag: =?utf-8?q?_py3-?=
          =?utf-8?q?none-any_?=
        tag: ignored
        Tag: UNKNOWN
        Tag: cp312-none-any
    "};

    let wheel = WheelFile::parse(source)?;
    assert_eq!(
        wheel.tags().context("Expected wheel tags")?,
        ["py3-none-any", "UNKNOWN", "cp312-none-any"]
    );

    let error = WheelFile::parse("wheel-version: 1.0\n")
        .err()
        .context("Expected a missing Wheel-Version error")?;
    assert_eq!(
        error.to_string(),
        "The wheel is invalid: Invalid Wheel-Version in `WHEEL` file: None"
    );

    Ok(())
}

#[test]
fn wheel_header_decoding_errors() -> Result<()> {
    for field in ["Wheel-Version", "Tag", "X-Unused"] {
        for (value, detail) in [
            (
                "=?x-unknown?q?text?=",
                "unsupported header charset: x-unknown",
            ),
            ("=?utf-8?b?A?=", "invalid Base64 in encoded header word"),
        ] {
            let source = format!("Wheel-Version: 1.0\n{field}: {value}\n");
            let error = WheelFile::parse(&source)
                .err()
                .context("Expected a header decoding error")?;
            assert_eq!(
                error.to_string(),
                format!("The wheel is invalid: Failed to decode `WHEEL` file: {detail}")
            );
        }
    }

    Ok(())
}

#[test]
fn malformed_lines_stop_wheel_headers() -> Result<()> {
    for line in ["colonless", "Bad Name: value"] {
        let source = format!("Wheel-Version: 1.0\nTag: py3-none-any\n{line}\nTag: =?utf-8?b?A?=\n");
        let wheel = WheelFile::parse(&source)?;
        assert_eq!(
            wheel.tags().context("Expected wheel tags")?,
            ["py3-none-any"]
        );

        let source = format!("{line}\nWheel-Version: 1.0\n");
        let error = WheelFile::parse(&source)
            .err()
            .context("Expected a missing Wheel-Version error")?;
        assert_eq!(
            error.to_string(),
            "The wheel is invalid: Invalid Wheel-Version in `WHEEL` file: None"
        );
    }

    Ok(())
}

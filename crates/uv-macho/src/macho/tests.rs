use anyhow::Result;

use super::parse;

const ARM64: &[u8] = include_bytes!("../../tests/fixtures/arm64.dylib");
const X86_64: &[u8] = include_bytes!("../../tests/fixtures/x86_64.dylib");
const SIGNED: &[u8] = include_bytes!("../../tests/fixtures/signed-arm64.dylib");

#[test]
fn parse_dylibs() -> Result<()> {
    for image in [ARM64, X86_64, SIGNED] {
        parse(image)?;
    }

    Ok(())
}

#[test]
fn malformed_inputs() {
    let mut failures = Vec::new();

    for (description, offset, value) in [
        ("fat", 0, 0xbeba_feca),
        ("32-bit", 0, 0xfeed_face),
        ("swapped", 0, 0xcffa_edfe),
        ("executable", 12, 2),
        ("command count", 16, u32::MAX),
        ("command size", 36, 0),
        ("command alignment", 36, 9),
        ("short segment", 36, 8),
        ("unknown command", 32, u32::MAX),
        ("section count", 96, u32::MAX),
        ("section file offset", 152, u32::MAX),
        ("segment overlap", 72, 1),
    ] {
        let mut bytes = ARM64.to_vec();
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        let error = parse(&bytes).expect_err("invalid fixture");
        failures.push(format!("{description}: {error}"));
    }

    insta::assert_snapshot!(failures.join("\n"));

    // Every truncated prefix must fail without panicking.
    for end in 0..SIGNED.len() {
        assert!(parse(&SIGNED[..end]).is_err());
    }
}

/// Locate commands in the known-good fixtures, independently of the layout reader.
fn command_offset(image: &[u8], kind: u32) -> Result<usize> {
    let mut offset = 32;
    for _ in 0..super::le32(image, 16)? {
        if super::le32(image, offset)? == kind {
            return Ok(offset);
        }
        offset += super::le32(image, offset + 4)? as usize;
    }

    anyhow::bail!("fixture is missing load command {kind:#x}")
}

#[test]
fn command_boundaries() -> Result<()> {
    let install_id = command_offset(ARM64, 0xd)?;
    let build_version = command_offset(ARM64, 0x32)?;
    let symbols = command_offset(ARM64, 0x2)?;
    let install_id_size = super::le32(ARM64, install_id + 4)?;
    let mut failures = Vec::new();

    for (description, offset, value) in [
        ("string inside command header", install_id + 8, 8),
        ("string outside command", install_id + 8, install_id_size),
        ("tool count", build_version + 20, u32::MAX),
        ("symbol count", symbols + 12, u32::MAX),
        ("symbol offset", symbols + 8, 32),
    ] {
        let mut image = ARM64.to_vec();
        image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        failures.push(format!(
            "{description}: {}",
            parse(&image).expect_err("invalid command")
        ));
    }

    let mut image = ARM64.to_vec();
    image[install_id + 24..install_id + install_id_size as usize].fill(b'x');
    failures.push(format!(
        "unterminated string: {}",
        parse(&image).expect_err("unterminated string")
    ));

    let mut image = ARM64.to_vec();
    // The first section's address plus its nonempty size must not wrap.
    image[136..144].copy_from_slice(&u64::MAX.to_le_bytes());
    failures.push(format!(
        "virtual address overflow: {}",
        parse(&image).expect_err("invalid address")
    ));

    insta::assert_snapshot!(failures.join("\n"));

    Ok(())
}

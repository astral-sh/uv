use anyhow::Result;

use crate::Error;

use super::parse;

const ARM64: &[u8] = include_bytes!("../../tests/fixtures/arm64.dylib");
const X86_64: &[u8] = include_bytes!("../../tests/fixtures/x86_64.dylib");
const SIGNED: &[u8] = include_bytes!("../../tests/fixtures/signed-arm64.dylib");

#[test]
fn parse_dylibs() -> Result<()> {
    for image in [ARM64, X86_64, SIGNED] {
        parse(image)?;
        parse(&reverse_regions(image)?)?;
    }

    Ok(())
}

#[test]
fn malformed_inputs() {
    for (description, offset, value, expected) in [
        (
            "fat",
            0,
            0xbeba_feca,
            Error::Unsupported("expected a thin little-endian 64-bit dylib"),
        ),
        (
            "32-bit",
            0,
            0xfeed_face,
            Error::Unsupported("expected a thin little-endian 64-bit dylib"),
        ),
        (
            "swapped",
            0,
            0xcffa_edfe,
            Error::Unsupported("expected a thin little-endian 64-bit dylib"),
        ),
        (
            "executable",
            12,
            2,
            Error::Unsupported("expected a thin little-endian 64-bit dylib"),
        ),
        (
            "command count",
            16,
            u32::MAX,
            Error::Malformed("invalid load-command count"),
        ),
        (
            "command size",
            36,
            0,
            Error::Malformed("invalid load-command size"),
        ),
        (
            "command alignment",
            36,
            9,
            Error::Malformed("invalid load-command size"),
        ),
        (
            "short segment",
            36,
            8,
            Error::Malformed("range extends past its containing data"),
        ),
        (
            "unknown command",
            32,
            u32::MAX,
            Error::Unsupported("load command"),
        ),
        (
            "section count",
            96,
            u32::MAX,
            Error::Malformed("range extends past its containing data"),
        ),
        (
            "section file offset",
            152,
            u32::MAX,
            Error::Malformed("range extends past its containing data"),
        ),
        (
            "text file offset",
            72,
            1,
            Error::Malformed("invalid or duplicate __TEXT segment"),
        ),
    ] {
        let mut bytes = ARM64.to_vec();
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert_eq!(parse(&bytes).err(), Some(expected), "{description}");
    }

    // Every truncated prefix must fail without panicking.
    for end in 0..SIGNED.len() {
        assert!(parse(&SIGNED[..end]).is_err(), "truncated at {end}");
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

/// Reverse load commands and section headers without moving the data they reference.
fn reverse_regions(image: &[u8]) -> Result<Vec<u8>> {
    let mut commands = Vec::new();
    let mut offset = 32;

    for _ in 0..super::le32(image, 16)? {
        let size = super::le32(image, offset + 4)? as usize;
        let data = &image[offset..offset + size];
        let mut command = data.to_vec();
        if super::le32(data, 0)? == 0x19 {
            let (sections, trailing) = data[72..].as_chunks::<80>();
            assert!(trailing.is_empty());
            for (index, section) in sections.iter().rev().enumerate() {
                command[72 + index * 80..72 + (index + 1) * 80].copy_from_slice(section);
            }
        }
        commands.push(command);
        offset += size;
    }

    let mut output = image[..32].to_vec();
    for command in commands.into_iter().rev() {
        output.extend(command);
    }
    output.extend_from_slice(&image[offset..]);

    Ok(output)
}

#[test]
fn command_boundaries() -> Result<()> {
    let install_id = command_offset(ARM64, 0xd)?;
    let build_version = command_offset(ARM64, 0x32)?;
    let symbols = command_offset(ARM64, 0x2)?;
    let install_id_size = super::le32(ARM64, install_id + 4)?;

    for (description, offset, value, expected) in [
        (
            "string inside command header",
            install_id + 8,
            8,
            Error::Malformed("invalid load-command string offset"),
        ),
        (
            "string outside command",
            install_id + 8,
            install_id_size,
            Error::Malformed("unterminated string"),
        ),
        (
            "tool count",
            build_version + 20,
            u32::MAX,
            Error::Malformed("range extends past its containing data"),
        ),
        (
            "symbol count",
            symbols + 12,
            u32::MAX,
            Error::Malformed("range extends past its containing data"),
        ),
        (
            "symbol offset",
            symbols + 8,
            32,
            Error::Malformed("file data overlaps load commands or the signature"),
        ),
    ] {
        let mut image = ARM64.to_vec();
        image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert_eq!(parse(&image).err(), Some(expected), "{description}");
    }

    let mut image = ARM64.to_vec();
    image[install_id + 24..install_id + install_id_size as usize].fill(b'x');
    assert_eq!(
        parse(&image).err(),
        Some(Error::Malformed("unterminated string"))
    );

    let mut image = ARM64.to_vec();
    // The first section's address plus its nonempty size must not wrap.
    image[136..144].copy_from_slice(&u64::MAX.to_le_bytes());
    assert_eq!(parse(&image).err(), Some(Error::TooLarge));

    Ok(())
}

#[test]
fn overlapping_file_regions() -> Result<()> {
    for fixture in [ARM64, X86_64] {
        let text = command_offset(fixture, 0x19)?;
        let linkedit = text + super::le32(fixture, text + 4)? as usize;
        let first_section = text + 72;
        let first_offset = super::le32(fixture, first_section + 48)?;
        let first_size = super::le32(fixture, first_section + 40)?;
        let second_offset = super::le32(fixture, first_section + 80 + 48)?;

        for (description, offset, value) in [
            (
                "overlapping segments",
                linkedit + 40,
                super::le32(fixture, linkedit + 40)? - 1,
            ),
            (
                "section overlaps previous",
                first_section + 80 + 48,
                first_offset + first_size - 1,
            ),
            (
                "section contains previous",
                first_section + 80 + 48,
                first_offset,
            ),
            (
                "section overlaps next",
                first_section + 40,
                second_offset - first_offset + 1,
            ),
        ] {
            let mut image = fixture.to_vec();
            image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            let reversed = reverse_regions(&image)?;

            for image in [&image, &reversed] {
                assert_eq!(
                    parse(image).err(),
                    Some(Error::Malformed("overlapping file regions")),
                    "{description}"
                );
            }
        }
    }

    Ok(())
}

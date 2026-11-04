use anyhow::{Context, Result};

use uv_macho::{Error, SignedDylib, set_install_name};

use crate::common::{ARM64, SIGNED, X86_64, commands};

fn edit(image: &[u8]) -> Result<SignedDylib, Error> {
    set_install_name(image, c"name", c"fixture")
}

#[test]
fn parse_reversed_regions() -> Result<()> {
    for image in [ARM64, X86_64, SIGNED] {
        edit(&reverse_regions(image)?)?;
    }

    Ok(())
}

#[test]
fn malformed_inputs() {
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
        ("unclaimed section records", 96, 0),
        ("section file offset", 152, u32::MAX),
        ("text file offset", 72, 1),
    ] {
        let mut bytes = ARM64.to_vec();
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(edit(&bytes).is_err(), "{description}");
    }

    // Every truncated prefix must fail without panicking.
    for end in 0..SIGNED.len() {
        assert!(edit(&SIGNED[..end]).is_err(), "truncated at {end}");
    }
}

fn command_offset(image: &[u8], kind: u32) -> Result<usize> {
    commands(image)?
        .into_iter()
        .find(|command| command.kind == kind)
        .map(|command| command.offset)
        .with_context(|| format!("fixture is missing load command {kind:#x}"))
}

/// Reverse load commands and section headers without moving the data they reference.
fn reverse_regions(image: &[u8]) -> Result<Vec<u8>> {
    let commands = commands(image)?;
    let mut output = image[..32].to_vec();
    let end = commands.last().context("last load command")?;
    for command in commands.iter().rev() {
        let offset = output.len();
        output.extend_from_slice(command.data);
        if command.kind == 0x19 {
            output[offset + 72..].as_chunks_mut::<80>().0.reverse();
        }
    }
    output.extend_from_slice(&image[end.offset + end.data.len()..]);
    Ok(output)
}

fn array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N]> {
    Ok(bytes[offset..offset + N].try_into()?)
}

#[test]
fn command_boundaries() -> Result<()> {
    let install_id = command_offset(ARM64, 0xd)?;
    let build_version = command_offset(ARM64, 0x32)?;
    let symbols = command_offset(ARM64, 0x2)?;
    let install_id_size = u32::from_le_bytes(array(ARM64, install_id + 4)?);

    for (description, offset, value) in [
        (
            "command extends past sizeofcmds",
            20,
            u32::from_le_bytes(array(ARM64, 20)?) - 8,
        ),
        ("string inside command header", install_id + 8, 8),
        ("string outside command", install_id + 8, install_id_size),
        ("tool count", build_version + 20, u32::MAX),
        ("symbol count", symbols + 12, u32::MAX),
        ("symbol offset", symbols + 8, 32),
    ] {
        let mut image = ARM64.to_vec();
        image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(edit(&image).is_err(), "{description}");
    }

    let mut image = ARM64.to_vec();
    image[install_id + 24..install_id + install_id_size as usize].fill(b'x');
    assert_eq!(
        edit(&image).err(),
        Some(Error::Malformed("unterminated string"))
    );

    let mut image = ARM64.to_vec();
    // The first section's address plus its nonempty size must not wrap.
    image[136..144].copy_from_slice(&u64::MAX.to_le_bytes());
    assert_eq!(edit(&image).err(), Some(Error::TooLarge));

    // An empty table does not reference any bytes, even with an out-of-bounds offset.
    let mut image = ARM64.to_vec();
    let dynamic_symbols = command_offset(ARM64, 0xb)?;
    assert_eq!(u32::from_le_bytes(array(&image, dynamic_symbols + 36)?), 0);
    image[dynamic_symbols + 32..dynamic_symbols + 36].copy_from_slice(&u32::MAX.to_le_bytes());
    edit(&image)?;

    Ok(())
}

/// A zero-filled gap after a sectionless `__TEXT` cannot hold load commands.
#[test]
fn header_growth_stays_in_text() -> Result<()> {
    let text = command_offset(ARM64, 0x19)?;
    let text_command_size = u32::from_le_bytes(array(ARM64, text + 4)?) as usize;
    let section_headers_size = text_command_size - 72;
    let commands_size = u32::from_le_bytes(array(ARM64, 20)?) as usize;
    let old_end = 32 + commands_size;
    let new_end = old_end - section_headers_size;

    let mut image = ARM64.to_vec();
    image.copy_within(text + text_command_size..old_end, text + 72);
    image[new_end..old_end].fill(0);
    image[20..24].copy_from_slice(&u32::try_from(new_end - 32)?.to_le_bytes());
    image[text + 4..text + 8].copy_from_slice(&72u32.to_le_bytes());
    image[text + 64..text + 68].copy_from_slice(&0u32.to_le_bytes());
    let text_filesize = text + 48..text + 56;
    let command_end = u64::try_from(new_end)?;
    image[text_filesize.clone()].copy_from_slice(&command_end.to_le_bytes());
    let name = c"@rpath/libfixture.dylib";
    assert_eq!(
        set_install_name(&image, name, c"fixture").err(),
        Some(Error::InsufficientHeaderPadding)
    );

    // Exactly 16 more bytes in __TEXT make room for LC_CODE_SIGNATURE.
    image[text_filesize].copy_from_slice(&(command_end + 16).to_le_bytes());
    let output = set_install_name(&image, name, c"fixture")?;
    assert_eq!(
        set_install_name(output.as_bytes(), name, c"fixture")?,
        output
    );
    Ok(())
}

/// Signing an empty `__LINKEDIT` at `__TEXT`'s address would make them overlap.
#[test]
fn empty_linkedit_overlaps_when_signed() -> Result<()> {
    let text = command_offset(ARM64, 0x19)?;
    let linkedit = text + u32::from_le_bytes(array(ARM64, text + 4)?) as usize;
    let file_end = u64::try_from(ARM64.len())?;
    let mut image = ARM64.to_vec();
    image.copy_within(text + 24..text + 32, linkedit + 24);
    image[linkedit + 32..linkedit + 40].fill(0);
    image[linkedit + 40..linkedit + 48].copy_from_slice(&file_end.to_le_bytes());
    image[linkedit + 48..linkedit + 56].fill(0);

    assert_eq!(
        edit(&image).err(),
        Some(Error::Malformed("invalid or duplicate __LINKEDIT segment"))
    );
    Ok(())
}

#[test]
fn overlapping_file_regions() -> Result<()> {
    for fixture in [ARM64, X86_64] {
        let text = command_offset(fixture, 0x19)?;
        let linkedit = text + u32::from_le_bytes(array(fixture, text + 4)?) as usize;
        let first_section = text + 72;
        let first_offset = u32::from_le_bytes(array(fixture, first_section + 48)?);
        let first_size = u32::from_le_bytes(array(fixture, first_section + 40)?);
        let second_offset = u32::from_le_bytes(array(fixture, first_section + 80 + 48)?);

        for (description, offset, value) in [
            (
                "overlapping segments",
                linkedit + 40,
                u32::from_le_bytes(array(fixture, linkedit + 40)?) - 1,
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
                    edit(image).err(),
                    Some(Error::Malformed("overlapping file regions")),
                    "{description}"
                );
            }
        }
    }

    Ok(())
}

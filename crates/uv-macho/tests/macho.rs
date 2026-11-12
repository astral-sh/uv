mod common;
mod layout;

use std::ffi::{CStr, CString};
#[cfg(target_os = "macos")]
use std::process::Command;

use anyhow::{Context, Result};
#[cfg(target_os = "macos")]
use assert_cmd::assert::OutputAssertExt;
use sha2::{Digest, Sha256};

use common::{ARM64, SIGNED, X86_64, commands, install_name, name_capacity, sections};

use uv_macho::{Error, set_install_name};

fn code_directory(image: &[u8]) -> Result<(usize, &[u8])> {
    let command = commands(image)?
        .into_iter()
        .find(|command| command.kind == 0x1d)
        .context("LC_CODE_SIGNATURE")?;
    let offset = u32::from_le_bytes(command.data[8..12].try_into()?) as usize;
    let size = u32::from_le_bytes(command.data[12..16].try_into()?) as usize;
    let data = image.get(offset..offset + size).context("signature data")?;

    assert_eq!(u32::from_be_bytes(data[..4].try_into()?), 0xfade_0cc0);
    assert_eq!(u32::from_be_bytes(data[8..12].try_into()?), 1);
    assert_eq!(u32::from_be_bytes(data[12..16].try_into()?), 0);
    let directory_offset = u32::from_be_bytes(data[16..20].try_into()?) as usize;
    let directory = data.get(directory_offset..).context("CodeDirectory")?;
    assert_eq!(u32::from_be_bytes(directory[..4].try_into()?), 0xfade_0c02);
    let length = u32::from_be_bytes(directory[4..8].try_into()?) as usize;
    Ok((
        offset,
        directory.get(..length).context("CodeDirectory length")?,
    ))
}

fn verify_hashes(image: &[u8]) -> Result<()> {
    let (limit, directory) = code_directory(image)?;
    assert_eq!(directory[36], 32);
    assert_eq!(directory[37], 2);

    let offset = u32::from_be_bytes(directory[16..20].try_into()?) as usize;
    let count = u32::from_be_bytes(directory[28..32].try_into()?) as usize;
    assert_eq!(
        u32::from_be_bytes(directory[32..36].try_into()?) as usize,
        limit
    );
    assert_eq!(count, limit.div_ceil(4096));
    assert_eq!(directory.len(), offset + count * 32);

    for (index, page) in image[..limit].chunks(4096).enumerate() {
        assert_eq!(
            &directory[offset + index * 32..][..32],
            Sha256::digest(page).as_slice()
        );
    }

    Ok(())
}

#[test]
fn edit_and_sign_arm64() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    // A large identifier forces __LINKEDIT to grow by multiple virtual pages.
    let identifier = CString::new(vec![b'x'; 25000])?;
    let path = temporary.path().join("arm64.dylib");
    let name = CString::new(path.as_os_str().as_encoded_bytes())?;
    let output = set_install_name(ARM64, &name, &identifier)?;
    assert_eq!(install_name(output.as_bytes())?, name.as_bytes());
    assert_eq!(sections(output.as_bytes())?, sections(ARM64)?);
    verify_hashes(output.as_bytes())?;
    assert_eq!(
        set_install_name(output.as_bytes(), &name, c"ignored",)?,
        output
    );

    #[cfg(target_os = "macos")]
    {
        fs_err::write(&path, output.as_bytes())?;
        Command::new("/usr/bin/otool")
            .arg("-D")
            .arg(&path)
            .assert()
            .success()
            .stdout(format!("{}:\n{}\n", path.display(), path.display()));
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict"])
            .arg(&path)
            .assert()
            .success();
    }

    Ok(())
}

#[test]
fn edit_and_sign_x86_64() -> Result<()> {
    let identifier = CString::new(vec![b'x'; 25000])?;
    let name = c"/a/longer/install/directory/libfixture.dylib";
    let output = set_install_name(X86_64, name, &identifier)?;
    assert_eq!(install_name(output.as_bytes())?, name.to_bytes());
    assert_eq!(sections(output.as_bytes())?, sections(X86_64)?);
    verify_hashes(output.as_bytes())?;

    Ok(())
}

#[test]
fn short_install_name() -> Result<()> {
    let output = set_install_name(ARM64, c"x", c"fixture")?;
    assert_eq!(install_name(output.as_bytes())?, b"x");
    assert_eq!(sections(output.as_bytes())?, sections(ARM64)?);
    Ok(())
}

#[test]
fn non_utf8_install_name() -> Result<()> {
    let name = c"/non-utf8-\xff/libfixture.dylib";
    let output = set_install_name(ARM64, name, c"fixture")?;
    assert_eq!(install_name(output.as_bytes())?, name.to_bytes());
    Ok(())
}

#[test]
fn empty_signature_command() -> Result<()> {
    let mut image = SIGNED.to_vec();
    let offset = commands(&image)?
        .into_iter()
        .find(|command| command.kind == 0x1d)
        .context("LC_CODE_SIGNATURE")?
        .offset;
    image[offset + 8..offset + 12].copy_from_slice(&u32::MAX.to_le_bytes());
    image[offset + 12..offset + 16].copy_from_slice(&0u32.to_le_bytes());

    let output = set_install_name(&image, c"fixture.dylib", c"fixture")?;
    verify_hashes(output.as_bytes())?;
    assert_eq!(
        commands(output.as_bytes())?
            .iter()
            .filter(|command| command.kind == 0x1d)
            .count(),
        1
    );
    Ok(())
}

#[test]
fn resign_linker_signature() -> Result<()> {
    let output = set_install_name(SIGNED, c"fixture.dylib", c"ignored")?;
    verify_hashes(output.as_bytes())?;
    let (_, directory) = code_directory(output.as_bytes())?;
    let offset = u32::from_be_bytes(directory[20..24].try_into()?) as usize;
    assert_eq!(
        CStr::from_bytes_until_nul(&directory[offset..])?,
        c"libfixture.dylib"
    );
    Ok(())
}

#[test]
fn malformed_signatures() -> Result<()> {
    let (signature, directory) = code_directory(SIGNED)?;
    let directory_offset =
        signature + u32::from_be_bytes(SIGNED[signature + 16..signature + 20].try_into()?) as usize;
    for (description, offset, value) in [
        ("blob count", signature + 8, u32::MAX),
        ("truncated blob index", signature + 4, 16),
        ("directory overlaps index", signature + 16, 12),
        (
            "directory length",
            directory_offset + 4,
            u32::try_from(directory.len())? - 1,
        ),
        ("identifier offset", directory_offset + 20, 0),
        ("hash offset", directory_offset + 16, u32::MAX),
        ("page count", directory_offset + 28, u32::MAX),
        ("code limit", directory_offset + 32, 0),
    ] {
        let mut image = SIGNED.to_vec();
        image[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        assert!(
            set_install_name(&image, c"name", c"fixture").is_err(),
            "{description}"
        );
    }

    let linkedit = commands(SIGNED)?
        .into_iter()
        .find(|command| command.kind == 0x19 && command.data[8..24].starts_with(b"__LINKEDIT\0"))
        .context("__LINKEDIT")?;

    let mut trailing = SIGNED.to_vec();
    let filesize =
        u64::from_le_bytes(trailing[linkedit.offset + 48..linkedit.offset + 56].try_into()?) + 1;
    trailing[linkedit.offset + 48..linkedit.offset + 56].copy_from_slice(&filesize.to_le_bytes());
    trailing.push(1);
    assert_eq!(
        set_install_name(&trailing, c"name", c"fixture"),
        Err(Error::Unsupported(
            "code signature is not at the end of __LINKEDIT"
        ))
    );

    Ok(())
}

#[test]
fn unsupported_signature_metadata() -> Result<()> {
    let (signature, _) = code_directory(SIGNED)?;
    let directory =
        signature + u32::from_be_bytes(SIGNED[signature + 16..signature + 20].try_into()?) as usize;
    for (description, offset, value) in [
        ("version", directory + 8, 0x20500u32),
        ("runtime flags", directory + 12, 0x10002),
        ("special slots", directory + 24, 1),
        ("SHA-1 hashes", directory + 36, 0x1401_000c),
    ] {
        let mut image = SIGNED.to_vec();
        image[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        assert_eq!(
            set_install_name(&image, c"name", c"fixture"),
            Err(Error::UnsupportedSignature),
            "{description}"
        );
    }
    Ok(())
}

#[test]
fn non_utf8_identifier() -> Result<()> {
    let identifier = c"fixture-\xff";
    let output = set_install_name(ARM64, c"fixture.dylib", identifier)?;

    let (_, directory) = code_directory(output.as_bytes())?;
    let offset = u32::from_be_bytes(directory[20..24].try_into()?) as usize;
    assert_eq!(
        CStr::from_bytes_until_nul(&directory[offset..])?,
        identifier
    );

    Ok(())
}

#[test]
fn header_padding() -> Result<()> {
    let available = name_capacity(ARM64)? - 16;
    let name = CString::new(vec![b'x'; available - 25])?;
    let output = set_install_name(ARM64, &name, c"fixture")?;

    verify_hashes(output.as_bytes())?;

    let too_long = CString::new(vec![b'x'; name.as_bytes().len() + 1])?;
    assert_eq!(
        set_install_name(ARM64, &too_long, c"fixture"),
        Err(Error::InsufficientHeaderPadding)
    );

    Ok(())
}

#[test]
fn empty_install_name() {
    assert_eq!(
        set_install_name(ARM64, c"", c"fixture"),
        Err(Error::InvalidName)
    );
}

#[test]
fn empty_signing_identifier() {
    assert_eq!(
        set_install_name(ARM64, c"name", c""),
        Err(Error::InvalidIdentifier)
    );
}

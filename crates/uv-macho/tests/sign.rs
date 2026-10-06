mod common;

use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
#[cfg(target_os = "macos")]
use std::process::Command;

use anyhow::{Context, Result};
#[cfg(target_os = "macos")]
use assert_cmd::assert::OutputAssertExt;
use sha2::{Digest, Sha256};

use common::{commands, install_name, name_capacity, sections};

use uv_macho::{
    Error, InstallName, SigningIdentifier, adhoc_sign, replace_install_name, set_install_name,
};

const ARM64: &[u8] = include_bytes!("fixtures/arm64.dylib");
const X86_64: &[u8] = include_bytes!("fixtures/x86_64.dylib");
const SIGNED: &[u8] = include_bytes!("fixtures/signed-arm64.dylib");

fn signature(image: &[u8]) -> Result<(usize, BTreeMap<u32, &[u8]>)> {
    let command = commands(image)?
        .into_iter()
        .find(|command| command.kind == 0x1d)
        .context("LC_CODE_SIGNATURE")?;
    let offset = u32::from_le_bytes(command.data[8..12].try_into()?) as usize;
    let size = u32::from_le_bytes(command.data[12..16].try_into()?) as usize;
    let data = image.get(offset..offset + size).context("signature data")?;
    assert_eq!(u32::from_be_bytes(data[..4].try_into()?), 0xfade_0cc0);

    let count = u32::from_be_bytes(data[8..12].try_into()?) as usize;
    let mut blobs = BTreeMap::new();

    for index in 0..count {
        let entry = &data[12 + index * 8..20 + index * 8];
        let slot = u32::from_be_bytes(entry[..4].try_into()?);
        let offset = u32::from_be_bytes(entry[4..8].try_into()?) as usize;
        let length = u32::from_be_bytes(data[offset + 4..offset + 8].try_into()?) as usize;
        blobs.insert(slot, &data[offset..offset + length]);
    }

    Ok((offset, blobs))
}

fn verify_hashes(image: &[u8]) -> Result<()> {
    let (limit, blobs) = signature(image)?;
    let info_plist = sections(image)?
        .into_iter()
        .find(|section| section.name == b"__info_plist\0\0\0\0")
        .map(|section| section.data);

    assert!(!blobs.keys().any(|slot| (0x1000..=0x1004).contains(slot)));
    let directory = blobs.get(&0).context("primary CodeDirectory")?;
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

    let specials = u32::from_be_bytes(directory[24..28].try_into()?) as usize;

    for special in 1..=specials {
        let actual = &directory[offset - special * 32..][..32];
        if let Some(data) = blobs.get(&u32::try_from(special)?) {
            assert_eq!(actual, Sha256::digest(data).as_slice());
        } else if special == 1
            && let Some(info_plist) = info_plist
        {
            assert_eq!(actual, Sha256::digest(info_plist).as_slice());
        } else {
            assert_eq!(actual, [0; 32]);
        }
    }

    Ok(())
}

#[test]
fn edit_and_sign() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    // A large identifier forces __LINKEDIT to grow by multiple virtual pages.
    let identifier = CString::new(vec![b'x'; 25000])?;
    let mut summary = Vec::new();

    for (architecture, fixture) in [("arm64", ARM64), ("x86_64", X86_64), ("signed", SIGNED)] {
        let path = temporary.path().join(format!("{architecture}.dylib"));
        let name = CString::new(path.as_os_str().as_encoded_bytes())?;
        let output = set_install_name(
            fixture,
            InstallName::new(&name)?,
            SigningIdentifier::new(&identifier)?,
        )?;
        assert_eq!(install_name(output.as_bytes())?, name.as_bytes());
        assert_eq!(sections(output.as_bytes())?, sections(fixture)?);
        verify_hashes(output.as_bytes())?;
        assert_eq!(
            replace_install_name(fixture, InstallName::new(&name)?)?
                .adhoc_sign(SigningIdentifier::new(&identifier)?)?,
            output
        );
        assert_eq!(
            adhoc_sign(output.as_bytes(), SigningIdentifier::new(c"ignored")?)?,
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

        let (_, blobs) = signature(output.as_bytes())?;
        summary.push(format!(
            "{architecture}: {:?}",
            blobs.keys().collect::<Vec<_>>()
        ));
    }

    insta::assert_snapshot!(summary.join("\n"), @r"
    arm64: [0, 2, 65536]
    x86_64: [0, 2, 65536]
    signed: [0, 2, 5, 7, 65536]
    ");

    Ok(())
}

#[test]
fn preserve_metadata() -> Result<()> {
    let output = adhoc_sign(SIGNED, SigningIdentifier::new(c"ignored")?)?;
    let (_, before) = signature(SIGNED)?;
    let (_, after) = signature(output.as_bytes())?;

    for slot in [2, 5, 7] {
        assert_eq!(
            before.get(&slot).context("original metadata")?,
            after.get(&slot).context("preserved metadata")?
        );
    }

    let original = before.get(&0).context("original directory")?;
    let directory = after.get(&0).context("new directory")?;
    assert_eq!(
        u32::from_be_bytes(directory[12..16].try_into()?),
        u32::from_be_bytes(original[12..16].try_into()?)
    );
    assert_eq!(
        u32::from_be_bytes(directory[88..92].try_into()?),
        u32::from_be_bytes(original[88..92].try_into()?)
    );

    let identifier_offset = u32::from_be_bytes(directory[20..24].try_into()?) as usize;
    assert_eq!(
        CStr::from_bytes_until_nul(&directory[identifier_offset..])?,
        c"org.astral.uv.fixture"
    );

    Ok(())
}

#[test]
fn malformed_signatures() -> Result<()> {
    let (signature_offset, _) = signature(SIGNED)?;
    let directory_offset = signature_offset
        + u32::from_be_bytes(SIGNED[signature_offset + 16..signature_offset + 20].try_into()?)
            as usize;
    for (description, offset, value, expected) in [
        (
            "blob count",
            signature_offset + 8,
            u32::MAX,
            Error::Malformed("range extends past its containing data"),
        ),
        (
            "truncated blob index",
            signature_offset + 4,
            16,
            Error::Malformed("range extends past its containing data"),
        ),
        (
            "blob offset",
            signature_offset + 16,
            0,
            Error::Malformed("invalid signature blob range"),
        ),
        (
            "duplicate slot",
            signature_offset + 20,
            0,
            Error::Malformed("duplicate signature slot"),
        ),
        (
            "unsupported special slot",
            signature_offset + 20,
            8,
            Error::UnsupportedValue {
                field: "unavailable or unsupported CodeDirectory special slot",
                value: 2,
            },
        ),
        (
            "short blob header",
            directory_offset + 4,
            4,
            Error::Malformed("invalid signature blob range"),
        ),
        (
            "short CodeDirectory",
            directory_offset + 4,
            88,
            Error::Malformed("range extends past its containing data"),
        ),
        (
            "truncated code hashes",
            directory_offset + 4,
            u32::from_be_bytes(SIGNED[directory_offset + 4..directory_offset + 8].try_into()?) - 1,
            Error::Malformed("range extends past its containing data"),
        ),
        (
            "version",
            directory_offset + 8,
            u32::MAX,
            Error::UnsupportedValue {
                field: "CodeDirectory version",
                value: u64::from(u32::MAX),
            },
        ),
        (
            "flags",
            directory_offset + 12,
            u32::MAX,
            Error::UnsupportedValue {
                field: "CodeDirectory flags",
                value: u64::from(u32::MAX),
            },
        ),
        (
            "identifier",
            directory_offset + 20,
            0,
            Error::Malformed("invalid signing identifier offset"),
        ),
        (
            "special slots",
            directory_offset + 24,
            u32::MAX,
            Error::UnsupportedValue {
                field: "CodeDirectory special-slot count",
                value: u64::from(u32::MAX),
            },
        ),
        (
            "page count",
            directory_offset + 28,
            u32::MAX,
            Error::Malformed("range extends past its containing data"),
        ),
        (
            "code limit",
            directory_offset + 32,
            0,
            Error::Unsupported("CodeDirectory does not cover the complete image"),
        ),
        (
            "scatter",
            directory_offset + 44,
            96,
            Error::Unsupported("scatter, pre-encryption, linkage, or platform signature"),
        ),
    ] {
        let mut image = SIGNED.to_vec();
        image[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        assert_eq!(
            set_install_name(
                &image,
                InstallName::new(c"name")?,
                SigningIdentifier::new(c"fixture")?
            ),
            Err(expected),
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
        set_install_name(
            &trailing,
            InstallName::new(c"name")?,
            SigningIdentifier::new(c"fixture")?
        ),
        Err(Error::Unsupported(
            "code signature is not at the end of __LINKEDIT"
        ))
    );

    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn resign_legacy_hashes() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("legacy.dylib");
    fs_err::write(&path, X86_64)?;

    Command::new("/usr/bin/codesign")
        .args([
            "--force",
            "--sign",
            "-",
            "--digest-algorithm",
            "sha1,sha256",
            "--identifier",
            "org.astral.uv.fixture",
        ])
        .arg(&path)
        .assert()
        .success();

    let image = fs_err::read(&path)?;
    let (_, before) = signature(&image)?;
    assert_eq!(before.get(&0).context("primary CodeDirectory")?[37], 1);
    assert_eq!(
        before.get(&0x1000).context("alternate CodeDirectory")?[37],
        2
    );

    let output = adhoc_sign(&image, SigningIdentifier::new(c"ignored")?)?;
    verify_hashes(output.as_bytes())?;

    let (_, after) = signature(output.as_bytes())?;
    assert_eq!(before.get(&2), after.get(&2));
    let directory = after.get(&0).context("new CodeDirectory")?;
    let identifier_offset = u32::from_be_bytes(directory[20..24].try_into()?) as usize;
    assert_eq!(
        CStr::from_bytes_until_nul(&directory[identifier_offset..])?,
        c"org.astral.uv.fixture"
    );

    fs_err::write(&path, output.as_bytes())?;
    Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict"])
        .arg(&path)
        .assert()
        .success();

    Ok(())
}

#[test]
fn non_utf8_identifier() -> Result<()> {
    let identifier = c"fixture-\xff";
    let output = adhoc_sign(ARM64, SigningIdentifier::new(identifier)?)?;

    let (_, blobs) = signature(output.as_bytes())?;
    let directory = blobs.get(&0).context("primary CodeDirectory")?;
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
    let output = set_install_name(
        ARM64,
        InstallName::new(&name)?,
        SigningIdentifier::new(c"fixture")?,
    )?;

    verify_hashes(output.as_bytes())?;

    let too_long = CString::new(vec![b'x'; name.as_bytes().len() + 1])?;
    assert_eq!(
        set_install_name(
            ARM64,
            InstallName::new(&too_long)?,
            SigningIdentifier::new(c"fixture")?
        ),
        Err(Error::InsufficientHeaderPadding)
    );

    Ok(())
}

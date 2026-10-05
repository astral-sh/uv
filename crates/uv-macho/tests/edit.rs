mod common;

use anyhow::Result;

use common::{install_name, name_capacity, sections};

use uv_macho::{Error, InstallName, replace_install_name};

const ARM64: &[u8] = include_bytes!("fixtures/arm64.dylib");
const X86_64: &[u8] = include_bytes!("fixtures/x86_64.dylib");
const SIGNED: &[u8] = include_bytes!("fixtures/signed-arm64.dylib");

#[test]
fn edit_install_name() -> Result<()> {
    for image in [ARM64, X86_64, SIGNED] {
        for name in [
            b"x".as_slice(),
            b"/a/longer/install/directory/libfixture.dylib",
        ] {
            let output = replace_install_name(image, &InstallName::new(name)?)?;
            assert_eq!(install_name(output.as_bytes())?, name);
            assert_eq!(
                replace_install_name(output.as_bytes(), &InstallName::new(name)?)?,
                output
            );

            assert_eq!(sections(output.as_bytes())?, sections(image)?);
        }
    }

    Ok(())
}

#[test]
fn invalid_names() {
    for name in [b"".as_slice(), b"invalid\0name"] {
        assert_eq!(InstallName::new(name), Err(Error::InvalidName));
    }
}

#[test]
fn non_utf8_name() -> Result<()> {
    let name = b"/non-utf8-\xff/libfixture.dylib";
    let output = replace_install_name(ARM64, &InstallName::new(name)?)?;
    assert_eq!(install_name(output.as_bytes())?, name);
    assert_eq!(
        replace_install_name(output.as_bytes(), &InstallName::new(name)?)?,
        output
    );

    Ok(())
}

#[test]
fn header_padding() -> Result<()> {
    let available = name_capacity(ARM64)?;
    let name = vec![b'x'; available - 25];
    let output = replace_install_name(ARM64, &InstallName::new(&name)?)?;

    assert_eq!(install_name(output.as_bytes())?, name);
    assert_eq!(sections(output.as_bytes())?, sections(ARM64)?);

    let too_long = vec![b'x'; name.len() + 1];
    assert_eq!(
        replace_install_name(ARM64, &InstallName::new(&too_long)?),
        Err(Error::InsufficientHeaderPadding)
    );

    Ok(())
}

mod common;

use std::ffi::CString;

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
            c"x",
            c"/a/longer/install/directory/libfixture.dylib",
            c"/non-utf8-\xff/libfixture.dylib",
        ] {
            let output = replace_install_name(image, InstallName::new(name)?)?;
            assert_eq!(install_name(output.as_bytes())?, name.to_bytes());
            assert_eq!(
                replace_install_name(output.as_bytes(), InstallName::new(name)?)?,
                output
            );

            assert_eq!(sections(output.as_bytes())?, sections(image)?);
        }
    }

    Ok(())
}

#[test]
fn header_padding() -> Result<()> {
    let available = name_capacity(ARM64)?;
    let name = CString::new(vec![b'x'; available - 25])?;
    let output = replace_install_name(ARM64, InstallName::new(&name)?)?;

    assert_eq!(install_name(output.as_bytes())?, name.as_bytes());
    assert_eq!(sections(output.as_bytes())?, sections(ARM64)?);

    let too_long = CString::new(vec![b'x'; name.as_bytes().len() + 1])?;
    assert_eq!(
        replace_install_name(ARM64, InstallName::new(&too_long)?),
        Err(Error::InsufficientHeaderPadding)
    );

    Ok(())
}

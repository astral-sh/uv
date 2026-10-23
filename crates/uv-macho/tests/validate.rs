use anyhow::Result;

use uv_macho::validate;

#[test]
fn validate_dylibs() -> Result<()> {
    for image in [
        include_bytes!("fixtures/arm64.dylib").as_slice(),
        include_bytes!("fixtures/x86_64.dylib").as_slice(),
        include_bytes!("fixtures/signed-arm64.dylib").as_slice(),
    ] {
        validate(image)?;
    }
    Ok(())
}

use std::ffi::CString;

use assert_cmd::assert::OutputAssertExt;

use uv_macho::{InstallName, SigningIdentifier, set_install_name};
use uv_test::uv_snapshot;

#[test]
fn macho_dylib_loading() -> anyhow::Result<()> {
    let context = uv_test::test_context_with_versions!(&[]).with_managed_python_dirs();
    context
        .venv()
        .args(["--python", "3.13.1", "--managed-python"])
        .assert()
        .success();

    #[cfg(target_arch = "aarch64")]
    let fixture = include_bytes!("../../../uv-macho/tests/fixtures/arm64.dylib");
    #[cfg(target_arch = "x86_64")]
    let fixture = include_bytes!("../../../uv-macho/tests/fixtures/x86_64.dylib");

    // A large identifier forces __LINKEDIT to grow by multiple virtual pages.
    let identifier = CString::new(vec![b'x'; 25000])?;
    let identifier = SigningIdentifier::new(&identifier)?;

    let path = context.temp_dir.join("fixture.dylib");
    let output = set_install_name(
        fixture,
        &InstallName::new(path.as_os_str().as_encoded_bytes())?,
        identifier,
    )?;
    fs_err::write(&path, output.as_bytes())?;

    uv_snapshot!(context.filters(), context.python_command()
        .args([
            "-I",
            "-c",
            "import ctypes, sys; print(ctypes.CDLL(sys.argv[1]).uv_macho_fixture())",
        ])
        .arg(&path), @"
    exit_code: 0 (success)
    ----- stdout -----
    42
    ");

    Ok(())
}

#[cfg(all(
    any(target_os = "linux", target_os = "macos"),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
use std::os::unix::fs::PermissionsExt;

use anyhow::Result;
use assert_fs::prelude::*;
#[cfg(unix)]
use fs_err::os::unix::fs::symlink;

use uv_test::packse::WheelBuilder;
use uv_test::uv_snapshot;

/// A directory without Python sources does not require a cached compiler.
#[test]
fn native_bytecode_empty() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let (filename, wheel) = WheelBuilder::new(&"example".parse()?, &"1.0.0".parse()?).build();
    let wheel_path = context.temp_dir.join(filename);
    fs_err::write(&wheel_path, wheel)?;
    context
        .temp_dir
        .child("requirements.txt")
        .write_str(&wheel_path.display().to_string())?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + example==1.0.0 (from file://[TEMP_DIR]/example-1.0.0-py3-none-any.whl)
    ");

    fs_err::remove_file(context.site_packages().join("example/__init__.py"))?;
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--offline")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Bytecode compiled 0 files in [TIME]
    ");
    Ok(())
}

/// Serc reports an unsupported Python version before attempting a download.
#[test]
fn native_bytecode_unsupported_python() -> Result<()> {
    let context = uv_test::test_context!("3.11");
    let (filename, wheel) = WheelBuilder::new(&"example".parse()?, &"1.0.0".parse()?).build();
    let wheel_path = context.temp_dir.join(filename);
    fs_err::write(&wheel_path, wheel)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg(&wheel_path)
        .arg("--offline")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode"), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
    error: Failed to bytecode-compile installed packages
      cause: unsupported Python version "3.11"; expected 3.12, 3.13, 3.14, or 3.15
    "#);
    Ok(())
}

/// Serc rejects interpreter settings its command line cannot honor.
#[test]
fn native_bytecode_unsupported_settings() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let (filename, wheel) = WheelBuilder::new(&"example".parse()?, &"1.0.0".parse()?).build();
    let wheel_path = context.temp_dir.join(filename);
    fs_err::write(&wheel_path, wheel)?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg(&wheel_path)
        .arg("--offline")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode")
        .env("PYTHONPYCACHEPREFIX", context.temp_dir.join("bytecode-cache")), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
    error: Failed to bytecode-compile installed packages
      cause: serc does not support a custom bytecode cache prefix (PYTHONPYCACHEPREFIX)
    ");
    Ok(())
}

/// Refuse to replace a symlink at the bytecode destination, as CPython does.
#[cfg(unix)]
#[test]
fn native_bytecode_symlink_destination() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_env("PYTHONOPTIMIZE", "0")
        .with_env("PYC_INVALIDATION_MODE", "TIMESTAMP")
        .with_env("PYTHONPYCACHEPREFIX", "")
        .with_env("PYTHONNODEBUGRANGES", "");
    let (filename, wheel) = WheelBuilder::new(&"example".parse()?, &"1.0.0".parse()?).build();
    let wheel_path = context.temp_dir.join(filename);
    fs_err::write(&wheel_path, wheel)?;
    context
        .temp_dir
        .child("requirements.txt")
        .write_str(&wheel_path.display().to_string())?;
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + example==1.0.0 (from file://[TEMP_DIR]/example-1.0.0-py3-none-any.whl)
    ");

    let cache_dir = context.site_packages().join("example/__pycache__");
    fs_err::create_dir_all(&cache_dir)?;
    let target = context.temp_dir.child("target.pyc");
    target.write_str("untouched")?;
    let destination = cache_dir.join("__init__.cpython-312.pyc");
    symlink(target.path(), &destination)?;

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--offline")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: Failed to bytecode-compile Python file in: [SITE_PACKAGES]/
      cause: Bytecode destination is not a regular file: [SITE_PACKAGES]/example/__pycache__/__init__.cpython-312.pyc
    ");
    assert!(
        fs_err::symlink_metadata(destination)?
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs_err::read_to_string(target.path())?, "untouched");
    Ok(())
}

/// A cached compiler can compile files offline and receives the target Python settings.
#[cfg(all(
    any(target_os = "linux", target_os = "macos"),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
fn native_bytecode_cached_compiler() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_env("PYTHONOPTIMIZE", "0")
        .with_env("PYC_INVALIDATION_MODE", "TIMESTAMP")
        .with_env("PYTHONPYCACHEPREFIX", "")
        .with_env("PYTHONNODEBUGRANGES", "");
    let target = if cfg!(target_os = "macos") {
        format!("{}-apple-darwin", std::env::consts::ARCH)
    } else if cfg!(target_env = "musl") {
        format!("{}-unknown-linux-musl", std::env::consts::ARCH)
    } else {
        format!("{}-unknown-linux-gnu", std::env::consts::ARCH)
    };
    let compiler_dir = context
        .cache_dir
        .join("binaries-v0/serc/0.1.6")
        .join(target);
    fs_err::create_dir_all(&compiler_dir)?;
    let compiler = compiler_dir.join("serc");
    fs_err::write(
        &compiler,
        r#"#!/bin/sh
exec "$UV_TEST_SERC_PYTHON" - "$@" <<'PY'
import py_compile
import sys

args = sys.argv[1:]
assert args[:5] == ["compile", "--python-version", "3.12", "--invalidation-mode", "timestamp"]
assert args[5] == "--jobs" and int(args[6]) > 0
assert args[7:9] == ["--quiet", "--"]
for path in args[9:]:
    py_compile.compile(path, doraise=True, invalidation_mode=py_compile.PycInvalidationMode.TIMESTAMP)
PY
"#,
    )?;
    fs_err::set_permissions(&compiler, std::fs::Permissions::from_mode(0o755))?;

    let (filename, wheel) = WheelBuilder::new(&"example".parse()?, &"1.0.0".parse()?)
        .files(&[("example/module.py", "value = 42\n")])
        .build();
    let wheel_path = context.temp_dir.join(filename);
    fs_err::write(&wheel_path, wheel)?;
    context
        .temp_dir
        .child("requirements.txt")
        .write_str(&wheel_path.display().to_string())?;

    uv_snapshot!(context.filters(), context.pip_install()
        .arg(&wheel_path)
        .arg("--offline")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode")
        .env("UV_TEST_SERC_PYTHON", context.interpreter()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
    Bytecode compiled 2 files in [TIME]
     + example==1.0.0 (from file://[TEMP_DIR]/example-1.0.0-py3-none-any.whl)
    ");

    // Keep the source size and timestamp unchanged so Python must use the cached bytecode.
    let source = context.site_packages().join("example/module.py");
    let modified = fs_err::metadata(&source)?.modified()?;
    fs_err::write(&source, "value = 43\n")?;
    fs_err::File::options()
        .write(true)
        .open(&source)?
        .set_modified(modified)?;
    uv_snapshot!(context.python_command()
        .arg("-c").arg("from example.module import value; print(value)"), @"
    exit_code: 0 (success)
    ----- stdout -----
    42
    ");

    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--offline")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode")
        .env("UV_TEST_SERC_PYTHON", context.interpreter()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Bytecode compiled 2 files in [TIME]
    ");
    uv_snapshot!(context.python_command()
        .arg("-c").arg("from example.module import value; print(value)"), @"
    exit_code: 0 (success)
    ----- stdout -----
    43
    ");
    Ok(())
}

/// A failing compiler subprocess reports its exit status and stderr.
#[cfg(all(
    any(target_os = "linux", target_os = "macos"),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
fn native_bytecode_subprocess_failure() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_env("PYTHONOPTIMIZE", "0")
        .with_env("PYC_INVALIDATION_MODE", "TIMESTAMP")
        .with_env("PYTHONPYCACHEPREFIX", "")
        .with_env("PYTHONNODEBUGRANGES", "");
    let target = if cfg!(target_os = "macos") {
        format!("{}-apple-darwin", std::env::consts::ARCH)
    } else if cfg!(target_env = "musl") {
        format!("{}-unknown-linux-musl", std::env::consts::ARCH)
    } else {
        format!("{}-unknown-linux-gnu", std::env::consts::ARCH)
    };
    let compiler_dir = context
        .cache_dir
        .join("binaries-v0/serc/0.1.6")
        .join(target);
    fs_err::create_dir_all(&compiler_dir)?;
    let compiler = compiler_dir.join("serc");
    fs_err::write(
        &compiler,
        "#!/bin/sh\nprintf 'fixture failure\\n' >&2\nexit 7\n",
    )?;
    fs_err::set_permissions(&compiler, std::fs::Permissions::from_mode(0o755))?;

    let (filename, wheel) = WheelBuilder::new(&"example".parse()?, &"1.0.0".parse()?).build();
    let wheel_path = context.temp_dir.join(filename);
    fs_err::write(&wheel_path, wheel)?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(&wheel_path)
        .arg("--offline")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
    error: Failed to bytecode-compile installed packages
      cause: serc exited with exit status: 7:
             fixture failure
    ");
    Ok(())
}

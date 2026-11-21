use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use walkdir::WalkDir;

use uv_test::packse::WheelBuilder;
use uv_test::{TestContext, uv_snapshot};

/// Create a wheel with valid Python and invalid source such as vendored Python 2 code.
fn bytecode_wheel(context: &TestContext) -> Result<PathBuf> {
    let (filename, bytes) = WheelBuilder::new(&"example".parse()?, &"1.0.0".parse()?)
        .files(&[
            (
                "example/module.py",
                indoc! {r#"
                DEBUG = __debug__
                def answer():
                    """Return the answer."""
                    return 42
            "#},
            ),
            ("example/invalid.py", "def invalid syntax\n"),
            ("example/python2.py", "print 'Python 2'\n"),
        ])
        .build();
    let wheel = context.temp_dir.join(filename);
    fs_err::write(&wheel, bytes)?;
    Ok(wheel)
}

/// Record bytecode paths and modification times to detect Python replacing Serc's output.
fn bytecode_files(package: &Path) -> Result<BTreeMap<PathBuf, SystemTime>> {
    let mut bytecode = BTreeMap::new();
    for entry in WalkDir::new(package) {
        let entry = entry?;
        if entry.path().extension().is_some_and(|ext| ext == "pyc") {
            bytecode.insert(entry.path().to_path_buf(), entry.metadata()?.modified()?);
        }
    }
    Ok(bytecode)
}

/// Python imports Serc's bytecode without rewriting it or creating new files.
/// Re-enable bytecode writes because the test context passes `-B`.
#[test]
fn native_bytecode() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = bytecode_wheel(&context)?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(&wheel)
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
    Bytecode compiled 2 files in [TIME]
     + example==1.0.0 (from file://[TEMP_DIR]/example-1.0.0-py3-none-any.whl)
    ");

    let package = context.site_packages().join("example");
    let compiled = bytecode_files(&package)?;
    assert_eq!(compiled.len(), 2);
    uv_snapshot!(context.python_command()
        .arg("-c").arg(indoc! {r"
            import sys
            sys.dont_write_bytecode = False
            from example.module import DEBUG, answer
            print(answer(), DEBUG, answer.__doc__)
        "}), @"
    exit_code: 0 (success)
    ----- stdout -----
    42 True Return the answer.
    ");
    assert_eq!(compiled, bytecode_files(&package)?);
    Ok(())
}

/// Trailing whitespace makes Python interpret the value as optimization level one.
#[test]
fn native_bytecode_optimized() -> Result<()> {
    let context = uv_test::test_context!("3.13");
    let wheel = bytecode_wheel(&context)?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(&wheel)
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode")
        .env("PYTHONOPTIMIZE", "2 "), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
    Bytecode compiled 2 files in [TIME]
     + example==1.0.0 (from file://[TEMP_DIR]/example-1.0.0-py3-none-any.whl)
    ");

    let package = context.site_packages().join("example");
    let compiled = bytecode_files(&package)?;
    assert_eq!(compiled.len(), 2);
    uv_snapshot!(context.python_command()
        .env("PYTHONOPTIMIZE", "2 ")
        .arg("-c").arg(indoc! {r"
            import sys
            sys.dont_write_bytecode = False
            from example.module import DEBUG, answer
            print(answer(), DEBUG, answer.__doc__)
        "}), @"
    exit_code: 0 (success)
    ----- stdout -----
    42 False Return the answer.
    ");
    assert_eq!(compiled, bytecode_files(&package)?);
    Ok(())
}

/// Python accepts leading whitespace and keeps the requested level in the cache filename.
/// Levels above two also remove docstrings.
#[test]
fn native_bytecode_optimized_docstrings() -> Result<()> {
    let context = uv_test::test_context!("3.14");
    let wheel = bytecode_wheel(&context)?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(&wheel)
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode")
        .env("PYTHONOPTIMIZE", " 3"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
    Bytecode compiled 2 files in [TIME]
     + example==1.0.0 (from file://[TEMP_DIR]/example-1.0.0-py3-none-any.whl)
    ");

    let package = context.site_packages().join("example");
    let compiled = bytecode_files(&package)?;
    assert_eq!(compiled.len(), 2);
    uv_snapshot!(context.python_command()
        .env("PYTHONOPTIMIZE", " 3")
        .arg("-c").arg(indoc! {r"
            import sys
            sys.dont_write_bytecode = False
            from example.module import DEBUG, answer
            print(answer(), DEBUG, answer.__doc__)
        "}), @"
    exit_code: 0 (success)
    ----- stdout -----
    42 False None
    ");
    assert_eq!(compiled, bytecode_files(&package)?);
    Ok(())
}

/// `SOURCE_DATE_EPOCH` defaults to checked hashes. Explicit `CHECKED_HASH` reuses that cache.
/// Python must detect source edits even when the size and modification time are unchanged.
#[test]
fn native_bytecode_checked_hash() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = bytecode_wheel(&context)?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(&wheel)
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode")
        .env("SOURCE_DATE_EPOCH", "0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
    Bytecode compiled 2 files in [TIME]
     + example==1.0.0 (from file://[TEMP_DIR]/example-1.0.0-py3-none-any.whl)
    ");

    let package = context.site_packages().join("example");
    let compiled = bytecode_files(&package)?;
    assert_eq!(compiled.len(), 2);
    context
        .temp_dir
        .child("requirements.txt")
        .write_str(&wheel.display().to_string())?;
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode")
        .env("PYC_INVALIDATION_MODE", "CHECKED_HASH"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Bytecode compiled 0 files in [TIME]
    ");
    assert_eq!(compiled, bytecode_files(&package)?);

    let source = package.join("module.py");
    let modified = fs_err::metadata(&source)?.modified()?;
    fs_err::write(
        &source,
        fs_err::read_to_string(&source)?.replace("return 42", "return 43"),
    )?;
    fs_err::File::options()
        .write(true)
        .open(&source)?
        .set_modified(modified)?;
    uv_snapshot!(context.python_command()
        .arg("-c").arg("from example.module import answer; print(answer())"), @"
    exit_code: 0 (success)
    ----- stdout -----
    43
    ");
    Ok(())
}

/// Unchecked hashes take precedence over `SOURCE_DATE_EPOCH`. Python trusts stale bytecode
/// until uv refreshes it.
#[test]
fn native_bytecode_unchecked_hash() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = bytecode_wheel(&context)?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(&wheel)
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode")
        .env("SOURCE_DATE_EPOCH", "0")
        .env("PYC_INVALIDATION_MODE", "UNCHECKED_HASH"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
    Bytecode compiled 2 files in [TIME]
     + example==1.0.0 (from file://[TEMP_DIR]/example-1.0.0-py3-none-any.whl)
    ");

    let package = context.site_packages().join("example");
    let compiled = bytecode_files(&package)?;
    assert_eq!(compiled.len(), 2);
    // Force Python to check that the original hash matches, allowing writes to detect rejection.
    uv_snapshot!(context.python_command()
        .arg("--check-hash-based-pycs").arg("always")
        .arg("-c").arg("import sys; sys.dont_write_bytecode = False; from example.module import answer; print(answer())"), @"
    exit_code: 0 (success)
    ----- stdout -----
    42
    ");
    assert_eq!(compiled, bytecode_files(&package)?);

    let source = package.join("module.py");
    let modified = fs_err::metadata(&source)?.modified()?;
    fs_err::write(
        &source,
        fs_err::read_to_string(&source)?.replace("return 42", "return 43"),
    )?;
    fs_err::File::options()
        .write(true)
        .open(&source)?
        .set_modified(modified)?;
    uv_snapshot!(context.python_command()
        .arg("-c").arg("from example.module import answer; print(answer())"), @"
    exit_code: 0 (success)
    ----- stdout -----
    42
    ");
    context
        .temp_dir
        .child("requirements.txt")
        .write_str(&wheel.display().to_string())?;
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode")
        .env("PYC_INVALIDATION_MODE", "UNCHECKED_HASH"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Bytecode compiled 1 file in [TIME]
    ");
    uv_snapshot!(context.python_command()
        .arg("-c").arg("from example.module import answer; print(answer())"), @"
    exit_code: 0 (success)
    ----- stdout -----
    43
    ");
    Ok(())
}

/// Unsupported interpreters fail instead of silently selecting Python's compiler.
#[test]
fn native_bytecode_unsupported_python() -> Result<()> {
    let context = uv_test::test_context!("3.11");
    let wheel = bytecode_wheel(&context)?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(&wheel)
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

/// Unsupported bytecode settings report which option cannot be honored by Serc.
#[test]
fn native_bytecode_cache_prefix() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = bytecode_wheel(&context)?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(&wheel)
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

#[test]
fn native_bytecode_no_debug_ranges() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = bytecode_wheel(&context)?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(&wheel)
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode")
        .env("PYTHONNODEBUGRANGES", "1"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
    error: Failed to bytecode-compile installed packages
      cause: serc does not support omitting debug ranges (PYTHONNODEBUGRANGES)
    ");

    Ok(())
}

/// Directory compilation reuses current bytecode, refreshes stale bytecode, and reports errors.
#[test]
fn native_bytecode_recompile() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = bytecode_wheel(&context)?;
    context
        .temp_dir
        .child("requirements.txt")
        .write_str(&wheel.display().to_string())?;
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
    Bytecode compiled 2 files in [TIME]
     + example==1.0.0 (from file://[TEMP_DIR]/example-1.0.0-py3-none-any.whl)
    ");

    let package = context.site_packages().join("example");
    let bytecode = package.join("__pycache__/module.cpython-312.pyc");
    let compiled = fs_err::metadata(&bytecode)?.modified()?;
    let invalid_package = package.join("invalid_only");
    fs_err::create_dir(&invalid_package)?;
    fs_err::write(invalid_package.join("invalid.py"), "def invalid syntax\n")?;
    // An empty SOURCE_DATE_EPOCH retains timestamp invalidation.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode")
        .env("SOURCE_DATE_EPOCH", ""), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Bytecode compiled 0 files in [TIME]
    ");
    assert_eq!(fs_err::metadata(&bytecode)?.modified()?, compiled);
    assert!(!invalid_package.join("__pycache__").exists());

    fs_err::write(
        package.join("module.py"),
        "def answer():\n    return 1234\n",
    )?;
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Bytecode compiled 1 file in [TIME]
    ");
    uv_snapshot!(context.python_command().arg("-c").arg("from example.module import answer; print(answer())"), @"
    exit_code: 0 (success)
    ----- stdout -----
    1234
    ");

    // CP437 is accepted by Python but not by serc.
    fs_err::write(package.join("legacy.py"), "# coding: cp437\nvalue = 42\n")?;
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: Failed to bytecode-compile Python file in: [SITE_PACKAGES]/
      cause: Failed to compile `[SITE_PACKAGES]/example/legacy.py` with serc
      cause: failed to decode Python source: unsupported source encoding `cp437`
      cause: unsupported source encoding `cp437`
    ");
    assert!(!package.join("__pycache__/legacy.cpython-312.pyc").exists());
    Ok(())
}

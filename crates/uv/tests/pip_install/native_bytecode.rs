use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::allow_duplicates;
use walkdir::WalkDir;

use uv_test::packse::generate_wheel_with_files;
use uv_test::{TestContext, uv_snapshot};

/// Create a wheel with valid Python and invalid source such as vendored Python 2 code.
fn bytecode_wheel(context: &TestContext) -> Result<PathBuf> {
    let (filename, bytes) = generate_wheel_with_files(
        &"example".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[
            (
                "example/module.py",
                indoc! {r#"
                DEBUG = __debug__
                def answer():
                    """Return the answer."""
                    return 42
                def check_assertions():
                    assert False
            "#},
            ),
            ("example/invalid.py", "def invalid syntax\n"),
            ("example/python2.py", "print 'Python 2'\n"),
        ],
    );
    let wheel = context.temp_dir.join(filename);
    fs_err::write(&wheel, bytes)?;
    Ok(wheel)
}

/// Record bytecode paths and modification times to detect Python replacing serc's output.
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

/// Python imports serc's bytecode at each optimization level without replacing it.
#[test]
fn native_bytecode() -> Result<()> {
    allow_duplicates! {
        for python_version in ["3.12", "3.13", "3.14"] {
            for optimization in ["0", "1", "2", "3"] {
                let context = uv_test::test_context!(python_version);
                let wheel = bytecode_wheel(&context)?;
                uv_snapshot!(context.filters(), context.pip_install()
                    .arg(&wheel).arg("--compile-bytecode")
                    .arg("--preview-features").arg("native-bytecode")
                    .env("PYTHONOPTIMIZE", optimization), @"
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
                uv_snapshot!(context.python_command().env("PYTHONOPTIMIZE", optimization)
                    .arg("-c").arg(indoc! {r"
                        import sys
                        # TestContext passes -B; allow writes to detect rejected bytecode.
                        sys.dont_write_bytecode = False
                        from example import module

                        optimized = sys.flags.optimize > 0
                        try:
                            module.check_assertions()
                            assertions_enabled = False
                        except AssertionError:
                            assertions_enabled = True
                        if (module.DEBUG, assertions_enabled) != (not optimized, not optimized):
                            raise RuntimeError('Incorrect assertion or __debug__ behavior')
                        if (module.answer.__doc__ is None) != (sys.flags.optimize >= 2):
                            raise RuntimeError('Incorrect docstring behavior')
                        print(module.answer())
                    "}), @"
                exit_code: 0 (success)
                ----- stdout -----
                42
                ");
                assert_eq!(compiled, bytecode_files(&package)?);
            }
        }
        Ok(())
    }
}

/// Hash caches are accepted by Python, reused by uv, and refreshed after same-size source edits.
#[test]
fn native_bytecode_hash() -> Result<()> {
    allow_duplicates! {
        for (variable, value, checked) in [
            ("PYC_INVALIDATION_MODE", "CHECKED_HASH", true),
            ("PYC_INVALIDATION_MODE", "UNCHECKED_HASH", false),
            // SOURCE_DATE_EPOCH selects checked hashes when no explicit mode is provided.
            ("SOURCE_DATE_EPOCH", "0", true),
        ] {
            let context = uv_test::test_context!("3.12");
            let wheel = bytecode_wheel(&context)?;
            context.temp_dir.child("requirements.txt").write_str(&wheel.display().to_string())?;
            uv_snapshot!(context.filters(), context.pip_install()
                .arg(&wheel).arg("--compile-bytecode")
                .arg("--preview-features").arg("native-bytecode")
                .env(variable, value), @"
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
            uv_snapshot!(context.python_command()
                .arg("--check-hash-based-pycs").arg("always")
                .arg("-c").arg("import sys; sys.dont_write_bytecode = False; from example.module import answer; print(answer())"), @"
            exit_code: 0 (success)
            ----- stdout -----
            42
            ");
            assert_eq!(compiled, bytecode_files(&package)?);
            // Warm the directory cache, including the virtualenv shim, then verify a cached run.
            context.pip_sync().arg("requirements.txt").arg("--compile-bytecode")
                .arg("--preview-features").arg("native-bytecode")
                .env(variable, value).assert().success();
            uv_snapshot!(context.filters(), context.pip_sync()
                .arg("requirements.txt").arg("--compile-bytecode")
                .arg("--preview-features").arg("native-bytecode")
                .env(variable, value), @"
            exit_code: 0 (success)
            ----- stderr -----
            Resolved 1 package in [TIME]
            Bytecode compiled 0 files in [TIME]
            ");
            assert_eq!(compiled, bytecode_files(&package)?);

            let source = package.join("module.py");
            let modified = fs_err::metadata(&source)?.modified()?;
            fs_err::write(&source, fs_err::read_to_string(&source)?.replace("return 42", "return 43"))?;
            fs_err::File::options().write(true).open(&source)?.set_modified(modified)?;
            uv_snapshot!(context.python_command().arg("-B").arg("-c").arg(indoc! {r"
                import sys
                from example.module import answer
                if answer() != int(sys.argv[1]):
                    raise RuntimeError('Incorrect hash invalidation behavior')
                print('ok')
            "}).arg(if checked { "43" } else { "42" }), @"
            exit_code: 0 (success)
            ----- stdout -----
            ok
            ");
            uv_snapshot!(context.filters(), context.pip_sync()
                .arg("requirements.txt").arg("--compile-bytecode")
                .arg("--preview-features").arg("native-bytecode")
                .env(variable, value), @"
            exit_code: 0 (success)
            ----- stderr -----
            Resolved 1 package in [TIME]
            Bytecode compiled 1 file in [TIME]
            ");
            uv_snapshot!(context.python_command().arg("-c").arg("from example.module import answer; print(answer())"), @"
            exit_code: 0 (success)
            ----- stdout -----
            43
            ");
        }
        Ok(())
    }
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

/// Unsupported bytecode settings report which option cannot be honored by serc.
#[test]
fn native_bytecode_unsupported_settings() -> Result<()> {
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
    Bytecode compiled 3 files in [TIME]
     + example==1.0.0 (from file://[TEMP_DIR]/example-1.0.0-py3-none-any.whl)
    ");

    let package = context.site_packages().join("example");
    let bytecode = package.join("__pycache__/module.cpython-312.pyc");
    let compiled = fs_err::metadata(&bytecode)?.modified()?;
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Bytecode compiled 0 files in [TIME]
    ");
    assert_eq!(fs_err::metadata(&bytecode)?.modified()?, compiled);

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

    // CP1252 is accepted by Python but not by serc.
    fs_err::write(package.join("legacy.py"), "# coding: cp1252\nvalue = 42\n")?;
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: Failed to bytecode-compile Python file in: [SITE_PACKAGES]/
      cause: Failed to compile `[SITE_PACKAGES]/example/legacy.py` with serc
      cause: failed to decode Python source: unsupported source encoding `cp1252`
      cause: unsupported source encoding `cp1252`
    ");
    assert!(!package.join("__pycache__/legacy.cpython-312.pyc").exists());
    Ok(())
}

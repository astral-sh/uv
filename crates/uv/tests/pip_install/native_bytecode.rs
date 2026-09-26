use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::allow_duplicates;

use uv_test::packse::generate_wheel_with_files;
use uv_test::{TestContext, uv_snapshot};

/// Create a wheel with executable Python source.
fn bytecode_wheel(context: &TestContext) -> Result<PathBuf> {
    let (filename, bytes) = generate_wheel_with_files(
        &"example".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[("example/module.py", "def answer():\n    return 42\n")],
    );
    let wheel = context.temp_dir.join(filename);
    fs_err::write(&wheel, bytes)?;
    Ok(wheel)
}

/// Prove that the opt-in compiles without compileall and produces code the target Python can load.
#[test]
fn native_bytecode() -> Result<()> {
    allow_duplicates! {
        for python_version in ["3.12", "3.13", "3.14"] {
            for preview in [false, true] {
                let context = uv_test::test_context!(python_version);
                let wheel = bytecode_wheel(&context)?;
                // A successful compileall invocation must not mask a broken serc path.
                fs_err::write(context.site_packages().join("disable_compileall.pth"),
                    "import compileall; compileall.compile_file = lambda *args, **kwargs: False\n",
                )?;
                let mut command = context.pip_install();
                command.arg(&wheel).arg("--compile-bytecode");
                if preview {
                    command.arg("--preview-features").arg("native-bytecode");
                }
                uv_snapshot!(context.filters(), command, @"
                exit_code: 0 (success)
                ----- stderr -----
                Resolved 1 package in [TIME]
                Prepared 1 package in [TIME]
                Installed 1 package in [TIME]
                Bytecode compiled 2 files in [TIME]
                 + example==1.0.0 (from file://[TEMP_DIR]/example-1.0.0-py3-none-any.whl)
                ");

                let bytecode_dir = context.site_packages().join("example/__pycache__");
                if !preview {
                    assert!(!bytecode_dir.exists());
                    continue;
                }
                uv_snapshot!(context.python_command().arg("-c").arg(indoc! {r"
                    import importlib.util
                    import marshal
                    import pathlib
                    import struct
                    import sysconfig

                    source = pathlib.Path(sysconfig.get_path('purelib')) / 'example/module.py'
                    bytecode = pathlib.Path(importlib.util.cache_from_source(str(source))).read_bytes()
                    assert bytecode[:4] == importlib.util.MAGIC_NUMBER
                    assert struct.unpack('<III', bytecode[4:16]) == (0, int(source.stat().st_mtime) & 0xffffffff, source.stat().st_size)
                    code = marshal.loads(bytecode[16:])
                    assert pathlib.Path(code.co_filename).samefile(source)
                    assert code == compile(source.read_bytes(), code.co_filename, 'exec')
                    namespace = {}
                    exec(code, namespace)
                    print(namespace['answer']())
                "}), @"
                exit_code: 0 (success)
                ----- stdout -----
                42
                ");
            }
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
      cause: Failed to configure native bytecode compilation
      cause: serc does not support Python 3.11
      cause: unsupported Python version "3.11"; expected 3.12, 3.13, 3.14, or 3.15
    "#);
    Ok(())
}

/// Unsupported bytecode settings report which option cannot be honored by serc.
#[test]
fn native_bytecode_unsupported_settings() -> Result<()> {
    allow_duplicates! {
        for (variable, value) in [
            ("PYC_INVALIDATION_MODE", "CHECKED_HASH"),
            ("SOURCE_DATE_EPOCH", "0"),
        ] {
            let context = uv_test::test_context!("3.12");
            let wheel = bytecode_wheel(&context)?;
            uv_snapshot!(context.filters(), context.pip_install()
                .arg(&wheel)
                .arg("--compile-bytecode")
                .arg("--preview-features").arg("native-bytecode")
                .env(variable, value), @"
            exit_code: 2 (failure)
            ----- stderr -----
            Resolved 1 package in [TIME]
            Prepared 1 package in [TIME]
            Installed 1 package in [TIME]
            error: Failed to bytecode-compile installed packages
              cause: Failed to configure native bytecode compilation
              cause: Bytecode target query failed: serc does not support bytecode invalidation mode CHECKED_HASH
            ");
        }
        Ok::<_, anyhow::Error>(())
    }?;

    let context = uv_test::test_context!("3.12");
    let wheel = bytecode_wheel(&context)?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(&wheel)
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode")
        .env("PYC_INVALIDATION_MODE", "UNCHECKED_HASH"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
    error: Failed to bytecode-compile installed packages
      cause: Failed to configure native bytecode compilation
      cause: Bytecode target query failed: serc does not support bytecode invalidation mode UNCHECKED_HASH
    ");

    let context = uv_test::test_context!("3.12");
    let wheel = bytecode_wheel(&context)?;
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(&wheel)
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode")
        .env("PYTHONOPTIMIZE", "1"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
    error: Failed to bytecode-compile installed packages
      cause: Failed to configure native bytecode compilation
      cause: Bytecode target query failed: serc does not support optimized bytecode (PYTHONOPTIMIZE)
    ");

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
      cause: Failed to configure native bytecode compilation
      cause: Bytecode target query failed: serc does not support a custom bytecode cache prefix (PYTHONPYCACHEPREFIX)
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
      cause: Failed to configure native bytecode compilation
      cause: Bytecode target query failed: serc does not support omitting debug ranges (PYTHONNODEBUGRANGES)
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
    Bytecode compiled 3 files in [TIME]
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
    Bytecode compiled 3 files in [TIME]
    ");
    uv_snapshot!(context.python_command().arg("-c").arg(indoc! {r"
        import importlib.util
        import marshal
        import pathlib
        import sysconfig

        package = pathlib.Path(sysconfig.get_path('purelib')) / 'example'
        source = package / 'module.py'
        bytecode = pathlib.Path(importlib.util.cache_from_source(str(source))).read_bytes()
        namespace = {}
        exec(marshal.loads(bytecode[16:]), namespace)
        print(namespace['answer']())
    "}), @"
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

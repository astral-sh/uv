use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::allow_duplicates;

use uv_test::packse::generate_wheel_with_files;
use uv_test::{TestContext, uv_snapshot};

/// Create a wheel with executable source and a syntax error, as found in vendored Python 2 code.
fn bytecode_wheel(context: &TestContext) -> Result<PathBuf> {
    let (filename, bytes) = generate_wheel_with_files(
        &"example".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[
            ("example/module.py", "def answer():\n    return 42\n"),
            ("example/invalid.py", "def invalid syntax\n"),
        ],
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
                Bytecode compiled 3 files in [TIME]
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
                    assert code.co_filename == str(source)
                    assert code == compile(source.read_bytes(), str(source), 'exec')
                    namespace = {}
                    exec(code, namespace)
                    print(namespace['answer']())
                    assert not pathlib.Path(importlib.util.cache_from_source(str(source.with_name('invalid.py')))).exists()
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

/// Unsupported interpreters and bytecode settings continue to use Python's compiler.
#[test]
fn native_bytecode_fallback() -> Result<()> {
    allow_duplicates! {
        for (python_version, variable, value, flags, optimization) in [
            ("3.11", "PYC_INVALIDATION_MODE", "TIMESTAMP", 0, ""),
            ("3.12", "PYC_INVALIDATION_MODE", "CHECKED_HASH", 3, ""),
            ("3.12", "PYC_INVALIDATION_MODE", "UNCHECKED_HASH", 1, ""),
            ("3.12", "SOURCE_DATE_EPOCH", "0", 3, ""),
            ("3.12", "PYTHONOPTIMIZE", "1", 0, "1"),
            ("3.12", "PYTHONPYCACHEPREFIX", "bytecode-cache", 0, ""),
        ] {
            let context = uv_test::test_context!(python_version);
            let wheel = bytecode_wheel(&context)?;
            // An absolute prefix is required because compiler workers run in the uv cache.
            let value = if variable == "PYTHONPYCACHEPREFIX" {
                context.temp_dir.join(value).to_string_lossy().into_owned()
            } else {
                value.to_string()
            };
            uv_snapshot!(context.filters(), context.pip_install()
                .arg(&wheel)
                .arg("--compile-bytecode")
                .arg("--preview-features").arg("native-bytecode")
                .env(variable, &value), @"
            exit_code: 0 (success)
            ----- stderr -----
            Resolved 1 package in [TIME]
            Prepared 1 package in [TIME]
            Installed 1 package in [TIME]
            Bytecode compiled 3 files in [TIME]
             + example==1.0.0 (from file://[TEMP_DIR]/example-1.0.0-py3-none-any.whl)
            ");

            uv_snapshot!(context.python_command().env(variable, &value)
                .env_remove("PYTHONOPTIMIZE")
                .arg("-c").arg(indoc! {r"
                    import importlib.util
                    import marshal
                    import pathlib
                    import struct
                    import sys
                    import sysconfig

                    source = pathlib.Path(sysconfig.get_path('purelib')) / 'example/module.py'
                    bytecode = pathlib.Path(importlib.util.cache_from_source(str(source), optimization=sys.argv[2])).read_bytes()
                    assert bytecode[:4] == importlib.util.MAGIC_NUMBER
                    assert struct.unpack('<I', bytecode[4:8])[0] == int(sys.argv[1])
                    namespace = {}
                    exec(marshal.loads(bytecode[16:]), namespace)
                    print(namespace['answer']())
                "}).arg(flags.to_string()).arg(optimization), @"
            exit_code: 0 (success)
            ----- stdout -----
            42
            ");
        }
        Ok(())
    }
}

/// Directory compilation reuses current bytecode, refreshes stale bytecode, and falls back per file.
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
    Bytecode compiled 4 files in [TIME]
     + example==1.0.0 (from file://[TEMP_DIR]/example-1.0.0-py3-none-any.whl)
    ");

    let package = context.site_packages().join("example");
    let bytecode = package.join("__pycache__/module.cpython-312.pyc");
    let compiled = fs_err::metadata(&bytecode)?.modified()?;
    // CP1252 is accepted by Python but not by serc.
    fs_err::write(package.join("legacy.py"), "# coding: cp1252\nvalue = 42\n")?;
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("requirements.txt")
        .arg("--compile-bytecode")
        .arg("--preview-features").arg("native-bytecode"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Bytecode compiled 5 files in [TIME]
    ");
    assert_eq!(fs_err::metadata(&bytecode)?.modified()?, compiled);
    assert!(package.join("__pycache__/legacy.cpython-312.pyc").exists());
    assert!(!package.join("__pycache__/invalid.cpython-312.pyc").exists());

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
    Bytecode compiled 5 files in [TIME]
    ");
    uv_snapshot!(context.python_command().arg("-c").arg(indoc! {r"
        import importlib.util
        import marshal
        import pathlib
        import sysconfig

        package = pathlib.Path(sysconfig.get_path('purelib')) / 'example'
        for name in ['module.py', 'legacy.py']:
            source = package / name
            bytecode = pathlib.Path(importlib.util.cache_from_source(str(source))).read_bytes()
            namespace = {}
            exec(marshal.loads(bytecode[16:]), namespace)
            print(namespace['answer']() if name == 'module.py' else namespace['value'])
    "}), @"
    exit_code: 0 (success)
    ----- stdout -----
    1234
    42
    ");
    Ok(())
}

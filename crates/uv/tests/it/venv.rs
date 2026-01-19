use anyhow::Result;
use assert_fs::prelude::*;

#[cfg(unix)]
use fs_err::os::unix::fs::symlink;

#[cfg(any(windows, target_os = "linux"))]
use uv_static::EnvVars;
use uv_test::uv_snapshot;

#[test]
#[cfg(feature = "pypi")]
fn seed_older_python_version() {
    let context = uv_test::test_context_with_versions!(&["3.11"]);
    uv_snapshot!(context.filters(), context.venv()
        .arg(context.venv.as_os_str())
        .arg("--seed")
        .arg("--python")
        .arg("3.11"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Creating virtual environment with seed packages at: .venv
     + pip==24.0
     + setuptools==69.2.0
     + wheel==0.43.0
    Activate with: source .venv/[BIN]/activate
    "
    );

    context.venv.assert(predicates::path::is_dir());
}

#[test]
#[cfg(windows)]
fn windows_shims() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.10", "3.9"]);
    let shim_path = context.temp_dir.child("shim");

    let py39 = context
        .python_versions
        .last()
        .expect("python_path_with_versions to set up the python versions");

    // We want 3.9 and the first version should be 3.10.
    // Picking the last is necessary to prove that shims work because the python version selects
    // the python version from the first path segment by default, so we take the last to prove it's not
    // returning that version.
    assert!(py39.0.to_string().contains("3.9"));

    // Write the shim script that forwards the arguments to the python3.9 installation.
    fs_err::create_dir(&shim_path)?;
    fs_err::write(
        shim_path.child("python.bat"),
        format!(
            "@echo off\r\n{}/python.exe %*",
            py39.1.parent().unwrap().display()
        ),
    )?;

    // Create a virtual environment at `.venv` with the shim
    uv_snapshot!(context.filters(), context.venv()
        .arg(context.venv.as_os_str())
        .env(EnvVars::UV_TEST_PYTHON_PATH, format!("{};{}", shim_path.display(), context.python_path().to_string_lossy())), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.9.[X] interpreter at: [PYTHON-3.9]
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "###
    );

    context.venv.assert(predicates::path::is_dir());

    Ok(())
}

/// See <https://github.com/astral-sh/uv/issues/3280>
#[test]
#[cfg(windows)]
fn path_with_trailing_space_gives_proper_error() {
    let context = uv_test::test_context_with_versions!(&["3.12"]);

    // Set a custom cache directory with a trailing space
    let path_with_trailing_slash = format!("{} ", context.cache_dir.path().display());
    let mut filters = context.filters();
    // Windows translates error messages, for example i get:
    // ": Das System kann den angegebenen Pfad nicht finden. (os error 3)"
    filters.push((
        r"CACHEDIR.TAG`: .* \(os error 3\)",
        "CACHEDIR.TAG`: The system cannot find the path specified. (os error 3)",
    ));
    uv_snapshot!(filters, std::process::Command::new(uv_test::get_bin!())
        .arg("venv")
        .env(EnvVars::UV_CACHE_DIR, path_with_trailing_slash), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to initialize cache at `[CACHE_DIR]/ `
      Caused by: failed to open file `[CACHE_DIR]/ /CACHEDIR.TAG`: The system cannot find the path specified. (os error 3)
    "###
    );
    // Note the extra trailing `/` in the snapshot is due to the filters, not the actual output.
}

/// Check that the activate script still works with the path contains an apostrophe.
#[test]
#[cfg(target_os = "linux")]
fn create_venv_apostrophe() {
    use std::env;
    use std::ffi::OsString;
    use std::io::Write;
    use std::process::Command;
    use std::process::Stdio;

    let context = uv_test::test_context_with_versions!(&["3.12"]);

    let venv_dir = context.temp_dir.join("Testing's");

    uv_snapshot!(context.filters(), context.venv()
        .arg(&venv_dir)
        .arg("--python")
        .arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: Testing's
    Activate with: source Testing's/[BIN]/activate
    "
    );

    // One of them should be commonly available on a linux developer machine, if not, we have to
    // extend the fallbacks.
    let shell = env::var_os(EnvVars::SHELL).unwrap_or(OsString::from("bash"));
    let mut child = Command::new(shell)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .current_dir(&venv_dir)
        .spawn()
        .expect("Failed to spawn shell script");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    std::thread::spawn(move || {
        stdin
            .write_all(". bin/activate && python -c 'import sys; print(sys.prefix)'".as_bytes())
            .expect("Failed to write to stdin");
    });

    let output = child.wait_with_output().expect("Failed to read stdout");

    assert!(output.status.success(), "{output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), venv_dir.to_string_lossy());
}

#[test]
fn venv_python_preference() {
    let context =
        uv_test::test_context_with_versions!(&["3.12", "3.11"]).with_versions_as_managed(&["3.12"]);

    // Create a managed interpreter environment
    uv_snapshot!(context.filters(), context.venv(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X]
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    uv_snapshot!(context.filters(), context.venv().arg("--no-managed-python"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Creating virtual environment at: .venv
    warning: A virtual environment already exists at `.venv`. In the future, uv will require `--clear` to replace it
    Activate with: source .venv/[BIN]/activate
    ");

    uv_snapshot!(context.filters(), context.venv().arg("--no-managed-python"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Creating virtual environment at: .venv
    warning: A virtual environment already exists at `.venv`. In the future, uv will require `--clear` to replace it
    Activate with: source .venv/[BIN]/activate
    ");

    uv_snapshot!(context.filters(), context.venv(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X]
    Creating virtual environment at: .venv
    warning: A virtual environment already exists at `.venv`. In the future, uv will require `--clear` to replace it
    Activate with: source .venv/[BIN]/activate
    ");

    uv_snapshot!(context.filters(), context.venv().arg("--managed-python"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X]
    Creating virtual environment at: .venv
    warning: A virtual environment already exists at `.venv`. In the future, uv will require `--clear` to replace it
    Activate with: source .venv/[BIN]/activate
    ");
}

#[test]
#[cfg(unix)]
fn create_venv_symlink_clear_preservation() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);

    // Create a target directory
    let target_dir = context.temp_dir.child("target");
    target_dir.create_dir_all()?;

    // Create a symlink pointing to the target directory
    let symlink_path = context.temp_dir.child(".venv");
    symlink(&target_dir, &symlink_path)?;

    // Verify symlink exists
    assert!(symlink_path.path().is_symlink());

    // Create virtual environment at symlink location
    uv_snapshot!(context.filters(), context.venv()
        .arg(symlink_path.as_os_str())
        .arg("--python")
        .arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    // Verify symlink is still preserved after creation
    assert!(symlink_path.path().is_symlink());

    // Run uv venv with --clear to test symlink preservation during clear
    uv_snapshot!(context.filters(), context.venv()
        .arg(symlink_path.as_os_str())
        .arg("--clear")
        .arg("--python")
        .arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    // Verify symlink is STILL preserved after --clear
    assert!(symlink_path.path().is_symlink());

    Ok(())
}

#[test]
#[cfg(unix)]
fn create_venv_symlink_recreate_preservation() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);

    // Create a target directory
    let target_dir = context.temp_dir.child("target");
    target_dir.create_dir_all()?;

    // Create a symlink pointing to the target directory
    let symlink_path = context.temp_dir.child(".venv");
    symlink(&target_dir, &symlink_path)?;

    // Verify symlink exists
    assert!(symlink_path.path().is_symlink());

    // Create virtual environment at symlink location
    uv_snapshot!(context.filters(), context.venv()
        .arg(symlink_path.as_os_str())
        .arg("--python")
        .arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    // Verify symlink is preserved after first creation
    assert!(symlink_path.path().is_symlink());

    // Run uv venv again WITHOUT --clear to test recreation behavior
    uv_snapshot!(context.filters(), context.venv()
        .arg(symlink_path.as_os_str())
        .arg("--python")
        .arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    warning: A virtual environment already exists at `.venv`. In the future, uv will require `--clear` to replace it
    Activate with: source .venv/[BIN]/activate
    "
    );

    // Verify symlink is STILL preserved after recreation
    assert!(symlink_path.path().is_symlink());

    Ok(())
}

#[test]
#[cfg(unix)]
fn create_venv_nested_symlink_preservation() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);

    // Create a target directory
    let target_dir = context.temp_dir.child("target");
    target_dir.create_dir_all()?;

    // Create first symlink level: intermediate -> target
    let intermediate_link = context.temp_dir.child("intermediate");
    symlink(&target_dir, &intermediate_link)?;

    // Create second symlink level: .venv -> intermediate (nested symlink)
    let symlink_path = context.temp_dir.child(".venv");
    symlink(&intermediate_link, &symlink_path)?;

    // Verify nested symlink exists
    assert!(symlink_path.path().is_symlink());
    assert!(intermediate_link.path().is_symlink());

    // Create virtual environment at nested symlink location
    uv_snapshot!(context.filters(), context.venv()
        .arg(symlink_path.as_os_str())
        .arg("--python")
        .arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    "
    );

    // Verify both symlinks are preserved
    assert!(symlink_path.path().is_symlink());
    assert!(intermediate_link.path().is_symlink());

    // Run uv venv again to test nested symlink preservation during recreation
    uv_snapshot!(context.filters(), context.venv()
        .arg(symlink_path.as_os_str())
        .arg("--python")
        .arg("3.12"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    warning: A virtual environment already exists at `.venv`. In the future, uv will require `--clear` to replace it
    Activate with: source .venv/[BIN]/activate
    "
    );

    // Verify nested symlinks are STILL preserved
    assert!(symlink_path.path().is_symlink());
    assert!(intermediate_link.path().is_symlink());

    Ok(())
}

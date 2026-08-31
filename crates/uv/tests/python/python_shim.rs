use std::path::Path;
use std::process::Command;

use assert_cmd::assert::OutputAssertExt;
#[cfg(feature = "test-python-managed")]
use assert_fs::{
    assert::PathAssert,
    prelude::{FileWriteStr, PathChild},
};
#[cfg(feature = "test-python-managed")]
use predicates::prelude::predicate;

use uv_static::EnvVars;
use uv_test::{TestContext, uv_snapshot};
use uv_trampoline_builder::python_shim;

fn shim(context: &TestContext) -> Command {
    let path = context
        .bin_dir
        .join(format!("uv-python{}", std::env::consts::EXE_SUFFIX));
    if !path.exists() {
        fs_err::create_dir_all(&context.bin_dir).expect("create shim directory");
        python_shim::write_to_path(&path).expect("write embedded shim");
    }
    installed_shim(context, "uv-python")
}

fn installed_shim(context: &TestContext, name: &str) -> Command {
    let mut command = Command::new(
        context
            .bin_dir
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX)),
    );
    context.add_shared_env(&mut command, false);
    command.env(EnvVars::UV_CACHE_DIR, context.cache_dir.path());
    // Extracted shims find this build of uv through PATH.
    command.env(
        EnvVars::PATH,
        Path::new(env!("CARGO_BIN_EXE_uv"))
            .parent()
            .expect("uv binary directory"),
    );
    command
}

#[test]
fn python_shim() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"]);

    uv_snapshot!(context.filters(), shim(&context).arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 11)
    ");

    uv_snapshot!(context.filters(), shim(&context).arg("+3.12").arg("-c").arg("import sys; print(sys.version_info[:2]); print(sys.argv[1:])").arg("+system"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 12)
    ['+system']
    ");

    uv_snapshot!(context.filters(), shim(&context).arg("-c").arg("raise SystemExit(17)"), @"
    exit_code: 17 (failure)
    ");

    uv_snapshot!(context.filters(), shim(&context).env(EnvVars::UV_INTERNAL__PYTHON_QUERY, "1"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Ignoring recursive query from uv
    ");
}

#[test]
fn python_shim_virtualenv() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"]);
    context
        .venv()
        .arg("--python")
        .arg("3.12")
        .assert()
        .success();

    uv_snapshot!(context.filters(), shim(&context).arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 12)
    ");

    uv_snapshot!(context.filters(), shim(&context).arg("+system").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 11)
    ");
}

#[test]
#[cfg(feature = "test-python-managed")]
fn python_install_shim() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_python_download_cache();

    uv_snapshot!(context.filters(), context.python_install().arg("--shim").arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The uv Python shim is experimental and may change without warning. Pass `--preview-features python-shim` to disable this warning
    Installed Python shim to `[BIN]/python`
    Installed Python shim to `[BIN]/python3`
    Installed Python shim to `[BIN]/python3.12`
    Installed Python 3.12.[LATEST] in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM]
    ");

    uv_snapshot!(context.filters(), installed_shim(&context, "python").arg("+managed").arg("+3.12").arg("-c").arg("print('hello from the installed shim')"), @"
    exit_code: 0 (success)
    ----- stdout -----
    hello from the installed shim
    ");

    uv_snapshot!(context.filters(), shim(&context).arg("+managed").arg("+3.12").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 12)
    ");

    uv_snapshot!(context.filters(), context.python_install().arg("--shim").arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The uv Python shim is experimental and may change without warning. Pass `--preview-features python-shim` to disable this warning
    Python executable already exists at `[BIN]/python`
    Python executable already exists at `[BIN]/python3`
    Python executable already exists at `[BIN]/python3.12`
    Python 3.12 is already installed
    ");

    for name in ["python", "python3", "python3.12"] {
        let installed = context
            .bin_dir
            .child(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        fs_err::remove_file(installed.path()).expect("remove installed shim");
        installed
            .write_str("existing executable")
            .expect("write existing executable");
    }
    uv_snapshot!(context.filters(), context.python_install().arg("--shim").arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The uv Python shim is experimental and may change without warning. Pass `--preview-features python-shim` to disable this warning
    Python executable already exists at `[BIN]/python`
    Python executable already exists at `[BIN]/python3`
    Python executable already exists at `[BIN]/python3.12`
    Python 3.12 is already installed
    ");
    for name in ["python", "python3", "python3.12"] {
        let installed = context
            .bin_dir
            .child(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        installed.assert("existing executable");
    }

    // Explicitly forcing the install can replace all of the unmanaged names.
    uv_snapshot!(context.filters(), context.python_install().args(["--preview-features", "python-shim", "--force", "3.12"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python shim to `[BIN]/python`
    Installed Python shim to `[BIN]/python3`
    Installed Python shim to `[BIN]/python3.12`
    Python 3.12 is already installed
    ");
    let shim_contents = python_shim::binary().expect("embedded shim for this platform");
    for name in ["python", "python3", "python3.12"] {
        let installed = context
            .bin_dir
            .child(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        assert!(
            fs_err::read(installed.path()).expect("read installed shim") == shim_contents,
            "{name} should be a shim"
        );
    }
}

#[test]
#[cfg(feature = "test-python-managed")]
fn python_install_shim_from_standalone_uv() {
    let binaries = assert_fs::TempDir::new().expect("create binary directory");
    let uv = binaries.child(format!("uv{}", std::env::consts::EXE_SUFFIX));
    fs_err::copy(env!("CARGO_BIN_EXE_uv"), uv.path()).expect("copy standalone uv");
    let context = TestContext::new_with_versions_and_bin(&[], uv.path().to_path_buf())
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_python_download_cache();

    // There is no uv-python next to uv. Keep system utilities available for
    // Python installation (e.g., install_name_tool on macOS), but do not put
    // the build directory on PATH.
    uv_snapshot!(context.filters(), context.python_install().args(["--preview-features", "python-shim", "3.12"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python shim to `[BIN]/python`
    Installed Python shim to `[BIN]/python3`
    Installed Python shim to `[BIN]/python3.12`
    Installed Python 3.12.[LATEST] in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM]
    ");

    for name in ["python", "python3", "python3.12"] {
        let path = context
            .bin_dir
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        assert!(
            !fs_err::symlink_metadata(&path)
                .expect("shim metadata")
                .is_symlink()
        );
        insta::allow_duplicates! {
            uv_snapshot!(context.filters(), installed_shim(&context, name).env(EnvVars::PATH, binaries.path()).args(["+managed", "-c", "import sys; print(sys.version_info[:2])"]), @"
            exit_code: 0 (success)
            ----- stdout -----
            (3, 12)
            ");
        }
    }

    // Shims also find uv alongside themselves without a PATH entry.
    fs_err::copy(
        uv.path(),
        context.bin_dir.join(uv.file_name().expect("uv filename")),
    )
    .expect("copy uv beside shims");
    uv_snapshot!(context.filters(), installed_shim(&context, "python3.12").env(EnvVars::PATH, "").args(["+managed", "-c", "print('sibling uv')"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    sibling uv
    ");
}

#[test]
#[cfg(feature = "test-python-managed")]
fn python_install_shim_preview() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_python_download_cache();

    let installed = context
        .bin_dir
        .child(format!("python{}", std::env::consts::EXE_SUFFIX));

    // Enabling another preview feature should not install the shim.
    uv_snapshot!(context.filters(), context.python_install().args(["--preview-features", "python-install-default"]).arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.[LATEST] in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM] (python3.12)
    ");
    installed.assert(predicate::path::missing());

    // An explicit opt-out takes precedence over the named preview feature.
    uv_snapshot!(context.filters(), context.python_install().args(["--preview-features", "python-shim"]).arg("--no-shim").arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python 3.12 is already installed
    ");
    installed.assert(predicate::path::missing());

    // The named preview feature is sufficient to install the shim.
    uv_snapshot!(context.filters(), context.python_install().args(["--preview-features", "python-shim"]).arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python shim to `[BIN]/python`
    Installed Python shim to `[BIN]/python3`
    Installed Python shim to `[BIN]/python3.12`
    Python 3.12 is already installed
    ");
    installed.assert(predicate::path::exists());

    // Explicit installation does not warn when the named preview feature is enabled.
    uv_snapshot!(context.filters(), context.python_install().args(["--preview-features", "python-shim"]).arg("--shim").arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python executable already exists at `[BIN]/python`
    Python executable already exists at `[BIN]/python3`
    Python executable already exists at `[BIN]/python3.12`
    Python 3.12 is already installed
    ");

    // Global preview mode continues to enable the shim too.
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python executable already exists at `[BIN]/python`
    Python executable already exists at `[BIN]/python3`
    Python executable already exists at `[BIN]/python3.12`
    Python 3.12 is already installed
    ");
}

#[test]
#[cfg(feature = "test-python-managed")]
fn python_shim_names() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_python_sources()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_python_download_cache();

    uv_snapshot!(context.filters(), context.python_install().args(["--preview-features", "python-shim", "3.11", "3.12"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python shim to `[BIN]/python`
    Installed Python shim to `[BIN]/python3`
    Installed Python shim to `[BIN]/python3.11`
    Installed Python shim to `[BIN]/python3.12`
    Installed 2 versions in [TIME]
     + cpython-3.11.[LATEST]-[PLATFORM]
     + cpython-3.12.[LATEST]-[PLATFORM]
    ");

    uv_snapshot!(context.filters(), installed_shim(&context, "python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 12)
    ");
    uv_snapshot!(context.filters(), installed_shim(&context, "python3").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 12)
    ");
    uv_snapshot!(context.filters(), installed_shim(&context, "python3.11").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 11)
    ");
    uv_snapshot!(context.filters(), installed_shim(&context, "python3.12").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 12)
    ");

    // Windows executable names are case-insensitive.
    #[cfg(windows)]
    uv_snapshot!(context.filters(), installed_shim(&context, "PYTHON3.11").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 11)
    ");

    // Explicit version requests still override the default inferred from the name.
    uv_snapshot!(context.filters(), installed_shim(&context, "python3.11").arg("+3.12").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 12)
    ");

    context
        .venv()
        .arg("--python")
        .arg("3.11")
        .assert()
        .success();
    uv_snapshot!(context.filters(), installed_shim(&context, "python").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 11)
    ");
    // A virtual environment with a different minor version must not override the shim name.
    uv_snapshot!(context.filters(), installed_shim(&context, "python3.12").arg("-c").arg("import sys; print(sys.version_info[:2])"), @"
    exit_code: 0 (success)
    ----- stdout -----
    (3, 12)
    ");

    let missing = context
        .bin_dir
        .child(format!("python3.10{}", std::env::consts::EXE_SUFFIX));
    python_shim::write_to_path(missing.path()).expect("create shim for missing version");
    // A missing version must fail rather than falling back to a different minor.
    uv_snapshot!(context.filters(), installed_shim(&context, "python3.10").arg("--version"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: No interpreter found for CPython 3.10 in [PYTHON SOURCES]
    ");
}

#[test]
#[cfg(feature = "test-python-managed")]
fn python_shim_variant_names() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_python_download_cache();

    uv_snapshot!(context.filters(), context.python_install().args(["--preview-features", "python-shim", "3.13t"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python shim to `[BIN]/python3.13t`
    Installed Python shim to `[BIN]/python3t`
    Installed Python shim to `[BIN]/pythont`
    Installed Python 3.13.[LATEST] in [TIME]
     + cpython-3.13.[LATEST]+freethreaded-[PLATFORM]
    ");
    uv_snapshot!(context.filters(), installed_shim(&context, "pythont").arg("-c").arg("import sys; print(sys._is_gil_enabled())"), @"
    exit_code: 0 (success)
    ----- stdout -----
    False
    ");
    uv_snapshot!(context.filters(), installed_shim(&context, "python3t").arg("-c").arg("import sys; print(sys._is_gil_enabled())"), @"
    exit_code: 0 (success)
    ----- stdout -----
    False
    ");
    uv_snapshot!(context.filters(), installed_shim(&context, "python3.13t").arg("-c").arg("import sys; print(sys._is_gil_enabled())"), @"
    exit_code: 0 (success)
    ----- stdout -----
    False
    ");
}

#[test]
#[cfg(feature = "test-python-managed")]
fn python_shim_replaces_managed_links() {
    let context = uv_test::test_context_with_versions!(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_latest_python_versions()
        .with_managed_python_dirs()
        .with_empty_python_install_mirror()
        .with_python_download_cache();

    context
        .python_install()
        .args([
            "--preview-features",
            "python-install-default",
            "--default",
            "3.12",
        ])
        .assert()
        .success();
    uv_snapshot!(context.filters(), context.python_install().args(["--preview-features", "python-shim", "3.12"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python shim to `[BIN]/python`
    Installed Python shim to `[BIN]/python3`
    Installed Python shim to `[BIN]/python3.12`
    Python 3.12 is already installed
    ");
    let shim_contents = python_shim::binary().expect("embedded shim for this platform");
    for name in ["python", "python3", "python3.12"] {
        let installed = context
            .bin_dir
            .child(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        assert!(
            fs_err::read(installed.path()).expect("read installed shim") == shim_contents,
            "{name} should be a shim"
        );
    }

    // Subsequent installs without the feature retain the shims without warning.
    uv_snapshot!(context.filters(), context.python_install().arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python 3.12 is already installed
    ");

    // An explicit default request can restore ordinary interpreter links.
    uv_snapshot!(context.filters(), context.python_install().args(["--preview-features", "python-install-default", "--default", "3.12"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed Python 3.12.[LATEST] in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM] (python, python3, python3.12)
    ");
    for name in ["python", "python3", "python3.12"] {
        let installed = context
            .bin_dir
            .child(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        assert!(
            fs_err::read(installed.path()).expect("read installed interpreter link")
                != shim_contents,
            "{name} should be an interpreter link"
        );
    }
}

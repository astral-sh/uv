#[cfg(feature = "test-python-managed")]
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

fn shim(context: &TestContext) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_uv-python"));
    command.current_dir(context.temp_dir.path());
    context.add_shared_env(&mut command, false);
    command.env(EnvVars::UV_CACHE_DIR, context.cache_dir.path());
    command
}

#[cfg(feature = "test-python-managed")]
fn installed_shim(context: &TestContext, name: &str) -> Command {
    let mut command = Command::new(
        context
            .bin_dir
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX)),
    );
    context.add_shared_env(&mut command, false);
    // Windows shims are copies, so they find this build of uv through PATH.
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
    let shim_contents = fs_err::read(env!("CARGO_BIN_EXE_uv-python")).expect("read shim binary");
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
    uv_fs::symlink_or_copy_file(env!("CARGO_BIN_EXE_uv-python"), missing.path())
        .expect("create shim for missing version");
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
    let shim_contents = fs_err::read(env!("CARGO_BIN_EXE_uv-python")).expect("read shim binary");
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

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
    Installed Python 3.12.[LATEST] in [TIME]
     + cpython-3.12.[LATEST]-[PLATFORM] (python3.12)
    ");

    let installed = context
        .bin_dir
        .child(format!("python{}", std::env::consts::EXE_SUFFIX));
    let mut command = Command::new(installed.path());
    context.add_shared_env(&mut command, false);
    uv_snapshot!(context.filters(), command.arg("+managed").arg("+3.12").arg("-c").arg("print('hello from the installed shim')"), @"
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
    Python 3.12 is already installed
    ");

    fs_err::remove_file(installed.path()).expect("remove installed shim");
    installed
        .write_str("existing executable")
        .expect("write existing executable");
    uv_snapshot!(context.filters(), context.python_install().arg("--shim").arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The uv Python shim is experimental and may change without warning. Pass `--preview-features python-shim` to disable this warning
    Python executable already exists at `[BIN]/python`
    Python 3.12 is already installed
    ");
    assert_eq!(
        fs_err::read_to_string(installed.path()).expect("read existing executable"),
        "existing executable"
    );
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
    Python 3.12 is already installed
    ");
    installed.assert(predicate::path::exists());

    // Explicit installation does not warn when the named preview feature is enabled.
    uv_snapshot!(context.filters(), context.python_install().args(["--preview-features", "python-shim"]).arg("--shim").arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python executable already exists at `[BIN]/python`
    Python 3.12 is already installed
    ");

    // Global preview mode continues to enable the shim too.
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Python executable already exists at `[BIN]/python`
    Python 3.12 is already installed
    ");
}

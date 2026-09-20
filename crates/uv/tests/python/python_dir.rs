use assert_fs::fixture::PathChild;

use uv_static::EnvVars;

use uv_test::uv_snapshot;

#[test]
fn python_dir() {
    let context = uv_test::test_context!("3.12");

    let python_dir = context.temp_dir.child("python");
    uv_snapshot!(context.filters(), context.python_dir()
    .env(EnvVars::UV_PYTHON_INSTALL_DIR, python_dir.as_os_str()), @"
    exit_code: 0 (success)
    ----- stdout -----
    [TEMP_DIR]/python
    ");
}

#[test]
fn python_dir_json() {
    let context = uv_test::test_context!("3.12");

    let python_dir = context.temp_dir.child("python");
    uv_snapshot!(context.filters(), context.python_dir()
    .arg("--output-format").arg("json")
    .env(EnvVars::UV_PYTHON_INSTALL_DIR, python_dir.as_os_str()), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"path":"[TEMP_DIR]/python"}

    ----- stderr -----
    warning: The `--output-format json` option is experimental and the schema may change without warning. Pass `--preview-features json-output` to disable this warning.
    "#);
}

#[test]
fn python_dir_json_preview() {
    let context = uv_test::test_context!("3.12");

    let python_dir = context.temp_dir.child("python");
    uv_snapshot!(context.filters(), context.python_dir()
    .arg("--output-format").arg("json")
    .arg("--preview-features").arg("json-output")
    .env(EnvVars::UV_PYTHON_INSTALL_DIR, python_dir.as_os_str()), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"path":"[TEMP_DIR]/python"}
    "#);
}

#[test]
fn python_dir_bin_json_preview() {
    let context = uv_test::test_context!("3.12");

    let python_bin = context.temp_dir.child("bin");
    uv_snapshot!(context.filters(), context.python_dir()
    .arg("--bin")
    .arg("--output-format").arg("json")
    .arg("--preview-features").arg("json-output")
    .env(EnvVars::UV_PYTHON_BIN_DIR, python_bin.as_os_str()), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {"path":"[TEMP_DIR]/bin"}
    "#);
}

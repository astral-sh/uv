use std::process::Command;

use uv_static::EnvVars;
use uv_test::{get_bin, uv_snapshot};

#[test]
fn adjust_open_file_limit() {
    let context = uv_test::test_context!("3.12");
    let python = &context.python_versions[0].1;

    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg("ulimit -S -n 128; exec \"$@\"")
        .arg("sh")
        .arg(get_bin!())
        .arg("run")
        .arg("--no-project")
        .arg("--")
        .arg(python)
        .arg("-c")
        .arg("import resource; print(resource.getrlimit(resource.RLIMIT_NOFILE)[0] > 128)")
        .current_dir(context.temp_dir.path())
        .env(EnvVars::UV_CACHE_DIR, context.cache_dir.path())
        .env(EnvVars::UV_PYTHON_DOWNLOADS, "never");

    uv_snapshot!(context.filters(), command, @r"
    success: true
    exit_code: 0
    ----- stdout -----
    True

    ----- stderr -----
    ");
}

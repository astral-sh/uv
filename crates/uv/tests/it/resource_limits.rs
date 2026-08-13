use std::process::Command;

use uv_static::EnvVars;
use uv_test::{get_bin, uv_snapshot};

#[test]
fn adjust_open_file_limit() {
    let context = uv_test::test_context!("3.12");
    let python = &context.python_versions[0].1;

    let mut command = Command::new("sh");
    context.add_shared_env(&mut command, false);
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
        .env(EnvVars::UV_CACHE_DIR, context.cache_dir.path());

    uv_snapshot!(context.filters(), command, @r"
    exit_code: 0 (success)
    ----- stdout -----
    True
    ");
}

#[test]
fn run_open_file_limit_override() {
    let context = uv_test::test_context!("3.12");
    let python = &context.python_versions[0].1;

    let mut command = context.run();
    command
        .arg("--no-project")
        .arg("--")
        .arg(python)
        .arg("-c")
        .arg(
            "import resource; soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE); print(soft); print(hard > soft)",
        )
        .env(EnvVars::UV_RUN_RLIMIT_NOFILE, "128");

    uv_snapshot!(context.filters(), command, @r"
    exit_code: 0 (success)
    ----- stdout -----
    128
    True
    ");
}

#[test]
fn run_open_file_limit_override_invalid() {
    let context = uv_test::test_context!("3.12");
    let python = &context.python_versions[0].1;

    let mut command = context.run();
    command
        .arg("--no-project")
        .arg("--")
        .arg(python)
        .arg("-c")
        .arg("pass")
        .env(EnvVars::UV_RUN_RLIMIT_NOFILE, "invalid");

    uv_snapshot!(context.filters(), command, @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to parse environment variable `UV_RUN_RLIMIT_NOFILE` with invalid value `invalid`: invalid digit found in string
    ");
}

#[test]
fn run_open_file_limit_override_exceeds_hard_limit() {
    let context = uv_test::test_context!("3.12");
    let python = &context.python_versions[0].1;

    let mut command = Command::new("sh");
    context.add_shared_env(&mut command, false);
    command
        .arg("-c")
        .arg("ulimit -S -n 128; ulimit -H -n 128; exec \"$@\"")
        .arg("sh")
        .arg(get_bin!())
        .arg("run")
        .arg("--no-project")
        .arg("--")
        .arg(python)
        .arg("-c")
        .arg("pass")
        .env(EnvVars::UV_CACHE_DIR, context.cache_dir.path())
        .env(EnvVars::UV_RUN_RLIMIT_NOFILE, "256");

    uv_snapshot!(context.filters(), command, @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to apply `UV_RUN_RLIMIT_NOFILE` value `256`
      Caused by: requested open file limit (256) exceeds the hard limit (128)
    ");
}

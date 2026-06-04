use uv_test::uv_snapshot;

#[test]
fn upgrade_help() {
    let context = uv_test::test_context_with_versions!(&[]);

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("--help"),
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    Upgrade a dependency in the project

    Usage: uv upgrade [OPTIONS] <PACKAGE>

    Arguments:
      <PACKAGE>  The package to upgrade

    Cache options:
      -n, --no-cache               Avoid reading from or writing to the cache, instead using a temporary
                                   directory for the duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]  Path to the cache directory [env: UV_CACHE_DIR=]

    Python options:
          --managed-python       Require use of uv-managed Python versions [env: UV_MANAGED_PYTHON=]
          --no-managed-python    Disable use of uv-managed Python versions [env: UV_NO_MANAGED_PYTHON=]
          --no-python-downloads  Disable automatic downloads of Python. [env:
                                 "UV_PYTHON_DOWNLOADS=never"]

    Global options:
      -q, --quiet...
              Use quiet output
      -v, --verbose...
              Use verbose output
          --color <COLOR_CHOICE>
              Control the use of color in output [possible values: auto, always, never]
          --system-certs
              Whether to load TLS certificates from the platform's native certificate store [env:
              UV_SYSTEM_CERTS=]
          --offline
              Disable network access [env: UV_OFFLINE=]
          --allow-insecure-host <ALLOW_INSECURE_HOST>
              Allow insecure connections to a host [env: UV_INSECURE_HOST=]
          --no-progress
              Hide all progress outputs [env: UV_NO_PROGRESS=]
          --directory <DIRECTORY>
              Change to the given directory prior to running the command [env: UV_WORKING_DIR=]
          --project <PROJECT>
              Discover a project in the given directory [env: UV_PROJECT=]
          --config-file <CONFIG_FILE>
              The path to a `uv.toml` file to use for configuration [env: UV_CONFIG_FILE=]
          --no-config
              Avoid discovering configuration files (`pyproject.toml`, `uv.toml`) [env: UV_NO_CONFIG=]
      -h, --help
              Display the concise help for this command

    ----- stderr -----
    "#
    );
}

#[test]
fn upgrade_unsupported() {
    let context = uv_test::test_context_with_versions!(&[]);

    uv_snapshot!(
        context.filters(),
        context.upgrade().arg("Requests_Plus"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `uv upgrade` is not implemented yet
    "
    );
}

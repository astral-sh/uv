use std::process::Command;

use assert_fs::fixture::FileWriteStr;
use assert_fs::fixture::PathChild;

use common::uv_snapshot;

use crate::common::{get_bin, TestContext, EXCLUDE_NEWER};

mod common;

/// Create a `pip install` command with options shared across scenarios.
fn install_command(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command
        .arg("pip")
        .arg("install")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--exclude-newer")
        .arg(EXCLUDE_NEWER)
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir);

    if cfg!(all(windows, debug_assertions)) {
        // TODO(konstin): Reduce stack usage in debug mode enough that the tests pass with the
        // default windows stack of 1MB
        command.env("UV_STACK_SIZE", (2 * 1024 * 1024).to_string());
    }

    command
}

#[test]
fn no_package() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###
    );
}

#[test]
fn single_package() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(install_command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Downloaded 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "###
    );

    context.assert_command("import requests").success();
    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    requests v2.31.0
    └── charset-normalizer v3.3.2
    └── idna v3.6
    └── urllib3 v2.2.1
    └── certifi v2024.2.2

    ----- stderr -----
    "###
    );
}

#[test]
fn nested_dependencies() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("scikit-learn==1.4.1.post1")
        .unwrap();

    uv_snapshot!(install_command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Downloaded 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + joblib==1.3.2
     + numpy==1.26.4
     + scikit-learn==1.4.1.post1
     + scipy==1.12.0
     + threadpoolctl==3.4.0
    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    scikit-learn v1.4.1.post1
    └── numpy v1.26.4
    └── scipy v1.12.0
        └── numpy v1.26.4 (*)
    └── joblib v1.3.2
    └── threadpoolctl v3.4.0

    ----- stderr -----
    "###
    );
}

// Ensure `pip tree` behaves correctly after a package has been removed.
#[test]
fn removed_dependency() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(install_command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Downloaded 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("uninstall")
        .arg("requests")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - requests==2.31.0
    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    certifi v2024.2.2
    charset-normalizer v3.3.2
    idna v3.6
    urllib3 v2.2.1

    ----- stderr -----
    "###
    );
}

#[test]
fn multiple_packages() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(
            r"
        requests==2.31.0
        click==8.1.7
    ",
        )
        .unwrap();

    uv_snapshot!(install_command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Downloaded 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + click==8.1.7
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "###
    );

    let mut filters = context.filters();
    if cfg!(windows) {
        filters.push(("colorama v0.4.6\n", ""));
    }
    context.assert_command("import requests").success();
    uv_snapshot!(filters, Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    click v8.1.7
    requests v2.31.0
    └── charset-normalizer v3.3.2
    └── idna v3.6
    └── urllib3 v2.2.1
    └── certifi v2024.2.2

    ----- stderr -----
    "###
    );
}

// Both `pendulum` and `boto3` depend on `python-dateutil`.
#[test]
#[cfg(not(windows))]
fn multiple_packages_shared_descendant() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(
            r"
        pendulum==3.0.0
        boto3==1.34.69
    ",
        )
        .unwrap();

    uv_snapshot!(install_command(&context)
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Downloaded 10 packages in [TIME]
    Installed 10 packages in [TIME]
     + boto3==1.34.69
     + botocore==1.34.69
     + jmespath==1.0.1
     + pendulum==3.0.0
     + python-dateutil==2.9.0.post0
     + s3transfer==0.10.1
     + six==1.16.0
     + time-machine==2.14.1
     + tzdata==2024.1
     + urllib3==2.2.1

    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .env("VIRTUAL_ENV", context.venv.as_os_str())
        .env("UV_NO_WRAP", "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    boto3 v1.34.69
    └── botocore v1.34.69
        └── jmespath v1.0.1
        └── python-dateutil v2.9.0.post0
            └── six v1.16.0
    └── jmespath v1.0.1 (*)
    └── s3transfer v0.10.1
        └── botocore v1.34.69 (*)
    pendulum v3.0.0
    └── python-dateutil v2.9.0.post0 (*)
    └── tzdata v2024.1
    time-machine v2.14.1
    └── python-dateutil v2.9.0.post0 (*)
    urllib3 v2.2.1

    ----- stderr -----
    "###
    );
}

#[test]
fn with_editable() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), install_command(&context)
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/hatchling_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Downloaded 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + hatchling-editable==0.1.0 (from file://[WORKSPACE]/scripts/packages/hatchling_editable)
     + iniconfig==2.0.1.dev6+g9cae431 (from git+https://github.com/pytest-dev/iniconfig@9cae43103df70bac6fde7b9f35ad11a9f1be0cb4)
    "###
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, Command::new(get_bin())
    .arg("pip")
    .arg("tree")
    .arg("--cache-dir")
    .arg(context.cache_dir.path())
    .env("VIRTUAL_ENV", context.venv.as_os_str())
    .env("UV_NO_WRAP", "1")
    .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hatchling-editable v0.1.0
    └── iniconfig v2.0.1.dev6+g9cae431

    ----- stderr -----
    "###
    );
}

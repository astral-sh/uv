#![cfg(not(windows))]

use std::process::Command;

use assert_fs::fixture::FileWriteStr;
use assert_fs::fixture::PathChild;

use uv_static::EnvVars;

use crate::common::get_bin;
use crate::common::{uv_snapshot, TestContext};

#[test]
fn no_package() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----


    ----- stderr -----
    "###
    );
}

#[test]
fn prune_last_in_the_subgroup() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "###
    );

    context.assert_command("import requests").success();
    uv_snapshot!(context.filters(), context.pip_tree().arg("--prune").arg("certifi"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    requests v2.31.0
    ├── charset-normalizer v3.3.2
    ├── idna v3.6
    └── urllib3 v2.2.1

    ----- stderr -----
    "###
    );
}

#[test]
fn single_package() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "###
    );

    context.assert_command("import requests").success();

    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    requests v2.31.0
    ├── certifi v2024.2.2
    ├── charset-normalizer v3.3.2
    ├── idna v3.6
    └── urllib3 v2.2.1

    ----- stderr -----
    "###
    );
}

#[test]
fn nested_dependencies() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    flask v3.0.2
    ├── blinker v1.7.0
    ├── click v8.1.7
    ├── itsdangerous v2.1.2
    ├── jinja2 v3.1.3
    │   └── markupsafe v2.1.5
    └── werkzeug v3.0.1
        └── markupsafe v2.1.5

    ----- stderr -----
    "###
    );
}

/// Identical test as `invert` since `--reverse` is simply an alias for `--invert`.
#[test]
fn reverse() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree().arg("--reverse"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    blinker v1.7.0
    └── flask v3.0.2
    click v8.1.7
    └── flask v3.0.2
    itsdangerous v2.1.2
    └── flask v3.0.2
    markupsafe v2.1.5
    ├── jinja2 v3.1.3
    │   └── flask v3.0.2
    └── werkzeug v3.0.1
        └── flask v3.0.2

    ----- stderr -----
    "###
    );
}

#[test]
fn invert() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree().arg("--invert"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    blinker v1.7.0
    └── flask v3.0.2
    click v8.1.7
    └── flask v3.0.2
    itsdangerous v2.1.2
    └── flask v3.0.2
    markupsafe v2.1.5
    ├── jinja2 v3.1.3
    │   └── flask v3.0.2
    └── werkzeug v3.0.1
        └── flask v3.0.2

    ----- stderr -----
    "###
    );
}

#[test]
fn depth() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--depth")
        .arg("0")
        .env(EnvVars::VIRTUAL_ENV, context.venv.as_os_str())
        .env(EnvVars::UV_NO_WRAP, "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    flask v3.0.2

    ----- stderr -----
    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--depth")
        .arg("1")
        .env(EnvVars::VIRTUAL_ENV, context.venv.as_os_str())
        .env(EnvVars::UV_NO_WRAP, "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    flask v3.0.2
    ├── blinker v1.7.0
    ├── click v8.1.7
    ├── itsdangerous v2.1.2
    ├── jinja2 v3.1.3
    └── werkzeug v3.0.1

    ----- stderr -----
    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--depth")
        .arg("2")
        .env(EnvVars::VIRTUAL_ENV, context.venv.as_os_str())
        .env(EnvVars::UV_NO_WRAP, "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    flask v3.0.2
    ├── blinker v1.7.0
    ├── click v8.1.7
    ├── itsdangerous v2.1.2
    ├── jinja2 v3.1.3
    │   └── markupsafe v2.1.5
    └── werkzeug v3.0.1
        └── markupsafe v2.1.5

    ----- stderr -----
    "###
    );
}

#[test]
fn prune() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--prune")
        .arg("werkzeug")
        .env(EnvVars::VIRTUAL_ENV, context.venv.as_os_str())
        .env(EnvVars::UV_NO_WRAP, "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    flask v3.0.2
    ├── blinker v1.7.0
    ├── click v8.1.7
    ├── itsdangerous v2.1.2
    └── jinja2 v3.1.3
        └── markupsafe v2.1.5

    ----- stderr -----
    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--prune")
        .arg("werkzeug")
        .arg("--prune")
        .arg("jinja2")
        .env(EnvVars::VIRTUAL_ENV, context.venv.as_os_str())
        .env(EnvVars::UV_NO_WRAP, "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    flask v3.0.2
    ├── blinker v1.7.0
    ├── click v8.1.7
    └── itsdangerous v2.1.2
    markupsafe v2.1.5

    ----- stderr -----
    "###
    );

    uv_snapshot!(context.filters(), Command::new(get_bin())
        .arg("pip")
        .arg("tree")
        .arg("--cache-dir")
        .arg(context.cache_dir.path())
        .arg("--prune")
        .arg("werkzeug")
        .env(EnvVars::VIRTUAL_ENV, context.venv.as_os_str())
        .env(EnvVars::UV_NO_WRAP, "1")
        .current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    flask v3.0.2
    ├── blinker v1.7.0
    ├── click v8.1.7
    ├── itsdangerous v2.1.2
    └── jinja2 v3.1.3
        └── markupsafe v2.1.5

    ----- stderr -----
    "###
    );
}

/// Ensure `pip tree` behaves correctly after a package has been removed.
#[test]
fn removed_dependency() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "###
    );

    uv_snapshot!(context.filters(), context
        .pip_uninstall()
        .arg("requests"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - requests==2.31.0
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
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

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + click==8.1.7
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "###
    );

    context.assert_command("import requests").success();
    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    click v8.1.7
    requests v2.31.0
    ├── certifi v2024.2.2
    ├── charset-normalizer v3.3.2
    ├── idna v3.6
    └── urllib3 v2.2.1

    ----- stderr -----
    "###
    );
}

/// Show the installed tree in the presence of a cycle.
#[test]
fn cycle() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(
            r"
        testtools==2.3.0
        fixtures==3.0.0
    ",
        )
        .unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Prepared 10 packages in [TIME]
    Installed 10 packages in [TIME]
     + argparse==1.4.0
     + extras==1.0.0
     + fixtures==3.0.0
     + linecache2==1.0.0
     + pbr==6.0.0
     + python-mimeparse==1.6.0
     + six==1.16.0
     + testtools==2.3.0
     + traceback2==1.4.0
     + unittest2==1.1.0
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    testtools v2.3.0
    ├── extras v1.0.0
    ├── fixtures v3.0.0
    │   ├── pbr v6.0.0
    │   ├── six v1.16.0
    │   └── testtools v2.3.0 (*)
    ├── pbr v6.0.0
    ├── python-mimeparse v1.6.0
    ├── six v1.16.0
    ├── traceback2 v1.4.0
    │   └── linecache2 v1.0.0
    └── unittest2 v1.1.0
        ├── argparse v1.4.0
        ├── six v1.16.0
        └── traceback2 v1.4.0 (*)
    (*) Package tree already displayed

    ----- stderr -----
    "###
    );
}

/// Both `pendulum` and `boto3` depend on `python-dateutil`.
#[test]
fn multiple_packages_shared_descendant() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(
            r"
        pendulum
        time-machine
    ",
        )
        .unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + pendulum==3.0.0
     + python-dateutil==2.9.0.post0
     + six==1.16.0
     + time-machine==2.14.1
     + tzdata==2024.1
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pendulum v3.0.0
    ├── python-dateutil v2.9.0.post0
    │   └── six v1.16.0
    ├── time-machine v2.14.1
    │   └── python-dateutil v2.9.0.post0 (*)
    └── tzdata v2024.1
    (*) Package tree already displayed

    ----- stderr -----
    "###
    );
}

/// Test the interaction between `--no-dedupe` and `--invert`.
#[test]
fn no_dedupe_and_invert() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(
            r"
        pendulum
        time-machine
    ",
        )
        .unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + pendulum==3.0.0
     + python-dateutil==2.9.0.post0
     + six==1.16.0
     + time-machine==2.14.1
     + tzdata==2024.1
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree().arg("--no-dedupe").arg("--invert"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    six v1.16.0
    └── python-dateutil v2.9.0.post0
        ├── pendulum v3.0.0
        └── time-machine v2.14.1
            └── pendulum v3.0.0
    tzdata v2024.1
    └── pendulum v3.0.0

    ----- stderr -----
    "###
    );
}

#[test]
fn no_dedupe() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(
            r"
        pendulum
        time-machine
    ",
        )
        .unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + pendulum==3.0.0
     + python-dateutil==2.9.0.post0
     + six==1.16.0
     + time-machine==2.14.1
     + tzdata==2024.1
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree()
        .arg("--no-dedupe"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    pendulum v3.0.0
    ├── python-dateutil v2.9.0.post0
    │   └── six v1.16.0
    ├── time-machine v2.14.1
    │   └── python-dateutil v2.9.0.post0
    │       └── six v1.16.0
    └── tzdata v2024.1

    ----- stderr -----
    "###
    );
}

#[test]
#[cfg(feature = "git")]
fn with_editable() {
    let context = TestContext::new("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("scripts/packages/hatchling_editable")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
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

    uv_snapshot!(filters, context.pip_tree(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hatchling-editable v0.1.0
    └── iniconfig v2.0.1.dev6+g9cae431

    ----- stderr -----
    "###
    );
}

#[test]
fn package_flag() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree()
        .arg("--package")
        .arg("werkzeug"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    werkzeug v3.0.1
    └── markupsafe v2.1.5

    ----- stderr -----
    "###
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree()
        .arg("--package")
        .arg("werkzeug")
        .arg("--package")
        .arg("jinja2"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    jinja2 v3.1.3
    └── markupsafe v2.1.5
    werkzeug v3.0.1
    └── markupsafe v2.1.5

    ----- stderr -----
    "###
    );
}

#[test]
fn show_version_specifiers_simple() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "###
    );

    uv_snapshot!(context.filters(), context.pip_tree().arg("--show-version-specifiers"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    requests v2.31.0
    ├── certifi v2024.2.2 [required: >=2017.4.17]
    ├── charset-normalizer v3.3.2 [required: >=2, <4]
    ├── idna v3.6 [required: >=2.5, <4]
    └── urllib3 v2.2.1 [required: >=1.21.1, <3]

    ----- stderr -----
    "###
    );
}

#[test]
fn show_version_specifiers_with_invert() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree()
        .arg("--show-version-specifiers")
        .arg("--invert"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    blinker v1.7.0
    └── flask v3.0.2 [requires: blinker >=1.6.2]
    click v8.1.7
    └── flask v3.0.2 [requires: click >=8.1.3]
    itsdangerous v2.1.2
    └── flask v3.0.2 [requires: itsdangerous >=2.1.2]
    markupsafe v2.1.5
    ├── jinja2 v3.1.3 [requires: markupsafe >=2.0]
    │   └── flask v3.0.2 [requires: jinja2 >=3.1.2]
    └── werkzeug v3.0.1 [requires: markupsafe >=2.1.1]
        └── flask v3.0.2 [requires: werkzeug >=3.0.0]

    ----- stderr -----
    "###
    );
}

#[test]
fn show_version_specifiers_with_package() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree()
        .arg("--show-version-specifiers")
        .arg("--package")
        .arg("werkzeug"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    werkzeug v3.0.1
    └── markupsafe v2.1.5 [required: >=2.1.1]

    ----- stderr -----
    "###
    );
}

#[test]
fn print_output_even_with_quite_flag() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    "###
    );

    context.assert_command("import requests").success();
    uv_snapshot!(context.filters(), context.pip_tree().arg("--quiet"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###
    );
}

#[test]
fn outdated() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask==2.0.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + click==8.1.7
     + flask==2.0.0
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    "###
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree().arg("--outdated"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    flask v2.0.0 (latest: v3.0.2)
    ├── click v8.1.7
    ├── itsdangerous v2.1.2
    ├── jinja2 v3.1.3
    │   └── markupsafe v2.1.5
    └── werkzeug v3.0.1
        └── markupsafe v2.1.5

    ----- stderr -----
    "###
    );
}

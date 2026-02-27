#![cfg(not(windows))]

use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::FileTouch;
use assert_fs::fixture::FileWriteStr;
use assert_fs::fixture::PathChild;
use assert_fs::fixture::PathCreateDir;
use indoc::indoc;

use uv_test::uv_snapshot;

#[test]
fn no_package() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.pip_tree(), @"
    success: true
    exit_code: 0
    ----- stdout -----


    ----- stderr -----
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn prune_last_in_the_subgroup() {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
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
    "
    );

    context.assert_command("import requests").success();
    uv_snapshot!(context.filters(), context.pip_tree().arg("--prune").arg("certifi"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    requests v2.31.0
    ├── charset-normalizer v3.3.2
    ├── idna v3.6
    └── urllib3 v2.2.1

    ----- stderr -----
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn single_package() {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
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
    "
    );

    context.assert_command("import requests").success();

    uv_snapshot!(context.filters(), context.pip_tree(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    requests v2.31.0
    ├── certifi v2024.2.2
    ├── charset-normalizer v3.3.2
    ├── idna v3.6
    └── urllib3 v2.2.1

    ----- stderr -----
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn nested_dependencies() {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
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
    "
    );

    uv_snapshot!(context.filters(), context.pip_tree(), @"
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
    "
    );
}

/// Identical test as `invert` since `--reverse` is simply an alias for `--invert`.
#[test]
#[cfg(feature = "test-pypi")]
fn reverse() {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
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
    "
    );

    uv_snapshot!(context.filters(), context.pip_tree().arg("--reverse"), @"
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
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn invert() {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
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
    "
    );

    uv_snapshot!(context.filters(), context.pip_tree().arg("--invert"), @"
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
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn depth() {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
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
    "
    );

    uv_snapshot!(context.filters(), context.pip_tree()
        .arg("--depth")
        .arg("0"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    flask v3.0.2

    ----- stderr -----
    "
    );

    uv_snapshot!(context.filters(), context.pip_tree()
        .arg("--depth")
        .arg("1"), @"
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
    "
    );

    uv_snapshot!(context.filters(), context.pip_tree()
        .arg("--depth")
        .arg("2"), @"
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
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn prune() {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
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
    "
    );

    uv_snapshot!(context.filters(), context.pip_tree()
        .arg("--prune")
        .arg("werkzeug"), @"
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
    "
    );

    uv_snapshot!(context.filters(), context.pip_tree()
        .arg("--prune")
        .arg("werkzeug")
        .arg("--prune")
        .arg("jinja2"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    flask v3.0.2
    ├── blinker v1.7.0
    ├── click v8.1.7
    └── itsdangerous v2.1.2
    markupsafe v2.1.5

    ----- stderr -----
    "
    );

    uv_snapshot!(context.filters(), context.pip_tree()
        .arg("--prune")
        .arg("werkzeug"), @"
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
    "
    );
}

/// Ensure `pip tree` behaves correctly after a package has been removed.
#[test]
#[cfg(feature = "test-pypi")]
fn removed_dependency() {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
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
    "
    );

    uv_snapshot!(context.filters(), context
        .pip_uninstall()
        .arg("requests"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - requests==2.31.0
    "
    );

    uv_snapshot!(context.filters(), context.pip_tree(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    certifi v2024.2.2
    charset-normalizer v3.3.2
    idna v3.6
    urllib3 v2.2.1

    ----- stderr -----
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn multiple_packages() {
    let context = uv_test::test_context!("3.12");

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
        .arg("--strict"), @"
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
    "
    );

    context.assert_command("import requests").success();
    uv_snapshot!(context.filters(), context.pip_tree(), @"
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
    "
    );
}

/// Show the installed tree in the presence of a cycle.
#[test]
#[cfg(feature = "test-pypi")]
fn cycle() {
    let context = uv_test::test_context!("3.12");

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
        .arg("--strict"), @"
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
    "
    );

    uv_snapshot!(context.filters(), context.pip_tree(), @"
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
    "
    );
}

/// Both `pendulum` and `boto3` depend on `python-dateutil`.
#[test]
#[cfg(feature = "test-pypi")]
fn multiple_packages_shared_descendant() {
    let context = uv_test::test_context!("3.12");

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
        .arg("--strict"), @"
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
    "
    );

    uv_snapshot!(context.filters(), context.pip_tree(), @"
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
    "
    );
}

/// Test the interaction between `--no-dedupe` and `--invert`.
#[test]
#[cfg(feature = "test-pypi")]
fn no_dedupe_and_invert() {
    let context = uv_test::test_context!("3.12");

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
        .arg("--strict"), @"
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
    "
    );

    uv_snapshot!(context.filters(), context.pip_tree().arg("--no-dedupe").arg("--invert"), @"
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
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn no_dedupe() {
    let context = uv_test::test_context!("3.12");

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
        .arg("--strict"), @"
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
    "
    );

    uv_snapshot!(context.filters(), context.pip_tree()
        .arg("--no-dedupe"), @"
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
    "
    );
}

#[test]
#[cfg(feature = "test-git")]
fn with_editable() {
    let context = uv_test::test_context!("3.12");

    // Install the editable package.
    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg("-e")
        .arg(context.workspace_root.join("test/packages/hatchling_editable")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + hatchling-editable==0.1.0 (from file://[WORKSPACE]/test/packages/hatchling_editable)
     + iniconfig==2.0.1.dev6+g9cae431 (from git+https://github.com/pytest-dev/iniconfig@9cae43103df70bac6fde7b9f35ad11a9f1be0cb4)
    "
    );

    let filters = context
        .filters()
        .into_iter()
        .chain(vec![(r"\-\-\-\-\-\-+.*", "[UNDERLINE]"), ("  +", " ")])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.pip_tree(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    hatchling-editable v0.1.0
    └── iniconfig v2.0.1.dev6+g9cae431

    ----- stderr -----
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn package_flag() {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
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
    "
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree()
        .arg("--package")
        .arg("werkzeug"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    werkzeug v3.0.1
    └── markupsafe v2.1.5

    ----- stderr -----
    "
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree()
        .arg("--package")
        .arg("werkzeug")
        .arg("--package")
        .arg("jinja2"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    jinja2 v3.1.3
    └── markupsafe v2.1.5
    werkzeug v3.0.1
    └── markupsafe v2.1.5

    ----- stderr -----
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn show_version_specifiers_simple() {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
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
    "
    );

    uv_snapshot!(context.filters(), context.pip_tree().arg("--show-version-specifiers"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    requests v2.31.0
    ├── certifi v2024.2.2 [required: >=2017.4.17]
    ├── charset-normalizer v3.3.2 [required: >=2, <4]
    ├── idna v3.6 [required: >=2.5, <4]
    └── urllib3 v2.2.1 [required: >=1.21.1, <3]

    ----- stderr -----
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn show_version_specifiers_with_invert() {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
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
    "
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree()
        .arg("--show-version-specifiers")
        .arg("--invert"), @"
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
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn show_version_specifiers_with_package() {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
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
    "
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree()
        .arg("--show-version-specifiers")
        .arg("--package")
        .arg("werkzeug"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    werkzeug v3.0.1
    └── markupsafe v2.1.5 [required: >=2.1.1]

    ----- stderr -----
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn print_output_even_with_quite_flag() {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("requests==2.31.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
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
    "
    );

    context.assert_command("import requests").success();
    uv_snapshot!(context.filters(), context.pip_tree().arg("--quiet"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "
    );
}

#[test]
#[cfg(feature = "test-pypi")]
fn outdated() {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask==2.0.0").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @"
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
    "
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree().arg("--outdated"), @"
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
    "
    );
}

/// Test that dependencies with multiple marker-specific requirements
/// are only displayed once in the tree.
#[test]
#[cfg(feature = "test-pypi")]
fn no_duplicate_dependencies_with_markers() {
    const PY_PROJECT: &str = indoc! {r#"
        [project]
        name = "debug"
        version = "0.1.0"
        requires-python = ">=3.12.0"
        dependencies = [
          "sniffio>=1.0.0; python_version >= '3.11'",
          "sniffio>=1.0.1; python_version >= '3.12'",
          "sniffio>=1.0.2; python_version >= '3.13'",
        ]

        [build-system]
        requires = ["uv_build>=0.8.22,<10000"]
        build-backend = "uv_build"
    "#};

    let context = uv_test::test_context_with_versions!(&["3.12", "3.13"]).with_filtered_counts();

    let project = context.temp_dir.child("debug");

    project.create_dir_all().unwrap();

    project.child("src/debug").create_dir_all().unwrap();

    project.child("src/debug/__init__.py").touch().unwrap();

    project
        .child("pyproject.toml")
        .write_str(PY_PROJECT)
        .unwrap();

    context.reset_venv();

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg(project.path())
        .arg("--strict"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + debug==0.1.0 (from file://[TEMP_DIR]/debug)
     + sniffio==1.3.1
    "
    );

    // Ensure that the dependency is only listed once, even though `debug` declares multiple
    // marker-specific requirements for the same dependency.
    uv_snapshot!(context.filters(), context.pip_tree(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    debug v0.1.0
    └── sniffio v1.3.1

    ----- stderr -----
    "
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree().arg("--show-version-specifiers"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    debug v0.1.0
    └── sniffio v1.3.1 [required: >=1.0.1]

    ----- stderr -----
    "
    );

    context
        .venv()
        .arg("--clear")
        .arg("--python")
        .arg("3.13")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context
        .pip_install()
        .arg(project.path())
        .arg("--strict"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + debug==0.1.0 (from file://[TEMP_DIR]/debug)
     + sniffio==1.3.1
    "
    );

    uv_snapshot!(
        context.filters(),
        context.pip_tree().arg("--show-version-specifiers"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    debug v0.1.0
    └── sniffio v1.3.1 [required: >=1.0.2]

    ----- stderr -----
    "
    );
}

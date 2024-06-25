use std::process::Command;

use assert_fs::fixture::FileWriteStr;
use assert_fs::fixture::PathChild;

use common::uv_snapshot;

use crate::common::{get_bin, TestContext};

mod common;

fn tree_command(context: &TestContext) -> Command {
    let mut command = Command::new(get_bin());
    command.arg("pip").arg("tree");
    context.add_shared_args(&mut command);
    command
}

#[test]
fn no_package() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.filters(), tree_command(&context), @r###"
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
    uv_snapshot!(context.filters(), tree_command(&context), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    requests v2.31.0
    ├── charset-normalizer v3.3.2
    ├── idna v3.6
    ├── urllib3 v2.2.1
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
     + joblib==1.3.2
     + numpy==1.26.4
     + scikit-learn==1.4.1.post1
     + scipy==1.12.0
     + threadpoolctl==3.4.0
    "###
    );

    uv_snapshot!(context.filters(), tree_command(&context), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    scikit-learn v1.4.1.post1
    ├── numpy v1.26.4
    ├── scipy v1.12.0
    │   └── numpy v1.26.4 (*)
    ├── joblib v1.3.2
    └── threadpoolctl v3.4.0
    (*) Package tree already displayed

    ----- stderr -----
    "###
    );
}

#[test]
#[cfg(target_os = "macos")]
fn nested_dependencies_more_complex() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("packse").unwrap();

    uv_snapshot!(context
        .pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--strict"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 32 packages in [TIME]
    Prepared 32 packages in [TIME]
    Installed 32 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + chevron-blue==0.2.1
     + docutils==0.20.1
     + hatchling==1.22.4
     + idna==3.6
     + importlib-metadata==7.1.0
     + jaraco-classes==3.3.1
     + jaraco-context==4.3.0
     + jaraco-functools==4.0.0
     + keyring==25.0.0
     + markdown-it-py==3.0.0
     + mdurl==0.1.2
     + more-itertools==10.2.0
     + msgspec==0.18.6
     + nh3==0.2.15
     + packaging==24.0
     + packse==0.3.12
     + pathspec==0.12.1
     + pkginfo==1.10.0
     + pluggy==1.4.0
     + pygments==2.17.2
     + readme-renderer==43.0
     + requests==2.31.0
     + requests-toolbelt==1.0.0
     + rfc3986==2.0.0
     + rich==13.7.1
     + setuptools==69.2.0
     + trove-classifiers==2024.3.3
     + twine==4.0.2
     + urllib3==2.2.1
     + zipp==3.18.1
    "###
    );

    uv_snapshot!(context.filters(), tree_command(&context), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    packse v0.3.12
    ├── chevron-blue v0.2.1
    ├── hatchling v1.22.4
    │   ├── packaging v24.0
    │   ├── pathspec v0.12.1
    │   ├── pluggy v1.4.0
    │   └── trove-classifiers v2024.3.3
    ├── msgspec v0.18.6
    ├── setuptools v69.2.0
    └── twine v4.0.2
        ├── pkginfo v1.10.0
        ├── readme-renderer v43.0
        │   ├── nh3 v0.2.15
        │   ├── docutils v0.20.1
        │   └── pygments v2.17.2
        ├── requests v2.31.0
        │   ├── charset-normalizer v3.3.2
        │   ├── idna v3.6
        │   ├── urllib3 v2.2.1
        │   └── certifi v2024.2.2
        ├── requests-toolbelt v1.0.0
        │   └── requests v2.31.0 (*)
        ├── urllib3 v2.2.1 (*)
        ├── importlib-metadata v7.1.0
        │   └── zipp v3.18.1
        ├── keyring v25.0.0
        │   ├── jaraco-classes v3.3.1
        │   │   └── more-itertools v10.2.0
        │   ├── jaraco-functools v4.0.0
        │   │   └── more-itertools v10.2.0 (*)
        │   └── jaraco-context v4.3.0
        ├── rfc3986 v2.0.0
        └── rich v13.7.1
            ├── markdown-it-py v3.0.0
            │   └── mdurl v0.1.2
            └── pygments v2.17.2 (*)
    (*) Package tree already displayed

    ----- stderr -----
    "###
    );
}

// Ensure `pip tree` behaves correctly with a package that has a cyclic dependency.
// package `uv-cyclic-dependencies-a` and `uv-cyclic-dependencies-b` depend on each other,
// which creates a dependency cycle.
// Additionally, package `uv-cyclic-dependencies-c` is included (depends on `uv-cyclic-dependencies-a`)
// to make this test case more realistic and meaningful.
#[test]
fn cyclic_dependency() {
    let context = TestContext::new("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("uv-cyclic-dependencies-c")
        .unwrap();

    let mut command = context.pip_install();
    command.env_remove("UV_EXCLUDE_NEWER");
    command
        .arg("-r")
        .arg("requirements.txt")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/");

    uv_snapshot!(context.filters(), command, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + uv-cyclic-dependencies-a==0.1.0
     + uv-cyclic-dependencies-b==0.1.0
     + uv-cyclic-dependencies-c==0.1.0
    "###
    );

    uv_snapshot!(context.filters(), tree_command(&context), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    uv-cyclic-dependencies-c v0.1.0
    └── uv-cyclic-dependencies-a v0.1.0
        └── uv-cyclic-dependencies-b v0.1.0
            └── uv-cyclic-dependencies-a v0.1.0 (#)
    (#) Dependency cycle

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

    uv_snapshot!(context.filters(), tree_command(&context), @r###"
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

    let mut filters = context.filters();
    if cfg!(windows) {
        filters.push(("colorama v0.4.6\n", ""));
    }
    context.assert_command("import requests").success();
    uv_snapshot!(filters, tree_command(&context), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    click v8.1.7
    requests v2.31.0
    ├── charset-normalizer v3.3.2
    ├── idna v3.6
    ├── urllib3 v2.2.1
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

    uv_snapshot!(context.filters(), tree_command(&context), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    boto3 v1.34.69
    ├── botocore v1.34.69
    │   ├── jmespath v1.0.1
    │   └── python-dateutil v2.9.0.post0
    │       └── six v1.16.0
    ├── jmespath v1.0.1 (*)
    └── s3transfer v0.10.1
        └── botocore v1.34.69 (*)
    pendulum v3.0.0
    ├── python-dateutil v2.9.0.post0 (*)
    └── tzdata v2024.1
    time-machine v2.14.1
    └── python-dateutil v2.9.0.post0 (*)
    urllib3 v2.2.1
    (*) Package tree already displayed

    ----- stderr -----
    "###
    );
}

// Ensure that --no-dedupe behaves as expected
// in the presence of dependency cycles.
#[test]
#[cfg(not(windows))]
fn no_dedupe_and_cycle() {
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

    let mut command = context.pip_install();
    command.env_remove("UV_EXCLUDE_NEWER");
    command
        .arg("uv-cyclic-dependencies-c==0.1.0")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple/");

    uv_snapshot!(context.filters(), command, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + uv-cyclic-dependencies-a==0.1.0
     + uv-cyclic-dependencies-b==0.1.0
     + uv-cyclic-dependencies-c==0.1.0
    "###
    );

    uv_snapshot!(context.filters(), tree_command(&context)
        .arg("--no-dedupe"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    boto3 v1.34.69
    ├── botocore v1.34.69
    │   ├── jmespath v1.0.1
    │   └── python-dateutil v2.9.0.post0
    │       └── six v1.16.0
    ├── jmespath v1.0.1
    └── s3transfer v0.10.1
        └── botocore v1.34.69
            ├── jmespath v1.0.1
            └── python-dateutil v2.9.0.post0
                └── six v1.16.0
    pendulum v3.0.0
    ├── python-dateutil v2.9.0.post0
    │   └── six v1.16.0
    └── tzdata v2024.1
    time-machine v2.14.1
    └── python-dateutil v2.9.0.post0
        └── six v1.16.0
    urllib3 v2.2.1
    uv-cyclic-dependencies-c v0.1.0
    └── uv-cyclic-dependencies-a v0.1.0
        └── uv-cyclic-dependencies-b v0.1.0
            └── uv-cyclic-dependencies-a v0.1.0 (#)
    (#) Dependency cycle

    ----- stderr -----
    "###
    );
}

#[test]
#[cfg(not(windows))]
fn no_dedupe() {
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

    uv_snapshot!(context.filters(), tree_command(&context)
        .arg("--no-dedupe"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    boto3 v1.34.69
    ├── botocore v1.34.69
    │   ├── jmespath v1.0.1
    │   └── python-dateutil v2.9.0.post0
    │       └── six v1.16.0
    ├── jmespath v1.0.1
    └── s3transfer v0.10.1
        └── botocore v1.34.69
            ├── jmespath v1.0.1
            └── python-dateutil v2.9.0.post0
                └── six v1.16.0
    pendulum v3.0.0
    ├── python-dateutil v2.9.0.post0
    │   └── six v1.16.0
    └── tzdata v2024.1
    time-machine v2.14.1
    └── python-dateutil v2.9.0.post0
        └── six v1.16.0
    urllib3 v2.2.1

    ----- stderr -----
    "###
    );
}

#[test]
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

    uv_snapshot!(filters, tree_command(&context), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hatchling-editable v0.1.0
    └── iniconfig v2.0.1.dev6+g9cae431

    ----- stderr -----

    "###
    );
}

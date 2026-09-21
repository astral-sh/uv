use assert_cmd::prelude::*;

use uv_test::uv_snapshot;

#[test]
fn dry_run_json_install() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.pip_install()
        .arg("anyio==4.3.0")
        .arg("--dry-run")
        .arg("--output-format=json"), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "changes": [
        {
          "name": "anyio",
          "version": "4.3.0",
          "action": "installed"
        },
        {
          "name": "idna",
          "version": "3.6",
          "action": "installed"
        },
        {
          "name": "sniffio",
          "version": "1.3.1",
          "action": "installed"
        }
      ],
      "dry_run": true
    }

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Would download 3 packages
    Would install 3 packages
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "#);

    context.pip_freeze().assert().success().stdout("");
}

#[test]
fn dry_run_json_no_changes() {
    let context = uv_test::test_context!("3.12");
    context
        .pip_install()
        .arg("iniconfig==2.0.0")
        .assert()
        .success();

    // An already satisfied requirement returns before resolution.
    uv_snapshot!(context.pip_install()
        .arg("iniconfig==2.0.0")
        .arg("--dry-run")
        .arg("--output-format=json"), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "changes": [],
      "dry_run": true
    }

    ----- stderr -----
    Checked 1 package in [TIME]
    Would make no changes
    "#);

    // An upgrade can also resolve to the installed version.
    uv_snapshot!(context.pip_install()
        .arg("iniconfig==2.0.0")
        .arg("--upgrade")
        .arg("--dry-run")
        .arg("--output-format=json"), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "changes": [],
      "dry_run": true
    }

    ----- stderr -----
    Resolved 1 package in [TIME]
    Checked 1 package in [TIME]
    Would make no changes
    "#);
}

#[test]
fn dry_run_json_changes() {
    let context = uv_test::test_context!("3.12");
    context
        .pip_install()
        .args(["iniconfig==1.1.1", "idna==3.6"])
        .assert()
        .success();

    uv_snapshot!(context.pip_install()
        .arg("iniconfig==2.0.0")
        .arg("--exact")
        .arg("--dry-run")
        .arg("--output-format=json"), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "changes": [
        {
          "name": "idna",
          "version": "3.6",
          "action": "uninstalled"
        },
        {
          "name": "iniconfig",
          "version": "1.1.1",
          "action": "uninstalled"
        },
        {
          "name": "iniconfig",
          "version": "2.0.0",
          "action": "installed"
        }
      ],
      "dry_run": true
    }

    ----- stderr -----
    Resolved 1 package in [TIME]
    Would download 1 package
    Would uninstall 2 packages
    Would install 1 package
     - idna==3.6
     - iniconfig==1.1.1
     + iniconfig==2.0.0
    "#);

    // JSON remains available when progress messages are disabled.
    uv_snapshot!(context.pip_install()
        .arg("iniconfig==1.1.1")
        .arg("--reinstall")
        .arg("--dry-run")
        .arg("--output-format=json")
        .arg("--quiet"), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "changes": [
        {
          "name": "iniconfig",
          "version": "1.1.1",
          "action": "uninstalled"
        },
        {
          "name": "iniconfig",
          "version": "1.1.1",
          "action": "installed"
        }
      ],
      "dry_run": true
    }
    "#);

    uv_snapshot!(context.pip_freeze(), @"
    exit_code: 0 (success)
    ----- stdout -----
    idna==3.6
    iniconfig==1.1.1
    ");
}

#[test]
fn dry_run_json_errors() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.pip_install()
        .arg("iniconfig==2.0.0")
        .arg("--output-format=json"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the following required arguments were not provided:
      --dry-run

    Usage: uv pip install --dry-run --cache-dir [CACHE_DIR] --output-format <OUTPUT_FORMAT> --exclude-newer <EXCLUDE_NEWER> <PACKAGE|--requirements <REQUIREMENTS>|--editable <EDITABLE>|--group <GROUP>>

    For more information, try '--help'.
    ");

    uv_snapshot!(context.pip_install()
        .arg("iniconfig==2.0.0")
        .arg("--no-index")
        .arg("--dry-run")
        .arg("--output-format=json"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because iniconfig was not found in the provided package locations and you require iniconfig==2.0.0, we can conclude that your requirements are unsatisfiable.

    hint: Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)
    ");

    context.pip_freeze().assert().success().stdout("");
}

#[test]
fn dry_run_json_source_distribution() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.pip_install()
        .arg("anyio @ https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz")
        .arg("--no-deps")
        .arg("--dry-run")
        .arg("--output-format=json")
        .arg("--quiet"), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    {
      "schema": {
        "version": "preview"
      },
      "changes": [
        {
          "name": "anyio",
          "action": "installed"
        }
      ],
      "dry_run": true
    }
    "#);

    context.pip_freeze().assert().success().stdout("");
}

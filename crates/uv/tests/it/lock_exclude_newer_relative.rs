use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::{FileWriteStr, PathChild};
use insta::assert_snapshot;
use uv_static::EnvVars;

use crate::common::{TestContext, uv_snapshot};

/// Lock with a relative exclude-newer timestamp.
#[test]
fn lock_exclude_newer_relative_timestamp() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z")
        .arg("--exclude-newer")
        .arg("2 weeks"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2025-11-07T12:00:00Z"
    exclude-newer-span = "P2W"

    [[package]]
    name = "iniconfig"
    version = "2.3.0"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/72/34/14ca021ce8e5dfedc35312d08ba8bf51fdd999c576889fc2c24cb97f4f10/iniconfig-2.3.0.tar.gz", hash = "sha256:c76315c77db068650d49c5b56314774a7804df16fee4402c1f19d6d15d8c4730", size = 20503, upload-time = "2025-10-18T21:55:43.219Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/cb/b1/3846dd7f199d53cb17f49cba7e651e9ce294d8497c8c150530ed11865bb8/iniconfig-2.3.0-py3-none-any.whl", hash = "sha256:f631c04d2c48c52b84d0d0549c99ff3859c98df65b3101406327ecc7d53fbf12", size = 7484, upload-time = "2025-10-18T21:55:41.639Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "iniconfig" },
    ]

    [package.metadata]
    requires-dist = [{ name = "iniconfig" }]
    "#);

    context
        .lock()
        .env("UV_EXCLUDE_NEWER", "2024-03-25T00:00:00Z")
        .arg("--locked")
        .assert()
        .failure();

    context
        .lock()
        .arg("--upgrade")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z")
        .arg("--exclude-newer")
        .arg("1 week")
        .assert()
        .success();

    Ok(())
}

/// Lock with various relative exclude-newer formats.
#[test]
fn lock_exclude_newer_relative_formats() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("1 day"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let _ = fs_err::remove_file(context.temp_dir.child("uv.lock"));
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("30days"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let _ = fs_err::remove_file(context.temp_dir.child("uv.lock"));
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z")
        .arg("--exclude-newer")
        .arg("3 months"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '3 months' for '--exclude-newer <EXCLUDE_NEWER>': Duration `3 months` uses 'months' which is not allowed; use days instead, e.g., `90 days`.

    For more information, try '--help'.
    ");

    Ok(())
}

/// Error on invalid relative timestamps.
#[test]
fn lock_exclude_newer_invalid_relative() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("invalid span"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'invalid span' for '--exclude-newer <EXCLUDE_NEWER>': `invalid span` could not be parsed as a valid exclude-newer value (expected a date like `2024-01-01`, a timestamp like `2024-01-01T00:00:00Z`, or a relative duration like `3 days` or `2 weeks`)

    For more information, try '--help'.
    ");

    Ok(())
}

/// Lock with package-specific relative exclude-newer should reject months/years.
#[test]
fn lock_exclude_newer_package_relative() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["requests", "tqdm"]
        "#,
    )?;

    // Test that months are rejected in global exclude-newer
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z")
        .arg("--exclude-newer")
        .arg("6 months"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '6 months' for '--exclude-newer <EXCLUDE_NEWER>': Duration `6 months` uses 'months' which is not allowed; use days instead, e.g., `180 days`.

    For more information, try '--help'.
    ");

    // Test that years are rejected in package-specific exclude-newer
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z")
        .arg("--exclude-newer")
        .arg("2 weeks")
        .arg("--exclude-newer-package")
        .arg("tqdm=1 year"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'tqdm=1 year' for '--exclude-newer-package <EXCLUDE_NEWER_PACKAGE>': Invalid `exclude-newer-package` timestamp `1 year`: Duration `1 year` uses unit 'years' which is not allowed; use days instead, e.g., `365 days`.

    For more information, try '--help'.
    ");

    Ok(())
}

/// Error messages for invalid exclude-newer values.
#[test]
fn lock_exclude_newer_error_messages() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--exclude-newer")
        .arg("invalid span"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'invalid span' for '--exclude-newer <EXCLUDE_NEWER>': `invalid span` could not be parsed as a valid exclude-newer value (expected a date like `2024-01-01`, a timestamp like `2024-01-01T00:00:00Z`, or a relative duration like `3 days` or `2 weeks`)

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--exclude-newer")
        .arg("2006-12-02T02:07:43"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###);

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--exclude-newer")
        .arg("12/02/2006"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '12/02/2006' for '--exclude-newer <EXCLUDE_NEWER>': `12/02/2006` could not be parsed as a valid exclude-newer value (expected a date like `2024-01-01`, a timestamp like `2024-01-01T00:00:00Z`, or a relative duration like `3 days` or `2 weeks`)

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--exclude-newer")
        .arg("2 weak"), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '2 weak' for '--exclude-newer <EXCLUDE_NEWER>': `2 weak` could not be parsed as a relative duration: failed to parse "2 weak" in the "friendly" format: parsed value 'P2W', but unparsed input "eak" remains (expected no unparsed input)

    For more information, try '--help'.
    "#);

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--exclude-newer")
        .arg("30"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '30' for '--exclude-newer <EXCLUDE_NEWER>': `30` could not be parsed as a valid exclude-newer value (expected a date like `2024-01-01`, a timestamp like `2024-01-01T00:00:00Z`, or a relative duration like `3 days` or `2 weeks`)

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--exclude-newer")
        .arg("1000000 years"), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '1000000 years' for '--exclude-newer <EXCLUDE_NEWER>': `1000000 years` could not be parsed as a relative duration: failed to parse "1000000 years" in the "friendly" format: failed to set value 1000000 as year unit on span: parameter 'years' with value 1000000 is not in the required range of -19998..=19998

    For more information, try '--help'.
    "#);

    Ok(())
}

/// Lock with relative exclude-newer in pyproject.toml.
#[test]
fn lock_exclude_newer_relative_pyproject() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [tool.uv]
        exclude-newer = "2 weeks"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove("UV_EXCLUDE_NEWER").env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z").env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2025-11-07T12:00:00Z"
    exclude-newer-span = "P2W"

    [[package]]
    name = "iniconfig"
    version = "2.3.0"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/72/34/14ca021ce8e5dfedc35312d08ba8bf51fdd999c576889fc2c24cb97f4f10/iniconfig-2.3.0.tar.gz", hash = "sha256:c76315c77db068650d49c5b56314774a7804df16fee4402c1f19d6d15d8c4730", size = 20503, upload-time = "2025-10-18T21:55:43.219Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/cb/b1/3846dd7f199d53cb17f49cba7e651e9ce294d8497c8c150530ed11865bb8/iniconfig-2.3.0-py3-none-any.whl", hash = "sha256:f631c04d2c48c52b84d0d0549c99ff3859c98df65b3101406327ecc7d53fbf12", size = 7484, upload-time = "2025-10-18T21:55:41.639Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "iniconfig" },
    ]

    [package.metadata]
    requires-dist = [{ name = "iniconfig" }]
    "#);
    uv_snapshot!(context.filters(), context.lock().env_remove("UV_EXCLUDE_NEWER").env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z").env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to addition of exclude newer span `P2W`
    Resolved 2 packages in [TIME]
    ");

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        
        [tool.uv]
        exclude-newer = "2024-01-01T00:00:00Z"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove("UV_EXCLUDE_NEWER").env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to change of exclude newer timestamp from `2025-11-07T12:00:00Z` to `2024-01-01T00:00:00Z`
    Resolved 2 packages in [TIME]
    Updated iniconfig v2.3.0 -> v2.0.0
    ");

    let lock2 = context.read("uv.lock");
    assert_snapshot!(lock2, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-01-01T00:00:00Z"

    [[package]]
    name = "iniconfig"
    version = "2.0.0"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "iniconfig" },
    ]

    [package.metadata]
    requires-dist = [{ name = "iniconfig" }]
    "#);

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        
        [tool.uv]
        exclude-newer = "invalid span"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove("UV_EXCLUDE_NEWER").env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 9, column 25
        |
      9 |         exclude-newer = "invalid span"
        |                         ^^^^^^^^^^^^^^
      `invalid span` could not be parsed as a valid exclude-newer value (expected a date like `2024-01-01`, a timestamp like `2024-01-01T00:00:00Z`, or a relative duration like `3 days` or `2 weeks`)

    Ignoring existing lockfile due to removal of global exclude newer
    Resolved 2 packages in [TIME]
    Updated iniconfig v2.0.0 -> v2.3.0
    "#);

    Ok(())
}

/// Update lockfile with --upgrade when exclude-newer changes in pyproject.toml.
#[test]
fn lock_exclude_newer_pyproject_upgrade_works() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        
        [tool.uv]
        exclude-newer = "2 weeks"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove("UV_EXCLUDE_NEWER").env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock1 = context.read("uv.lock");
    let timestamp1 = lock1
        .lines()
        .find(|line| line.contains("exclude-newer = "))
        .expect("Should find exclude-newer line");

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [tool.uv]
        exclude-newer = "30 days"
        "#,
    )?;

    context
        .lock()
        .env_remove("UV_EXCLUDE_NEWER")
        .arg("--upgrade")
        .assert()
        .success();

    let lock2 = context.read("uv.lock");
    let timestamp2 = lock2
        .lines()
        .find(|line| line.contains("exclude-newer = "))
        .expect("Should find exclude-newer line");

    assert_ne!(
        timestamp1, timestamp2,
        "Timestamp should change with --upgrade"
    );

    Ok(())
}

/// Update lockfile when absolute exclude-newer changes in pyproject.toml.
#[test]
fn lock_exclude_newer_pyproject_absolute_update() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--exclude-newer").arg("2024-03-25T00:00:00Z"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock1 = context.read("uv.lock");
    assert_snapshot!(lock1, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [[package]]
    name = "iniconfig"
    version = "2.0.0"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "iniconfig" },
    ]

    [package.metadata]
    requires-dist = [{ name = "iniconfig" }]
    "#);

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        
        [tool.uv]
        exclude-newer = "2024-01-01T00:00:00Z"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove("UV_EXCLUDE_NEWER").env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z").arg("--upgrade"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to change of exclude newer timestamp from `2024-03-25T00:00:00Z` to `2024-01-01T00:00:00Z`
    Resolved 2 packages in [TIME]
    ");

    let lock2 = context.read("uv.lock");
    assert_snapshot!(lock2, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-01-01T00:00:00Z"

    [[package]]
    name = "iniconfig"
    version = "2.0.0"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "iniconfig" },
    ]

    [package.metadata]
    requires-dist = [{ name = "iniconfig" }]
    "#);

    Ok(())
}

/// Relative timestamps in pyproject.toml produce reproducible lockfiles.
#[test]
fn lock_exclude_newer_pyproject_relative_reproducible() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        
        [tool.uv]
        exclude-newer = "2 weeks"
        "#,
    )?;

    // Initial lock (remove env var to use pyproject.toml value)
    context
        .lock()
        .env_remove("UV_EXCLUDE_NEWER")
        .assert()
        .success();

    // Check the initial lockfile
    let lock1 = context.read("uv.lock");
    let timestamp1 = lock1
        .lines()
        .find(|line| line.contains("exclude-newer = "))
        .expect("Should find exclude-newer line");

    // Sleep for a bit to ensure time passes
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Re-lock WITHOUT any changes - relative timestamps get recalculated each time
    // to the current time minus the span
    context
        .lock()
        .env_remove("UV_EXCLUDE_NEWER")
        .assert()
        .success();

    // Check the lockfile again
    let lock2 = context.read("uv.lock");
    let timestamp2 = lock2
        .lines()
        .find(|line| line.contains("exclude-newer = "))
        .expect("Should find exclude-newer line");

    // Verify both lockfiles have the span stored (for reproducibility tracking)
    assert!(lock1.contains("exclude-newer-span = \"P2W\""));
    assert!(lock2.contains("exclude-newer-span = \"P2W\""));

    // The timestamps will be different because they're computed from the current time,
    // but they should be close (within a few hundred milliseconds)
    assert_ne!(
        timestamp1, timestamp2,
        "Relative timestamps get recalculated each lock run"
    );

    // Now test with ABSOLUTE timestamp
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        
        [tool.uv]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#,
    )?;

    // Lock with absolute timestamp
    context
        .lock()
        .env_remove("UV_EXCLUDE_NEWER")
        .assert()
        .success();

    let lock3 = context.read("uv.lock");
    assert_snapshot!(lock3, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [[package]]
    name = "iniconfig"
    version = "2.0.0"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "iniconfig" },
    ]

    [package.metadata]
    requires-dist = [{ name = "iniconfig" }]
    "#);

    // Re-lock - with absolute timestamp, it should be stable
    context
        .lock()
        .env_remove("UV_EXCLUDE_NEWER")
        .assert()
        .success();

    let lock4 = context.read("uv.lock");
    assert_snapshot!(lock4, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [[package]]
    name = "iniconfig"
    version = "2.0.0"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "iniconfig" },
    ]

    [package.metadata]
    requires-dist = [{ name = "iniconfig" }]
    "#);

    Ok(())
}

/// Test that years are explicitly rejected with specific error message.
#[test]
fn lock_exclude_newer_rejects_years() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z")
        .arg("--exclude-newer")
        .arg("1 year"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '1 year' for '--exclude-newer <EXCLUDE_NEWER>': Duration `1 year` uses unit 'years' which is not allowed; use days instead, e.g., `365 days`.

    For more information, try '--help'.
    ");

    Ok(())
}

/// Test that months are explicitly rejected with specific error message.
#[test]
fn lock_exclude_newer_rejects_months() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z")
        .arg("--exclude-newer")
        .arg("1 month"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '1 month' for '--exclude-newer <EXCLUDE_NEWER>': Duration `1 month` uses 'months' which is not allowed; use days instead, e.g., `30 days`.

    For more information, try '--help'.
    ");

    Ok(())
}

/// Test that package-specific exclude-newer works with valid relative times.
#[test]
fn lock_exclude_newer_package_valid_relative() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["requests", "tqdm"]
        "#,
    )?;

    // Test with days
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z")
        .arg("--exclude-newer-package")
        .arg("tqdm=7 days"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    "###);

    let _ = fs_err::remove_file(context.temp_dir.child("uv.lock"));

    // Test with weeks
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z")
        .arg("--exclude-newer-package")
        .arg("tqdm=2 weeks"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    "###);

    let _ = fs_err::remove_file(context.temp_dir.child("uv.lock"));

    // Test with hours
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z")
        .arg("--exclude-newer-package")
        .arg("tqdm=24 hours"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    "###);

    Ok(())
}

/// Test that package-specific exclude-newer rejects years.
#[test]
fn lock_exclude_newer_package_rejects_years() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["requests", "tqdm"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer-package")
        .arg("numpy=1 year"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'numpy=1 year' for '--exclude-newer-package <EXCLUDE_NEWER_PACKAGE>': Invalid `exclude-newer-package` timestamp `1 year`: Duration `1 year` uses unit 'years' which is not allowed; use days instead, e.g., `365 days`.

    For more information, try '--help'.
    ");

    Ok(())
}

/// Test that package-specific exclude-newer rejects months.
#[test]
fn lock_exclude_newer_package_rejects_months() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["requests", "tqdm"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer-package")
        .arg("tqdm=3 months"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'tqdm=3 months' for '--exclude-newer-package <EXCLUDE_NEWER_PACKAGE>': Invalid `exclude-newer-package` timestamp `3 months`: Duration `3 months` uses 'months' which is not allowed; use days instead, e.g., `90 days`.

    For more information, try '--help'.
    ");

    Ok(())
}

/// Test package-specific relative times in pyproject.toml.
#[test]
fn lock_exclude_newer_package_pyproject_relative() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["requests", "tqdm"]

        [tool.uv.exclude-newer-package]
        tqdm = "7 days"
        requests = "2 weeks"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove("UV_EXCLUDE_NEWER").env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    // Verify the lock file contains package-specific exclude-newer entries
    assert!(lock.contains("[options.exclude-newer-package]"));
    assert!(lock.contains("tqdm = "));
    assert!(lock.contains("requests = "));

    Ok(())
}

/// Test mixing absolute global with relative package-specific timestamps.
#[test]
fn lock_exclude_newer_mixed_absolute_global_relative_package() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["requests", "tqdm"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("2024-01-01T00:00:00Z")
        .arg("--exclude-newer-package")
        .arg("tqdm=7 days"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    assert!(lock.contains("exclude-newer = \"2024-01-01T00:00:00Z\""));
    assert!(lock.contains("[options.exclude-newer-package]"));

    Ok(())
}

/// Test mixing relative global with absolute package-specific timestamps.
#[test]
fn lock_exclude_newer_mixed_relative_global_absolute_package() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["requests", "tqdm"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z")
        .arg("--exclude-newer")
        .arg("1 week")
        .arg("--exclude-newer-package")
        .arg("tqdm=2024-01-01T00:00:00Z"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    assert!(lock.contains("exclude-newer-span = \"P1W\""));
    assert!(lock.contains("[options.exclude-newer-package]"));
    assert!(lock.contains("tqdm = \"2024-01-01T00:00:00Z\""));

    Ok(())
}

/// Test that negative durations produce the same timestamp as positive durations.
/// This ensures that `span.abs()` is applied correctly, so "-1 day" and "1 day" both
/// result in a cutoff 1 day in the past (not 1 day in the future for negative).
#[test]
fn lock_exclude_newer_negative_duration_same_as_positive() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    // Lock with positive duration "7 days"
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z")
        .arg("--exclude-newer")
        .arg("7 days"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock_positive = context.read("uv.lock");
    let timestamp_positive = lock_positive
        .lines()
        .find(|line| line.starts_with("exclude-newer = "))
        .expect("Should find exclude-newer line");

    let _ = fs_err::remove_file(context.temp_dir.child("uv.lock"));

    // Lock with negative ISO 8601 duration "-P7D" (should produce same timestamp)
    // Note: We use --exclude-newer=-P7D to avoid the dash being interpreted as a flag
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z")
        .arg("--exclude-newer=-P7D"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock_negative_iso = context.read("uv.lock");
    let timestamp_negative_iso = lock_negative_iso
        .lines()
        .find(|line| line.starts_with("exclude-newer = "))
        .expect("Should find exclude-newer line");

    let _ = fs_err::remove_file(context.temp_dir.child("uv.lock"));

    // Lock with "7 days ago" friendly format (should also produce same timestamp)
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z")
        .arg("--exclude-newer")
        .arg("7 days ago"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock_ago = context.read("uv.lock");
    let timestamp_ago = lock_ago
        .lines()
        .find(|line| line.starts_with("exclude-newer = "))
        .expect("Should find exclude-newer line");

    // All three should produce the same cutoff timestamp (7 days before 2025-11-21T12:00:00Z)
    assert_eq!(
        timestamp_positive, timestamp_negative_iso,
        "Negative ISO duration should produce the same timestamp as positive duration"
    );
    assert_eq!(
        timestamp_positive, timestamp_ago,
        "'7 days ago' should produce the same timestamp as '7 days'"
    );

    // Verify the actual timestamp is correct (2025-11-14T12:00:00Z = 7 days before 2025-11-21T12:00:00Z)
    assert!(
        timestamp_positive.contains("2025-11-14T12:00:00Z"),
        "Expected timestamp to be 2025-11-14T12:00:00Z, got: {timestamp_positive}"
    );

    Ok(())
}

/// Changing the span in pyproject.toml invalidates the lockfile.
#[test]
fn lock_exclude_newer_span_change_invalidates() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    // Initial pyproject.toml with exclude-newer using "2 weeks" span
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        
        [tool.uv]
        exclude-newer = "2 weeks"
        "#,
    )?;

    // Initial lock (remove env var to use pyproject.toml value)
    uv_snapshot!(context.filters(), context.lock().env_remove("UV_EXCLUDE_NEWER").env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Check the initial lockfile contains both exclude-newer and exclude-newer-span
    let lock1 = context.read("uv.lock");
    assert_snapshot!(lock1, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2025-11-07T12:00:00Z"
    exclude-newer-span = "P2W"

    [[package]]
    name = "iniconfig"
    version = "2.3.0"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/72/34/14ca021ce8e5dfedc35312d08ba8bf51fdd999c576889fc2c24cb97f4f10/iniconfig-2.3.0.tar.gz", hash = "sha256:c76315c77db068650d49c5b56314774a7804df16fee4402c1f19d6d15d8c4730", size = 20503, upload-time = "2025-10-18T21:55:43.219Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/cb/b1/3846dd7f199d53cb17f49cba7e651e9ce294d8497c8c150530ed11865bb8/iniconfig-2.3.0-py3-none-any.whl", hash = "sha256:f631c04d2c48c52b84d0d0549c99ff3859c98df65b3101406327ecc7d53fbf12", size = 7484, upload-time = "2025-10-18T21:55:41.639Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "iniconfig" },
    ]

    [package.metadata]
    requires-dist = [{ name = "iniconfig" }]
    "#);

    // Change the span in pyproject.toml to "30 days"
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        
        [tool.uv]
        exclude-newer = "30 days"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove("UV_EXCLUDE_NEWER").env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to change of exclude newer timestamp from `2025-11-07T12:00:00Z` to `2025-10-22T12:00:00Z`
    Resolved 2 packages in [TIME]
    ");

    let lock2 = context.read("uv.lock");
    assert_snapshot!(lock2, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2025-10-22T12:00:00Z"
    exclude-newer-span = "P30D"

    [[package]]
    name = "iniconfig"
    version = "2.3.0"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/72/34/14ca021ce8e5dfedc35312d08ba8bf51fdd999c576889fc2c24cb97f4f10/iniconfig-2.3.0.tar.gz", hash = "sha256:c76315c77db068650d49c5b56314774a7804df16fee4402c1f19d6d15d8c4730", size = 20503, upload-time = "2025-10-18T21:55:43.219Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/cb/b1/3846dd7f199d53cb17f49cba7e651e9ce294d8497c8c150530ed11865bb8/iniconfig-2.3.0-py3-none-any.whl", hash = "sha256:f631c04d2c48c52b84d0d0549c99ff3859c98df65b3101406327ecc7d53fbf12", size = 7484, upload-time = "2025-10-18T21:55:41.639Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "iniconfig" },
    ]

    [package.metadata]
    requires-dist = [{ name = "iniconfig" }]
    "#);
    uv_snapshot!(context.filters(), context.lock().env_remove("UV_EXCLUDE_NEWER").env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2025-11-21T12:00:00Z"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to addition of exclude newer span `P30D`
    Resolved 2 packages in [TIME]
    ");

    Ok(())
}

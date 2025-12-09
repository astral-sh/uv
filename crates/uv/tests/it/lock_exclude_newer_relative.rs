use anyhow::Result;
use assert_fs::fixture::{FileWriteStr, PathChild};
use insta::assert_snapshot;
use uv_static::EnvVars;

use crate::common::{TestContext, uv_snapshot};

/// Lock with a relative exclude-newer value.
#[test]
fn lock_exclude_newer_relative() -> Result<()> {
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
    exclude-newer = "2024-03-11T00:00:00Z"
    exclude-newer-span = "P2W"

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

    // Changing the current time should not result in a new lockfile
    let current_timestamp = "2024-04-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("2 weeks")
        .arg("--locked"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Changing the span, however, should cause a new resolution
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("1 week"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to change of exclude newer span from `P2W` to `P1W`
    Resolved 2 packages in [TIME]
    ");

    // Both `exclude-newer` values in the lockfile should be changed
    let lock = context.read("uv.lock");
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"
    exclude-newer-span = "P1W"

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

    // Similarly, using something like `--upgrade` should cause a new resolution
    let current_timestamp = "2024-05-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("1 week")
        .arg("--upgrade"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // And the `exclude-newer` timestamp value in the lockfile should be changed
    let lock = context.read("uv.lock");
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-04-24T00:00:00Z"
    exclude-newer-span = "P1W"

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

/// Lock with a relative exclude-newer-package value.
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
        dependencies = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer-package")
        .arg("iniconfig=2 weeks"), @r###"
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

    [options.exclude-newer-package]
    iniconfig = { timestamp = "2024-03-11T00:00:00Z", span = "P2W" }

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

    // Changing the current time should not result in a new lockfile
    let current_timestamp = "2024-04-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer-package")
        .arg("iniconfig=2 weeks")
        .arg("--locked"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Changing the span, however, should cause a new resolution
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer-package")
        .arg("iniconfig=1 week"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to change of exclude newer span from `P2W` to `P1W` for package `iniconfig`
    Resolved 2 packages in [TIME]
    ");

    // Both `exclude-newer-package` values in the lockfile should be changed
    let lock = context.read("uv.lock");
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]

    [options.exclude-newer-package]
    iniconfig = { timestamp = "2024-03-25T00:00:00Z", span = "P1W" }

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

    // Similarly, using something like `--upgrade` should cause a new resolution
    let current_timestamp = "2024-05-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer-package")
        .arg("iniconfig=1 week")
        .arg("--upgrade"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // And the `exclude-newer-package` timestamp value in the lockfile should be changed
    let lock = context.read("uv.lock");
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]

    [options.exclude-newer-package]
    iniconfig = { timestamp = "2024-04-24T00:00:00Z", span = "P1W" }

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

/// Lock with a relative exclude-newer value from the `pyproject.toml`.
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

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
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
    exclude-newer = "2024-03-11T00:00:00Z"
    exclude-newer-span = "P2W"

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

/// Lock with a relative exclude-newer-package value from the `pyproject.toml`.
#[test]
fn lock_exclude_newer_package_relative_pyproject() -> Result<()> {
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
        exclude-newer-package = { iniconfig = "2 weeks" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
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

    [options.exclude-newer-package]
    iniconfig = { timestamp = "2024-03-11T00:00:00Z", span = "P2W" }

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

/// Lock with both global and per-package relative exclude-newer values.
#[test]
fn lock_exclude_newer_relative_global_and_package() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig", "typing-extensions"]
        "#,
    )?;

    // Lock with both global exclude-newer and package-specific override using relative durations
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("2 weeks")
        .arg("--exclude-newer-package")
        .arg("typing-extensions=1 week"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-11T00:00:00Z"
    exclude-newer-span = "P2W"

    [options.exclude-newer-package]
    typing-extensions = { timestamp = "2024-03-18T00:00:00Z", span = "P1W" }

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
        { name = "typing-extensions" },
    ]

    [package.metadata]
    requires-dist = [
        { name = "iniconfig" },
        { name = "typing-extensions" },
    ]

    [[package]]
    name = "typing-extensions"
    version = "4.10.0"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-25T22:12:49.693Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-25T22:12:47.72Z" },
    ]
    "#);

    // Changing the current time should not invalidate the lockfile
    let current_timestamp = "2024-04-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("2 weeks")
        .arg("--exclude-newer-package")
        .arg("typing-extensions=1 week")
        .arg("--locked"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    // Changing the global span should invalidate the lockfile
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("1 week")
        .arg("--exclude-newer-package")
        .arg("typing-extensions=1 week"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to change of exclude newer span from `P2W` to `P1W`
    Resolved 3 packages in [TIME]
    ");

    // Changing the package-specific span should also invalidate the lockfile
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("1 week")
        .arg("--exclude-newer-package")
        .arg("typing-extensions=3 days"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to change of exclude newer span from `P1W` to `P3D` for package `typing-extensions`
    Resolved 3 packages in [TIME]
    ");

    // Use an absolute global value and relative package value
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("2024-03-01T00:00:00Z")
        .arg("--exclude-newer-package")
        .arg("typing-extensions=1 week"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to removal of exclude newer span
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-01T00:00:00Z"

    [options.exclude-newer-package]
    typing-extensions = { timestamp = "2024-03-18T00:00:00Z", span = "P1W" }

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
        { name = "typing-extensions" },
    ]

    [package.metadata]
    requires-dist = [
        { name = "iniconfig" },
        { name = "typing-extensions" },
    ]

    [[package]]
    name = "typing-extensions"
    version = "4.10.0"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-25T22:12:49.693Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-25T22:12:47.72Z" },
    ]
    "#);

    // Use a relative global value and absolute package value
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("2 weeks")
        .arg("--exclude-newer-package")
        .arg("typing-extensions=2024-03-01T00:00:00Z"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to addition of exclude newer span `P2W`
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-11T00:00:00Z"
    exclude-newer-span = "P2W"

    [options.exclude-newer-package]
    typing-extensions = "2024-03-01T00:00:00Z"

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
        { name = "typing-extensions" },
    ]

    [package.metadata]
    requires-dist = [
        { name = "iniconfig" },
        { name = "typing-extensions" },
    ]

    [[package]]
    name = "typing-extensions"
    version = "4.10.0"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-25T22:12:49.693Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-25T22:12:47.72Z" },
    ]
    "#);

    Ok(())
}

/// Lock with various relative exclude newer value formats.
#[test]
fn lock_exclude_newer_relative_values() -> Result<()> {
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

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("30days"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to change of exclude newer span from `P1D` to `P30D`
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("P1D"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to change of exclude newer span from `P30D` to `P1D`
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("1 week"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to change of exclude newer span from `P1D` to `P1W`
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("1 week ago"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to change of exclude newer span from `P1W` to `-P1W`
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("3 months"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '3 months' for '--exclude-newer <EXCLUDE_NEWER>': Duration `3 months` uses 'months' which is not allowed; use days instead, e.g., `90 days`.

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("2 months ago"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '2 months ago' for '--exclude-newer <EXCLUDE_NEWER>': Duration `2 months ago` uses 'months' which is not allowed; use days instead, e.g., `60 days`.

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("1 year"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '1 year' for '--exclude-newer <EXCLUDE_NEWER>': Duration `1 year` uses unit 'years' which is not allowed; use days instead, e.g., `365 days`.

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("1 year ago"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '1 year ago' for '--exclude-newer <EXCLUDE_NEWER>': Duration `1 year ago` uses unit 'years' which is not allowed; use days instead, e.g., `365 days`.

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--exclude-newer")
        .arg("invalid span"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'invalid span' for '--exclude-newer <EXCLUDE_NEWER>': `invalid span` could not be parsed as a valid exclude-newer value (expected a date like `2024-01-01`, a timestamp like `2024-01-01T00:00:00Z`, or a duration like `3 days` or `P3D`)

    For more information, try '--help'.
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--exclude-newer")
        .arg("P4Z"), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value 'P4Z' for '--exclude-newer <EXCLUDE_NEWER>': `P4Z` could not be parsed as an ISO 8601 duration: failed to parse "P4Z" as an ISO 8601 duration string: expected to find date unit designator suffix (Y, M, W or D), but found "Z" instead

    For more information, try '--help'.
    "#);

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--exclude-newer")
        .arg("2006-12-02T02:07:43"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to removal of exclude newer span
      × No solution found when resolving dependencies:
      ╰─▶ Because there are no versions of iniconfig and your project depends on iniconfig, we can conclude that your project's requirements are unsatisfiable.
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--exclude-newer")
        .arg("12/02/2006"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: invalid value '12/02/2006' for '--exclude-newer <EXCLUDE_NEWER>': `12/02/2006` could not be parsed as a valid exclude-newer value (expected a date like `2024-01-01`, a timestamp like `2024-01-01T00:00:00Z`, or a duration like `3 days` or `P3D`)

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
    error: invalid value '2 weak' for '--exclude-newer <EXCLUDE_NEWER>': `2 weak` could not be parsed as a duration: failed to parse "2 weak" in the "friendly" format: parsed value 'P2W', but unparsed input "eak" remains (expected no unparsed input)

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
    error: invalid value '30' for '--exclude-newer <EXCLUDE_NEWER>': `30` could not be parsed as a valid exclude-newer value (expected a date like `2024-01-01`, a timestamp like `2024-01-01T00:00:00Z`, or a duration like `3 days` or `P3D`)

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
    error: invalid value '1000000 years' for '--exclude-newer <EXCLUDE_NEWER>': `1000000 years` could not be parsed as a duration: failed to parse "1000000 years" in the "friendly" format: failed to set value 1000000 as year unit on span: parameter 'years' with value 1000000 is not in the required range of -19998..=19998

    For more information, try '--help'.
    "#);

    Ok(())
}

/// Lock with various relative exclude newer value formats in a `pyproject.toml`.
#[test]
fn lock_exclude_newer_relative_values_pyproject() -> Result<()> {
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
        exclude-newer = "invalid span"
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 9, column 25
        |
      9 |         exclude-newer = "invalid span"
        |                         ^^^^^^^^^^^^^^
      `invalid span` could not be parsed as a valid exclude-newer value (expected a date like `2024-01-01`, a timestamp like `2024-01-01T00:00:00Z`, or a duration like `3 days` or `P3D`)

    Resolved 2 packages in [TIME]
    "#);

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [tool.uv]
        exclude-newer = "2 foos"
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 9, column 25
        |
      9 |         exclude-newer = "2 foos"
        |                         ^^^^^^^^
      `2 foos` could not be parsed as a duration: failed to parse "2 foos" in the "friendly" format: expected to find unit designator suffix (e.g., 'years' or 'secs'), but found input beginning with "foos" instead

    Resolved 2 packages in [TIME]
    "#);

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [tool.uv]
        exclude-newer = "P4Z"
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 9, column 25
        |
      9 |         exclude-newer = "P4Z"
        |                         ^^^^^
      `P4Z` could not be parsed as an ISO 8601 duration: failed to parse "P4Z" as an ISO 8601 duration string: expected to find date unit designator suffix (Y, M, W or D), but found "Z" instead

    Resolved 2 packages in [TIME]
    "#);

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [tool.uv]
        exclude-newer = "10"
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 9, column 25
        |
      9 |         exclude-newer = "10"
        |                         ^^^^
      `10` could not be parsed as a valid exclude-newer value (expected a date like `2024-01-01`, a timestamp like `2024-01-01T00:00:00Z`, or a duration like `3 days` or `P3D`)

    Resolved 2 packages in [TIME]
    "#);

    Ok(())
}

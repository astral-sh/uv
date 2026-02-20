use anyhow::Result;
use assert_fs::fixture::{FileWriteStr, PathChild};
use insta::assert_snapshot;
use uv_static::EnvVars;

use uv_test::uv_snapshot;

/// Lock with a relative exclude-newer value.
///
/// Uses idna which has releases at:
/// - 3.6: 2023-11-25
/// - 3.7: 2024-04-11
#[test]
fn lock_exclude_newer_relative() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["idna"]
        "#,
    )?;

    // 3 weeks before 2024-05-01 is 2024-04-10, which is before idna 3.7 (released 2024-04-11).
    let current_timestamp = "2024-05-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("3 weeks"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    // Should resolve to idna 3.6 (released 2023-11-25, before cutoff of 2024-04-10)
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-04-10T00:00:00Z"
    exclude-newer-span = "P3W"

    [[package]]
    name = "idna"
    version = "3.6"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "idna" },
    ]

    [package.metadata]
    requires-dist = [{ name = "idna" }]
    "#);

    // Changing the current time should not result in a new lockfile
    let later_timestamp = "2024-06-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, later_timestamp)
        .arg("--exclude-newer")
        .arg("3 weeks")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Changing the span to 2 weeks should cause a new resolution.
    // 2 weeks before 2024-05-01 is 2024-04-17, which is after idna 3.7 (released 2024-04-11).
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("2 weeks")
        .arg("--upgrade"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolving despite existing lockfile due to change of exclude newer span from `P3W` to `P2W`
    Resolved 2 packages in [TIME]
    Updated idna v3.6 -> v3.7
    ");

    // Both `exclude-newer` values in the lockfile should be changed, and we should now have idna 3.7
    let lock = context.read("uv.lock");
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-04-17T00:00:00Z"
    exclude-newer-span = "P2W"

    [[package]]
    name = "idna"
    version = "3.7"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/21/ed/f86a79a07470cb07819390452f178b3bef1d375f2ec021ecfc709fc7cf07/idna-3.7.tar.gz", hash = "sha256:028ff3aadf0609c1fd278d8ea3089299412a7a8b9bd005dd08b9f8285bcb5cfc", size = 189575, upload-time = "2024-04-11T03:34:43.276Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/e5/3e/741d8c82801c347547f8a2a06aa57dbb1992be9e948df2ea0eda2c8b79e8/idna-3.7-py3-none-any.whl", hash = "sha256:82fee1fc78add43492d3a1898bfa6d8a904cc97d8427f683ed8e798d07761aa0", size = 66836, upload-time = "2024-04-11T03:34:41.447Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "idna" },
    ]

    [package.metadata]
    requires-dist = [{ name = "idna" }]
    "#);

    // Similarly, using something like `--upgrade` should cause a new resolution
    let current_timestamp = "2024-06-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("2 weeks")
        .arg("--upgrade"), @"
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
    exclude-newer = "2024-05-18T00:00:00Z"
    exclude-newer-span = "P2W"

    [[package]]
    name = "idna"
    version = "3.7"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/21/ed/f86a79a07470cb07819390452f178b3bef1d375f2ec021ecfc709fc7cf07/idna-3.7.tar.gz", hash = "sha256:028ff3aadf0609c1fd278d8ea3089299412a7a8b9bd005dd08b9f8285bcb5cfc", size = 189575, upload-time = "2024-04-11T03:34:43.276Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/e5/3e/741d8c82801c347547f8a2a06aa57dbb1992be9e948df2ea0eda2c8b79e8/idna-3.7-py3-none-any.whl", hash = "sha256:82fee1fc78add43492d3a1898bfa6d8a904cc97d8427f683ed8e798d07761aa0", size = 66836, upload-time = "2024-04-11T03:34:41.447Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "idna" },
    ]

    [package.metadata]
    requires-dist = [{ name = "idna" }]
    "#);

    // Similarly, using something like `--refresh` should cause a new resolution
    let current_timestamp = "2024-07-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("2 weeks")
        .arg("--refresh"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    Ok(())
}

/// Test that exclude-newer changes in either direction work correctly:
/// - Getting OLDER (more restrictive): forces downgrade of invalid versions
/// - Getting NEWER (less restrictive): keeps existing versions stable (use --upgrade to get newer)
///
/// Uses idna which has releases at:
/// - 3.6: 2023-11-25
/// - 3.7: 2024-04-11
#[test]
fn lock_exclude_newer_older_vs_newer() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["idna"]
        "#,
    )?;

    // Start with a cutoff that allows idna 3.7 (released 2024-04-11)
    // 2 weeks before 2024-05-01 is 2024-04-17, which is AFTER idna 3.7 release
    let current_timestamp = "2024-05-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("2 weeks"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert!(
        lock.contains("version = \"3.7\""),
        "Expected idna 3.7 in lockfile"
    );

    // Now make exclude-newer OLDER (more restrictive): 3 weeks back from 2024-05-01 is 2024-04-10
    // This is BEFORE idna 3.7 release (2024-04-11), so 3.7 becomes INVALID and must be replaced
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("3 weeks"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolving despite existing lockfile due to change of exclude newer span from `P2W` to `P3W`
    Resolved 2 packages in [TIME]
    Updated idna v3.7 -> v3.6
    ");

    let lock = context.read("uv.lock");
    assert!(
        lock.contains("version = \"3.6\""),
        "Expected idna 3.6 in lockfile after downgrade"
    );

    // Now make exclude-newer NEWER (less restrictive): back to 2 weeks (2024-04-17)
    // This allows idna 3.7 again, but existing version (3.6) is still valid so it stays
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("2 weeks"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolving despite existing lockfile due to change of exclude newer span from `P3W` to `P2W`
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert!(
        lock.contains("version = \"3.6\""),
        "Expected idna 3.6 to stay stable without --upgrade"
    );

    // With --upgrade, should now get idna 3.7 since the constraint allows it
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("2 weeks")
        .arg("--upgrade"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Updated idna v3.6 -> v3.7
    ");

    let lock = context.read("uv.lock");
    assert!(
        lock.contains("version = \"3.7\""),
        "Expected idna 3.7 after --upgrade"
    );

    Ok(())
}

/// Lock with a relative exclude-newer-package value.
///
/// Uses idna which has releases at:
/// - 3.6: 2023-11-25
/// - 3.7: 2024-04-11
#[test]
fn lock_exclude_newer_package_relative() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["idna"]
        "#,
    )?;

    // 3 weeks before 2024-05-01 is 2024-04-10, which is before idna 3.7 (released 2024-04-11).
    let current_timestamp = "2024-05-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer-package")
        .arg("idna=3 weeks"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    // Should resolve to idna 3.6 (released 2023-11-25, before cutoff of 2024-04-10)
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]

    [options.exclude-newer-package]
    idna = { timestamp = "2024-04-10T00:00:00Z", span = "P3W" }

    [[package]]
    name = "idna"
    version = "3.6"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "idna" },
    ]

    [package.metadata]
    requires-dist = [{ name = "idna" }]
    "#);

    // Changing the current time should not result in a new lockfile
    let later_timestamp = "2024-06-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, later_timestamp)
        .arg("--exclude-newer-package")
        .arg("idna=3 weeks")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Changing the span to 2 weeks should cause a new resolution.
    // 2 weeks before 2024-05-01 is 2024-04-17, which is after idna 3.7 (released 2024-04-11).
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer-package")
        .arg("idna=2 weeks")
        .arg("--upgrade"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolving despite existing lockfile due to change of exclude newer span from `P3W` to `P2W` for package `idna`
    Resolved 2 packages in [TIME]
    Updated idna v3.6 -> v3.7
    ");

    // Both `exclude-newer-package` values in the lockfile should be changed, and we should now have idna 3.7
    let lock = context.read("uv.lock");
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]

    [options.exclude-newer-package]
    idna = { timestamp = "2024-04-17T00:00:00Z", span = "P2W" }

    [[package]]
    name = "idna"
    version = "3.7"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/21/ed/f86a79a07470cb07819390452f178b3bef1d375f2ec021ecfc709fc7cf07/idna-3.7.tar.gz", hash = "sha256:028ff3aadf0609c1fd278d8ea3089299412a7a8b9bd005dd08b9f8285bcb5cfc", size = 189575, upload-time = "2024-04-11T03:34:43.276Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/e5/3e/741d8c82801c347547f8a2a06aa57dbb1992be9e948df2ea0eda2c8b79e8/idna-3.7-py3-none-any.whl", hash = "sha256:82fee1fc78add43492d3a1898bfa6d8a904cc97d8427f683ed8e798d07761aa0", size = 66836, upload-time = "2024-04-11T03:34:41.447Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "idna" },
    ]

    [package.metadata]
    requires-dist = [{ name = "idna" }]
    "#);

    // Similarly, using something like `--upgrade` should cause a new resolution
    let current_timestamp = "2024-06-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer-package")
        .arg("idna=2 weeks")
        .arg("--upgrade"), @"
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
    idna = { timestamp = "2024-05-18T00:00:00Z", span = "P2W" }

    [[package]]
    name = "idna"
    version = "3.7"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/21/ed/f86a79a07470cb07819390452f178b3bef1d375f2ec021ecfc709fc7cf07/idna-3.7.tar.gz", hash = "sha256:028ff3aadf0609c1fd278d8ea3089299412a7a8b9bd005dd08b9f8285bcb5cfc", size = 189575, upload-time = "2024-04-11T03:34:43.276Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/e5/3e/741d8c82801c347547f8a2a06aa57dbb1992be9e948df2ea0eda2c8b79e8/idna-3.7-py3-none-any.whl", hash = "sha256:82fee1fc78add43492d3a1898bfa6d8a904cc97d8427f683ed8e798d07761aa0", size = 66836, upload-time = "2024-04-11T03:34:41.447Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "idna" },
    ]

    [package.metadata]
    requires-dist = [{ name = "idna" }]
    "#);

    Ok(())
}

/// Lock with a relative exclude-newer value from the `pyproject.toml`.
///
/// Uses idna which has releases at:
/// - 3.6: 2023-11-25
/// - 3.7: 2024-04-11
#[test]
fn lock_exclude_newer_relative_pyproject() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["idna"]

        [tool.uv]
        exclude-newer = "3 weeks"
        "#,
    )?;

    // 3 weeks before 2024-05-01 is 2024-04-10, which is before idna 3.7 (released 2024-04-11).
    let current_timestamp = "2024-05-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    // Should resolve to idna 3.6 (released 2023-11-25, before cutoff of 2024-04-10)
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-04-10T00:00:00Z"
    exclude-newer-span = "P3W"

    [[package]]
    name = "idna"
    version = "3.6"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "idna" },
    ]

    [package.metadata]
    requires-dist = [{ name = "idna" }]
    "#);

    Ok(())
}

/// Lock with a relative exclude-newer-package value from the `pyproject.toml`.
///
/// Uses idna which has releases at:
/// - 3.6: 2023-11-25
/// - 3.7: 2024-04-11
#[test]
fn lock_exclude_newer_package_relative_pyproject() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["idna"]

        [tool.uv]
        exclude-newer-package = { idna = "3 weeks" }
        "#,
    )?;

    // 3 weeks before 2024-05-01 is 2024-04-10, which is before idna 3.7 (released 2024-04-11).
    let current_timestamp = "2024-05-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    // Should resolve to idna 3.6 (released 2023-11-25, before cutoff of 2024-04-10)
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]

    [options.exclude-newer-package]
    idna = { timestamp = "2024-04-10T00:00:00Z", span = "P3W" }

    [[package]]
    name = "idna"
    version = "3.6"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "idna" },
    ]

    [package.metadata]
    requires-dist = [{ name = "idna" }]
    "#);

    Ok(())
}

/// Lock with both global and per-package relative exclude-newer values.
///
/// Uses idna which has releases at:
/// - 3.6: 2023-11-25
/// - 3.7: 2024-04-11
///
/// And typing-extensions which has releases at:
/// - 4.10.0: 2024-02-25
/// - 4.11.0: 2024-04-05
#[test]
fn lock_exclude_newer_relative_global_and_package() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["idna", "typing-extensions"]
        "#,
    )?;

    // Use a fixed timestamp so the test is reproducible.
    // Current time: 2024-05-01
    // Global: 3 weeks back = 2024-04-10 (before idna 3.7 released 2024-04-11) → idna 3.6
    // Per-package: 2 weeks back = 2024-04-17 (after typing-extensions 4.11.0 released 2024-04-05) → typing-extensions 4.11.0
    let current_timestamp = "2024-05-01T00:00:00Z";

    // Lock with both global exclude-newer and package-specific override using relative durations
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("3 weeks")
        .arg("--exclude-newer-package")
        .arg("typing-extensions=2 weeks"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    // idna 3.6 (global cutoff 2024-04-10 is before 3.7 release on 2024-04-11)
    // typing-extensions 4.11.0 (per-package cutoff 2024-04-17 is after 4.11.0 release on 2024-04-05)
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-04-10T00:00:00Z"
    exclude-newer-span = "P3W"

    [options.exclude-newer-package]
    typing-extensions = { timestamp = "2024-04-17T00:00:00Z", span = "P2W" }

    [[package]]
    name = "idna"
    version = "3.6"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "idna" },
        { name = "typing-extensions" },
    ]

    [package.metadata]
    requires-dist = [
        { name = "idna" },
        { name = "typing-extensions" },
    ]

    [[package]]
    name = "typing-extensions"
    version = "4.11.0"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/f6/f3/b827b3ab53b4e3d8513914586dcca61c355fa2ce8252dea4da56e67bf8f2/typing_extensions-4.11.0.tar.gz", hash = "sha256:83f085bd5ca59c80295fc2a82ab5dac679cbe02b9f33f7d83af68e241bea51b0", size = 78744, upload-time = "2024-04-05T12:35:47.093Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/01/f3/936e209267d6ef7510322191003885de524fc48d1b43269810cd589ceaf5/typing_extensions-4.11.0-py3-none-any.whl", hash = "sha256:c1f94d72897edaf4ce775bb7558d5b79d8126906a14ea5ed1635921406c0387a", size = 34698, upload-time = "2024-04-05T12:35:44.388Z" },
    ]
    "#);

    // Changing the current time should not invalidate the lockfile
    let later_timestamp = "2024-07-01T00:00:00Z";
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, later_timestamp)
        .arg("--exclude-newer")
        .arg("3 weeks")
        .arg("--exclude-newer-package")
        .arg("typing-extensions=2 weeks")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    // Changing the global span to 2 weeks should cause a new resolution.
    // 2 weeks before 2024-05-01 is 2024-04-17 (after idna 3.7 released 2024-04-11) → idna 3.7
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("2 weeks")
        .arg("--exclude-newer-package")
        .arg("typing-extensions=2 weeks")
        .arg("--upgrade"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolving despite existing lockfile due to change of exclude newer span from `P3W` to `P2W`
    Resolved 3 packages in [TIME]
    Updated idna v3.6 -> v3.7
    ");

    // Changing the package-specific span should also invalidate the lockfile
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("2 weeks")
        .arg("--exclude-newer-package")
        .arg("typing-extensions=3 days"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolving despite existing lockfile due to change of exclude newer span from `P2W` to `P3D` for package `typing-extensions`
    Resolved 3 packages in [TIME]
    ");

    // Use an absolute global value and relative package value
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("2024-05-20T00:00:00Z")
        .arg("--exclude-newer-package")
        .arg("typing-extensions=2 weeks"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolving despite existing lockfile due to removal of exclude newer span
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    // idna 3.7 (absolute cutoff 2024-05-20 is after 3.7 release on 2024-04-11)
    // typing-extensions 4.11.0 (relative cutoff 2024-04-17)
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-05-20T00:00:00Z"

    [options.exclude-newer-package]
    typing-extensions = { timestamp = "2024-04-17T00:00:00Z", span = "P2W" }

    [[package]]
    name = "idna"
    version = "3.7"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/21/ed/f86a79a07470cb07819390452f178b3bef1d375f2ec021ecfc709fc7cf07/idna-3.7.tar.gz", hash = "sha256:028ff3aadf0609c1fd278d8ea3089299412a7a8b9bd005dd08b9f8285bcb5cfc", size = 189575, upload-time = "2024-04-11T03:34:43.276Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/e5/3e/741d8c82801c347547f8a2a06aa57dbb1992be9e948df2ea0eda2c8b79e8/idna-3.7-py3-none-any.whl", hash = "sha256:82fee1fc78add43492d3a1898bfa6d8a904cc97d8427f683ed8e798d07761aa0", size = 66836, upload-time = "2024-04-11T03:34:41.447Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "idna" },
        { name = "typing-extensions" },
    ]

    [package.metadata]
    requires-dist = [
        { name = "idna" },
        { name = "typing-extensions" },
    ]

    [[package]]
    name = "typing-extensions"
    version = "4.11.0"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/f6/f3/b827b3ab53b4e3d8513914586dcca61c355fa2ce8252dea4da56e67bf8f2/typing_extensions-4.11.0.tar.gz", hash = "sha256:83f085bd5ca59c80295fc2a82ab5dac679cbe02b9f33f7d83af68e241bea51b0", size = 78744, upload-time = "2024-04-05T12:35:47.093Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/01/f3/936e209267d6ef7510322191003885de524fc48d1b43269810cd589ceaf5/typing_extensions-4.11.0-py3-none-any.whl", hash = "sha256:c1f94d72897edaf4ce775bb7558d5b79d8126906a14ea5ed1635921406c0387a", size = 34698, upload-time = "2024-04-05T12:35:44.388Z" },
    ]
    "#);

    // Use a relative global value and absolute package value
    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, current_timestamp)
        .arg("--exclude-newer")
        .arg("3 weeks")
        .arg("--exclude-newer-package")
        .arg("typing-extensions=2024-04-01T00:00:00Z"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolving despite existing lockfile due to addition of exclude newer span `P3W`
    Resolved 3 packages in [TIME]
    Updated idna v3.7 -> v3.6
    Updated typing-extensions v4.11.0 -> v4.10.0
    ");

    let lock = context.read("uv.lock");
    // idna 3.6 (relative cutoff 2024-04-10 is before 3.7 release on 2024-04-11)
    // typing-extensions 4.10.0 (absolute cutoff 2024-04-01 is before 4.11.0 release on 2024-04-05)
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-04-10T00:00:00Z"
    exclude-newer-span = "P3W"

    [options.exclude-newer-package]
    typing-extensions = "2024-04-01T00:00:00Z"

    [[package]]
    name = "idna"
    version = "3.6"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
    ]

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    dependencies = [
        { name = "idna" },
        { name = "typing-extensions" },
    ]

    [package.metadata]
    requires-dist = [
        { name = "idna" },
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
    let context = uv_test::test_context!("3.12");
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
        .arg("1 day"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("30days"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolving despite existing lockfile due to change of exclude newer span from `P1D` to `P30D`
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("P1D"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolving despite existing lockfile due to change of exclude newer span from `P30D` to `P1D`
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("1 week"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolving despite existing lockfile due to change of exclude newer span from `P1D` to `P1W`
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("1 week ago"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolving despite existing lockfile due to change of exclude newer span from `P1W` to `-P1W`
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--exclude-newer")
        .arg("3 months"), @"
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
        .arg("2 months ago"), @"
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
        .arg("1 year"), @"
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
        .arg("1 year ago"), @"
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
        .arg("invalid span"), @"
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
    error: invalid value 'P4Z' for '--exclude-newer <EXCLUDE_NEWER>': `P4Z` could not be parsed as an ISO 8601 duration: expected to find date unit designator suffix (`Y`, `M`, `W` or `D`), but found `Z` instead

    For more information, try '--help'.
    "#);

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--exclude-newer")
        .arg("2006-12-02T02:07:43"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolving despite existing lockfile due to removal of exclude newer span
      × No solution found when resolving dependencies:
      ╰─▶ Because there are no versions of iniconfig and iniconfig==2.0.0 was published after the exclude newer time, we can conclude that all versions of iniconfig cannot be used.
          And because your project depends on iniconfig, we can conclude that your project's requirements are unsatisfiable.
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--exclude-newer")
        .arg("12/02/2006"), @"
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
    error: invalid value '2 weak' for '--exclude-newer <EXCLUDE_NEWER>': `2 weak` could not be parsed as a duration: failed to parse input in the "friendly" duration format: parsed value 'P2W', but unparsed input "eak" remains (expected no unparsed input)

    For more information, try '--help'.
    "#);

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--exclude-newer")
        .arg("30"), @"
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
    error: invalid value '1000000 years' for '--exclude-newer <EXCLUDE_NEWER>': `1000000 years` could not be parsed as a duration: failed to parse input in the "friendly" duration format: failed to set value for year unit on span: parameter 'years' with value 1000000 is not in the required range of -19998..=19998

    For more information, try '--help'.
    "#);

    Ok(())
}

/// Lock with various relative exclude newer value formats in a `pyproject.toml`.
#[test]
fn lock_exclude_newer_relative_values_pyproject() -> Result<()> {
    let context = uv_test::test_context!("3.12");
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
      `2 foos` could not be parsed as a duration: failed to parse input in the "friendly" duration format: expected to find unit designator suffix (e.g., `years` or `secs`) after parsing integer

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
      `P4Z` could not be parsed as an ISO 8601 duration: expected to find date unit designator suffix (`Y`, `M`, `W` or `D`), but found `Z` instead

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

use anyhow::Result;
use assert_fs::fixture::{FileWriteStr, PathChild};
use insta::assert_snapshot;
use uv_static::EnvVars;

use uv_test::uv_snapshot;

/// Lock with `exclude-newer` and `allow-bypass = ["direct-pinned"]`.
///
/// Uses idna which has releases at:
/// - 3.6: 2023-11-25
/// - 3.7: 2024-04-11
///
/// When a direct dependency pins `idna==3.7` and `allow-bypass` includes `direct-pinned`,
/// exclude-newer should be bypassed for that package, allowing 3.7 to be resolved
/// despite the timestamp cutoff.
#[test]
fn lock_exclude_newer_allow_bypass_direct_pinned() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    // Pin idna==3.7 as a direct dependency with `allow-bypass = ["direct-pinned"]`.
    // The exclude-newer timestamp is set before idna 3.7 was released (2024-04-11),
    // but the bypass should allow it through.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["idna==3.7"]

        [tool.uv]
        exclude-newer = { timestamp = "2024-03-01T00:00:00Z", allow-bypass = ["direct-pinned"] }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    // Should resolve to idna 3.7 despite the exclude-newer cutoff of 2024-03-01,
    // because idna is a direct dependency pinned with `==`.
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-01T00:00:00Z"
    exclude-newer-allow-bypass = ["direct-pinned"]

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
    requires-dist = [{ name = "idna", specifier = "==3.7" }]
    "#);

    Ok(())
}

/// Without `allow-bypass`, the same pinned dependency should be excluded.
#[test]
fn lock_exclude_newer_no_bypass_direct_pinned() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    // Pin idna==3.7 but without allow-bypass. Should fail because 3.7 is excluded.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["idna==3.7"]

        [tool.uv]
        exclude-newer = "2024-03-01T00:00:00Z"
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there is no version of idna==3.7 and your project depends on idna==3.7, we can conclude that your project's requirements are unsatisfiable.
    ");

    Ok(())
}

/// Non-pinned direct dependencies should still be filtered by exclude-newer,
/// even when `allow-bypass = ["direct-pinned"]` is set.
#[test]
fn lock_exclude_newer_allow_bypass_non_pinned() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    // Use a version range for idna (not ==), so the bypass should NOT apply.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["idna>=3.0"]

        [tool.uv]
        exclude-newer = { timestamp = "2024-03-01T00:00:00Z", allow-bypass = ["direct-pinned"] }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    // Should resolve to idna 3.6 (the latest version before the cutoff),
    // NOT 3.7, because `>=3.0` is not an `==` pin.
    assert_snapshot!(lock, @r#"
    version = 1
    revision = 3
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-01T00:00:00Z"
    exclude-newer-allow-bypass = ["direct-pinned"]

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
    requires-dist = [{ name = "idna", specifier = ">=3.0" }]
    "#);

    Ok(())
}

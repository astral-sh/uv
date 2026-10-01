use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
#[cfg(feature = "test-universal")]
use indoc::formatdoc;
use indoc::indoc;
use insta::assert_snapshot;

use uv_lock::Lock;
use uv_test::{diff_snapshot, uv_snapshot};

/// Upgrade an existing lockfile without silently accepting it under `--locked`.
#[test]
fn lockfile_v2_upgrade() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        default-groups = ["docs"]

        [tool.uv.sources]
        child = { path = "child" }

        [dependency-groups]
        docs = ["child"]
    "#})?;
    context
        .temp_dir
        .child("child/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "child"
        version = "1.0.0"

        [tool.uv]
        package = false
    "#})?;

    context.lock().arg("--offline").assert().success();
    let original = context.read("uv.lock");
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked", "--preview-features", "lockfile-v2"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    assert_eq!(context.read("uv.lock"), original);

    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Resolved 2 packages in [TIME]
    ");
    let upgraded = context.read("uv.lock");
    // The v2 field names must be handled by the canonical parser, without a TOML fallback.
    assert_eq!(Lock::from_canonical_toml(&upgraded)?.to_toml()?, upgraded);
    assert_snapshot!(upgraded, @r#"
    version = 2
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [[package]]
    name = "child"
    version = "1.0.0"
    source = { virtual = "child" }

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    default-groups = ["docs"]

    [package.dependency-groups]
    docs = [
        "child",
    ]

    [package.metadata]

    [package.metadata.dependency-groups]
    docs = [{ name = "child", virtual = "child" }]
    "#);

    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Resolved 2 packages in [TIME]
    ");
    assert_eq!(context.read("uv.lock"), upgraded);

    // Frozen reads require the feature too, including when no resolution is needed.
    uv_snapshot!(context.filters(), context.sync().args(["--offline", "--frozen"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The lockfile at `uv.lock` uses an unsupported schema version (v2, but versions up to v1 are supported). Downgrade to a compatible uv version, or remove the `uv.lock` prior to running `uv lock` or `uv sync`.
    ");
    uv_snapshot!(context.filters(), context.sync().args(["--offline", "--frozen", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Checked in [TIME]
    ");
    assert_eq!(context.read("uv.lock"), upgraded);

    Ok(())
}

/// Read the feature from project configuration when creating a lockfile.
#[test]
fn lockfile_v2_config() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        preview-features = ["lockfile-v2"]
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    ");
    assert_snapshot!(context.read("uv.lock"), @r#"
    version = 2
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [[package]]
    name = "project"
    version = "0.1.0"
    source = { virtual = "." }
    "#);
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Resolved 1 package in [TIME]
    ");

    Ok(())
}

/// Apply the format feature to script lockfiles, including noncanonical TOML reads.
#[test]
fn lockfile_v2_script() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context.temp_dir.child("script.py").write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = []
        # ///
    "#})?;

    uv_snapshot!(context.filters(), context.lock().args(["--script", "script.py", "--offline"]).env("UV_PREVIEW_FEATURES", "lockfile-v2"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved in [TIME]
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    ");
    assert_snapshot!(context.read("script.py.lock"), @r#"
    version = 2
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"
    "#);

    context
        .temp_dir
        .child("script.py.lock")
        .write_str(indoc! {r"
        # Hand-edited lockfile using noncanonical TOML.
        version = 2
        requires-python = '>=3.12'

        [options]
        exclude-newer = '2024-03-25T00:00:00Z'
    "})?;
    uv_snapshot!(context.filters(), context.lock().args(["--script", "script.py", "--offline", "--locked", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Resolved in [TIME]
    ");

    Ok(())
}

/// Enabling v2 still rejects unknown schema versions, even when their contents cannot be parsed.
#[test]
fn lockfile_v2_unsupported_version() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
    "#})?;
    context.temp_dir.child("uv.lock").write_str(indoc! {r#"
        version = 3
        requires-python = ">=3.12"
    "#})?;
    uv_snapshot!(context.filters(), context.lock().args(["--frozen", "--preview-features", "lockfile-v2"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    error: The lockfile at `uv.lock` uses an unsupported schema version (v3, but versions up to v2 are supported). Downgrade to a compatible uv version, or remove the `uv.lock` prior to running `uv lock` or `uv sync`.
    ");

    context.temp_dir.child("uv.lock").write_str(indoc! {r"
        version = 3
        requires-python = false
    "})?;
    uv_snapshot!(context.filters(), context.lock().args(["--frozen", "--preview-features", "lockfile-v2"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    error: Failed to parse `uv.lock`, which uses an unsupported schema version (v3, but versions up to v2 are supported). Downgrade to a compatible uv version, or remove the `uv.lock` prior to running `uv lock` or `uv sync`.
      cause: TOML parse error at line 2, column 19
               |
             2 | requires-python = false
               |                   ^^^^^
             invalid type: boolean `false`, expected a string
    ");

    Ok(())
}

/// Relative cutoffs omit compatibility timestamps while absolute and disabled overrides retain
/// their meaning.
#[test]
fn lockfile_v2_exclude_newer() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        exclude-newer = "7 days"
        exclude-newer-package = { relative = "7 days", absolute = "2024-01-01T00:00:00Z", disabled = false }
    "#})?;
    context
        .lock()
        .env_remove("UV_EXCLUDE_NEWER")
        .arg("--offline")
        .assert()
        .success();
    let original = context.read("uv.lock");
    context
        .lock()
        .env_remove("UV_EXCLUDE_NEWER")
        .args(["--offline", "--preview-features", "lockfile-v2"])
        .assert()
        .success();
    let upgraded = context.read("uv.lock");
    assert_snapshot!(diff_snapshot(&original, &upgraded, 3), @r#"
    --- old
    +++ new
    @@ -1,15 +1,13 @@
    -version = 1
    -revision = 5
    +version = 2
     requires-python = ">=3.12"

     [options]
    -exclude-newer = "0001-01-01T00:00:00Z" # This has no effect and is included for backwards compatibility when using relative exclude-newer values.
     exclude-newer-span = "P7D"

     [options.exclude-newer-package]
     absolute = "2024-01-01T00:00:00Z"
     disabled = false
    -relative = { timestamp = "0001-01-01T00:00:00Z", span = "P7D" }
    +relative = { span = "P7D" }

     [[package]]
     name = "project"
    "#);
    assert_eq!(Lock::from_canonical_toml(&upgraded)?.to_toml()?, upgraded);
    assert_eq!(toml::from_str::<Lock>(&upgraded)?.to_toml()?, upgraded);
    context
        .lock()
        .env_remove("UV_EXCLUDE_NEWER")
        .args([
            "--offline",
            "--locked",
            "--no-cache",
            "--preview-features",
            "lockfile-v2",
        ])
        .assert()
        .success();
    assert_eq!(context.read("uv.lock"), upgraded);

    // An empty override must not silently disable the cutoff.
    context
        .temp_dir
        .child("uv.lock")
        .write_str(&upgraded.replace("{ span = \"P7D\" }", "{}"))?;
    uv_snapshot!(context.filters(), context.lock().args(["--frozen", "--preview-features", "lockfile-v2"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    error: Failed to parse `uv.lock`
      cause: TOML parse error at line 4, column 1
               |
             4 | [options]
               | ^^^^^^^^^
             expected `timestamp` or `span`
    ");
    Ok(())
}

/// Name-only edges use strings in each dependency section, while extras, markers and ambiguous
/// package identities keep their tables.
#[test]
#[cfg(feature = "test-universal")]
fn lockfile_v2_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["base[feature]", "conditional; sys_platform == 'win32'", "forked", "plain"]

        [project.optional-dependencies]
        feature = ["plain"]

        [dependency-groups]
        dev = ["plain"]

        [tool.uv.sources]
        base = { path = "base" }
        conditional = { path = "conditional" }
        plain = { path = "plain" }
        forked = [
            { path = "forked-v1", marker = "sys_platform == 'win32'" },
            { path = "forked-v2", marker = "sys_platform != 'win32'" },
        ]
    "#})?;
    for (path, name, version) in [
        ("conditional", "conditional", "1.0.0"),
        ("plain", "plain", "1.0.0"),
        ("forked-v1", "forked", "1.0.0"),
        ("forked-v2", "forked", "2.0.0"),
    ] {
        context
            .temp_dir
            .child(path)
            .child("pyproject.toml")
            .write_str(&formatdoc! {r#"
            [project]
            name = "{name}"
            version = "{version}"

            [tool.uv]
            package = false
        "#})?;
    }
    context
        .temp_dir
        .child("base/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "base"
        version = "1.0.0"

        [project.optional-dependencies]
        feature = ["plain"]

        [tool.uv]
        package = false

        [tool.uv.sources]
        plain = { path = "../plain" }
    "#})?;
    context.lock().arg("--offline").assert().success();
    let original = context.read("uv.lock");
    context
        .lock()
        .args(["--offline", "--preview-features", "lockfile-v2"])
        .assert()
        .success();
    let upgraded = context.read("uv.lock");
    assert_snapshot!(diff_snapshot(&original, &upgraded, 5), @r#"
    --- old
    +++ new
    @@ -1,7 +1,6 @@
    -version = 1
    -revision = 5
    +version = 2
     requires-python = ">=3.12"
     resolution-markers = [
         "sys_platform == 'win32'",
         "sys_platform != 'win32'",
     ]
    @@ -14,11 +13,11 @@
     version = "1.0.0"
     source = { virtual = "base" }

     [package.optional-dependencies]
     feature = [
    -    { name = "plain" },
    +    "plain",
     ]

     [package.metadata]
     requires-dist = [{ name = "plain", marker = "extra == 'feature'", virtual = "plain" }]
     provides-extras = ["feature"]
    @@ -52,25 +51,25 @@
     [[package]]
     name = "project"
     version = "0.1.0"
     source = { virtual = "." }
     dependencies = [
    -    { name = "base", extra = ["feature"] },
    +    { name = "base", extras = ["feature"] },
         { name = "conditional", marker = "sys_platform == 'win32'" },
         { name = "forked", version = "1.0.0", source = { virtual = "forked-v1" }, marker = "sys_platform == 'win32'" },
         { name = "forked", version = "2.0.0", source = { virtual = "forked-v2" }, marker = "sys_platform != 'win32'" },
    -    { name = "plain" },
    +    "plain",
     ]

     [package.optional-dependencies]
     feature = [
    -    { name = "plain" },
    +    "plain",
     ]

    -[package.dev-dependencies]
    +[package.dependency-groups]
     dev = [
    -    { name = "plain" },
    +    "plain",
     ]

     [package.metadata]
     requires-dist = [
         { name = "base", extras = ["feature"], virtual = "base" },
    @@ -80,7 +79,7 @@
         { name = "plain", virtual = "plain" },
         { name = "plain", marker = "extra == 'feature'", virtual = "plain" },
     ]
     provides-extras = ["feature"]

    -[package.metadata.requires-dev]
    +[package.metadata.dependency-groups]
     dev = [{ name = "plain", virtual = "plain" }]
    "#);
    assert_eq!(Lock::from_canonical_toml(&upgraded)?.to_toml()?, upgraded);
    assert_eq!(toml::from_str::<Lock>(&upgraded)?.to_toml()?, upgraded);
    context
        .lock()
        .args([
            "--offline",
            "--locked",
            "--no-cache",
            "--preview-features",
            "lockfile-v2",
        ])
        .assert()
        .success();
    uv_snapshot!(context.filters(), context.tree().args(["--frozen", "--universal", "--all-groups", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    project v0.1.0
    ├── base[feature] v1.0.0
    │   └── plain v1.0.0 (extra: feature)
    ├── conditional v1.0.0
    ├── forked v1.0.0
    ├── forked v2.0.0
    ├── plain v1.0.0
    ├── plain v1.0.0 (extra: feature)
    └── plain v1.0.0 (group: dev)

    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    ");

    // Legacy edge spelling remains readable in v2, including through the TOML fallback.
    let legacy = upgraded.replace(
        "{ name = \"base\", extras = [\"feature\"] }",
        "{ name = \"base\", extra = [\"feature\"] }",
    );
    assert_eq!(Lock::from_canonical_toml(&legacy)?.to_toml()?, upgraded);
    assert_eq!(toml::from_str::<Lock>(&legacy)?.to_toml()?, upgraded);
    Ok(())
}

/// Python requirements live with their groups for packages and projectless roots, including
/// empty groups and requirements inherited through group inclusion.
#[test]
#[cfg(feature = "test-universal")]
fn lockfile_v2_group_requires_python() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["member"]

        [tool.uv.sources]
        member = { workspace = true }

        [dependency-groups]
        docs = ["member"]
        empty = []
        plain = []
        inherited = [{ include-group = "docs" }]

        [tool.uv.dependency-groups]
        docs = { requires-python = ">=3.12" }
        empty = { requires-python = ">=3.13" }
        inherited = { requires-python = "<3.14" }
    "#})?;
    context
        .temp_dir
        .child("member/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "member"
        version = "1.0.0"
        requires-python = ">=3.12"

        [tool.uv]
        package = false

        [tool.uv.sources]
        leaf = { path = "../leaf" }

        [dependency-groups]
        docs = ["leaf"]
        empty = []
        plain = []
        inherited = [{ include-group = "docs" }]

        [tool.uv.dependency-groups]
        docs = { requires-python = ">=3.12" }
        empty = { requires-python = ">=3.13" }
        inherited = { requires-python = "<3.14" }
    "#})?;
    context
        .temp_dir
        .child("leaf/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "leaf"
        version = "1.0.0"

        [tool.uv]
        package = false
    "#})?;
    context.lock().arg("--offline").assert().success();
    let original = context.read("uv.lock");
    context
        .lock()
        .args(["--offline", "--preview-features", "lockfile-v2"])
        .assert()
        .success();
    let upgraded = context.read("uv.lock");
    assert_snapshot!(diff_snapshot(&original, &upgraded, 3), @r#"
    --- old
    +++ new
    @@ -1,5 +1,4 @@
    -version = 1
    -revision = 5
    +version = 2
     requires-python = ">=3.12"
     resolution-markers = [
         "python_full_version >= '3.14'",
    @@ -15,15 +14,10 @@
     ]

     [manifest.dependency-groups]
    -docs = [{ name = "member", marker = "python_full_version >= '3.12'", virtual = "member" }]
    -empty = []
    -inherited = [{ name = "member", marker = "python_full_version >= '3.12' and python_full_version < '3.14'", virtual = "member" }]
    +docs = { requires-python = ">=3.12", dependencies = [{ name = "member", marker = "python_full_version >= '3.12'", virtual = "member" }] }
    +empty = { requires-python = ">=3.13", dependencies = [] }
    +inherited = { requires-python = ">=3.12,<3.14", dependencies = [{ name = "member", marker = "python_full_version >= '3.12' and python_full_version < '3.14'", virtual = "member" }] }
     plain = []
    -
    -[manifest.group-requires-python]
    -docs = ">=3.12"
    -empty = ">=3.13"
    -inherited = ">=3.12,<3.14"

     [[package]]
     name = "leaf"
    @@ -35,22 +29,14 @@
     version = "1.0.0"
     source = { virtual = "member" }

    -[package.dev-dependencies]
    -docs = [
    -    { name = "leaf" },
    -]
    -inherited = [
    -    { name = "leaf", marker = "python_full_version < '3.14'" },
    -]
    -
    -[package.group-requires-python]
    -docs = ">=3.12"
    -empty = ">=3.13"
    -inherited = ">=3.12,<3.14"
    +[package.dependency-groups]
    +docs = { requires-python = ">=3.12", dependencies = ["leaf"] }
    +empty = { requires-python = ">=3.13", dependencies = [] }
    +inherited = { requires-python = ">=3.12,<3.14", dependencies = [{ name = "leaf", marker = "python_full_version < '3.14'" }] }

     [package.metadata]

    -[package.metadata.requires-dev]
    +[package.metadata.dependency-groups]
     docs = [{ name = "leaf", marker = "python_full_version >= '3.12'", virtual = "leaf" }]
     empty = []
     inherited = [{ name = "leaf", marker = "python_full_version >= '3.12' and python_full_version < '3.14'", virtual = "leaf" }]
    "#);
    assert_eq!(Lock::from_canonical_toml(&upgraded)?.to_toml()?, upgraded);
    assert_eq!(toml::from_str::<Lock>(&upgraded)?.to_toml()?, upgraded);
    context
        .lock()
        .args([
            "--offline",
            "--locked",
            "--no-cache",
            "--preview-features",
            "lockfile-v2",
        ])
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.sync().args(["--frozen", "--offline", "--dry-run", "--python", "3.12", "--group", "empty", "--preview-features", "lockfile-v2"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    error: The requested interpreter resolved to Python 3.12.[X], which is incompatible with the project's Python requirement: `>=3.13`.
    The following `requires-python` declarations do not permit this version:
    - member:empty: >=3.13
    - workspace:empty: >=3.13
    ");

    // Changing an inline group requirement must still make the lock stale.
    let pyproject = context
        .read("member/pyproject.toml")
        .replace(">=3.13", ">=3.12");
    context
        .temp_dir
        .child("member/pyproject.toml")
        .write_str(&pyproject)?;
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked", "--preview-features", "lockfile-v2"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    Ok(())
}

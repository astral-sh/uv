#[cfg(all(feature = "test-git", feature = "test-universal"))]
use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
#[cfg(feature = "test-universal")]
use indoc::formatdoc;
use indoc::indoc;
use insta::assert_snapshot;
#[cfg(all(feature = "test-git", feature = "test-universal"))]
use url::Url;

use uv_lock::Lock;
use uv_test::{diff_snapshot, uv_snapshot};

/// V2 conflict locks retain extra and group selection across frozen reads and revalidation.
#[test]
#[cfg(feature = "test-universal")]
fn lockfile_v2_conflicting_extras_and_groups() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "1.0.0"
        requires-python = ">=3.12"

        [project.optional-dependencies]
        foo = ["child==1"]
        bar = ["child==2"]

        [dependency-groups]
        first = ["project[foo]"]
        second = ["project[bar]"]

        [tool.uv]
        conflicts = [[{ extra = "foo" }, { extra = "bar" }]]

        [tool.uv.sources]
        project = { workspace = true }
        child = [{ path = "one", extra = "foo" }, { path = "two", extra = "bar" }]
    "#})?;
    for (directory, version) in [("one", "1.0.0"), ("two", "2.0.0")] {
        context
            .temp_dir
            .child(directory)
            .child("pyproject.toml")
            .write_str(&formatdoc! {r#"
            [project]
            name = "child"
            version = "{version}"
            dependencies = ["leaf"]

            [tool.uv]
            package = false

            [tool.uv.sources]
            leaf = {{ path = "../leaf" }}
        "#})?;
    }
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
    context
        .lock()
        .args(["--offline", "--preview-features", "lockfile-v2"])
        .assert()
        .success();
    let lock = context.read("uv.lock");
    assert_eq!(Lock::from_canonical_toml(&lock)?.to_toml()?, lock);
    assert_eq!(toml::from_str::<Lock>(&lock)?.to_toml()?, lock);
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Resolved 4 packages in [TIME]
    ");
    assert_eq!(context.read("uv.lock"), lock);
    uv_snapshot!(context.filters(), context.sync().args(["--frozen", "--offline", "--extra", "foo", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Checked in [TIME]
    ");
    uv_snapshot!(context.filters(), context.sync().args(["--frozen", "--offline", "--extra", "bar", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Checked in [TIME]
    ");
    uv_snapshot!(context.filters(), context.sync().args(["--frozen", "--offline", "--group", "first", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Checked in [TIME]
    ");
    uv_snapshot!(context.filters(), context.sync().args(["--frozen", "--offline", "--group", "second", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Checked in [TIME]
    ");
    uv_snapshot!(context.filters(), context.sync().args(["--frozen", "--offline", "--all-extras", "--preview-features", "lockfile-v2"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    error: Extras `bar` and `foo` are incompatible with the declared conflicts: {`project[bar]`, `project[foo]`}
    ");
    // Revalidation without declaration metadata must use the same marker simplification.
    context
        .lock()
        .args([
            "--offline",
            "--preview-features",
            "lockfile-v2,lock-without-metadata",
        ])
        .assert()
        .success();
    context
        .lock()
        .args([
            "--offline",
            "--locked",
            "--preview-features",
            "lockfile-v2,lock-without-metadata",
        ])
        .assert()
        .success();
    Ok(())
}

/// Conflict discovery can provisionally visit a package that is later excluded after all
/// transitive extras have been activated. Its dependencies must be evaluated under the package's
/// reachability marker during that preliminary traversal.
#[test]
fn lockfile_v2_conflict_discovery_respects_parent_reachability() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        feature = ["x[foo]"]

        [tool.uv]
        conflicts = [[
            { package = "x", extra = "foo" },
            { package = "q", extra = "bar" },
        ]]
        "#,
    )?;

    // This is the lock shape produced when `parent`'s reachability marker is removed from its
    // outgoing edge: the edge is only valid in the context of `parent`, but conflict discovery
    // traverses `parent` before it knows that `x[foo]` makes the package unreachable.
    context.temp_dir.child("uv.lock").write_str(
        r#"
        version = 2
        requires-python = ">=3.12"
        conflicts = [[
            { package = "q", extra = "bar" },
            { package = "x", extra = "foo" },
        ]]

        [[package]]
        name = "parent"
        source = { virtual = "parent" }
        dependencies = [
            { name = "q", extras = ["bar"] },
        ]

        [[package]]
        name = "project"
        source = { virtual = "." }
        dependencies = [
            { name = "parent", marker = "extra != 'extra-1-x-foo'" },
        ]

        [package.optional-dependencies]
        feature = [
            { name = "x", extras = ["foo"] },
        ]

        [package.metadata]
        provides-extras = ["feature"]

        [[package]]
        name = "q"
        source = { virtual = "q" }

        [package.optional-dependencies]
        bar = []

        [[package]]
        name = "x"
        source = { virtual = "x" }

        [package.optional-dependencies]
        foo = []
        "#,
    )?;

    uv_snapshot!(context.filters(), context.sync()
        .arg("--extra")
        .arg("feature")
        .args(["--frozen", "--offline", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Checked in [TIME]
    ");

    Ok(())
}

/// Static and package metadata retain their Core Metadata field names.
#[test]
fn lockfile_v2_metadata_names() -> Result<()> {
    let input = indoc! {r#"
        version = 1
        requires-python = ">=3.12"

        [[manifest.dependency-metadata]]
        name = "child"
        version = "1.0.0"
        requires-dist = ["leaf>=1 ; extra == 'feature'"]
        requires-python = ">=3.12"
        provides-extras = ["feature"]

        [[manifest.dependency-metadata]]
        name = "leaf"
        version = "1.0.0"

        [[package]]
        name = "project"
        version = "1.0.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        feature = []

        [package.metadata]
        provides-extras = ["feature"]
    "#};
    let original = toml::from_str::<Lock>(input)?.to_toml()?;
    let expected = toml::from_str::<Lock>(&input.replace("version = 1\n", "version = 2\n"))?;
    let upgraded = expected.to_toml()?;
    assert_snapshot!(diff_snapshot(&original, &upgraded, 3), @r#"
    --- old
    +++ new
    @@ -1,14 +1,14 @@
    -version = 1
    +version = 2
     requires-python = ">=3.12"

    -[[manifest.dependency-metadata]]
    +[[workspace.dependency-metadata]]
     name = "child"
     version = "1.0.0"
     requires-dist = ["leaf>=1 ; extra == 'feature'"]
     requires-python = ">=3.12"
     provides-extras = ["feature"]

    -[[manifest.dependency-metadata]]
    +[[workspace.dependency-metadata]]
     name = "leaf"
     version = "1.0.0"
    "#);
    assert_eq!(Lock::from_canonical_toml(&upgraded)?, expected);
    assert_eq!(toml::from_str::<Lock>(&upgraded)?, expected);
    assert_eq!(Lock::from_canonical_toml(&upgraded)?.to_toml()?, upgraded);

    let legacy = upgraded.replace("[workspace", "[manifest");
    assert_eq!(Lock::from_canonical_toml(&legacy)?, expected);
    assert_eq!(toml::from_str::<Lock>(&legacy)?, expected);

    // The two table names are aliases, so mixing them must not merge their metadata.
    let duplicate = upgraded.replacen(
        "[[workspace.dependency-metadata]]",
        "[[manifest.dependency-metadata]]",
        1,
    );
    assert!(Lock::from_canonical_toml(&duplicate).is_err());
    assert!(toml::from_str::<Lock>(&duplicate).is_err());
    Ok(())
}

/// Partial identities resolve uniquely, including source trees without a static version.
#[test]
fn lockfile_v2_dependency_identities() -> Result<()> {
    let input = indoc! {r#"
        version = 2
        requires-python = ">=3.12"

        [[package]]
        name = "versioned"
        version = "1.0.0"
        source = { registry = "https://example.com/simple" }

        [[package]]
        name = "versioned"
        version = "2.0.0"
        source = { registry = "https://example.com/simple" }

        [[package]]
        name = "sourced"
        version = "1.0.0"
        source = { virtual = "one" }

        [[package]]
        name = "sourced"
        version = "1.0.0"
        source = { virtual = "two" }

        [[package]]
        name = "ambiguous"
        version = "1.0.0"
        source = { virtual = "one" }

        [[package]]
        name = "ambiguous"
        version = "1.0.0"
        source = { virtual = "two" }

        [[package]]
        name = "ambiguous"
        version = "2.0.0"
        source = { virtual = "one" }

        [[package]]
        name = "dynamic"
        source = { virtual = "one" }

        [[package]]
        name = "dynamic"
        source = { virtual = "two" }

        [[package]]
        name = "project"
        version = "1.0.0"
        source = { virtual = "." }
        dependencies = [
            { name = "versioned", version = "1.0.0", source = { registry = "https://example.com/simple" } },
            { name = "sourced", version = "1.0.0", source = { virtual = "one" } },
            { name = "ambiguous", version = "1.0.0", source = { virtual = "one" } },
            { name = "dynamic", source = { virtual = "one" } },
        ]
    "#};
    let expected = toml::from_str::<Lock>(input)?;
    let canonical = expected.to_toml()?;
    assert_snapshot!(canonical, @r#"
    version = 2
    requires-python = ">=3.12"

    [[package]]
    name = "ambiguous"
    version = "1.0.0"
    source = { virtual = "one" }

    [[package]]
    name = "ambiguous"
    version = "1.0.0"
    source = { virtual = "two" }

    [[package]]
    name = "ambiguous"
    version = "2.0.0"
    source = { virtual = "one" }

    [[package]]
    name = "dynamic"
    source = { virtual = "one" }

    [[package]]
    name = "dynamic"
    source = { virtual = "two" }

    [[package]]
    name = "project"
    version = "1.0.0"
    source = { virtual = "." }
    dependencies = [
        { name = "ambiguous", version = "1.0.0", source = { virtual = "one" } },
        { name = "dynamic", source = { virtual = "one" } },
        { name = "sourced", source = { virtual = "one" } },
        { name = "versioned", version = "1.0.0" },
    ]

    [[package]]
    name = "sourced"
    version = "1.0.0"
    source = { virtual = "one" }

    [[package]]
    name = "sourced"
    version = "1.0.0"
    source = { virtual = "two" }

    [[package]]
    name = "versioned"
    version = "1.0.0"
    source = { registry = "https://example.com/simple" }

    [[package]]
    name = "versioned"
    version = "2.0.0"
    source = { registry = "https://example.com/simple" }
    "#);
    assert_eq!(Lock::from_canonical_toml(&canonical)?, expected);
    assert_eq!(toml::from_str::<Lock>(&canonical)?, expected);

    // A version shared by different sources cannot identify a package on its own.
    let ambiguous = input.replace(
        r#"{ name = "ambiguous", version = "1.0.0", source = { virtual = "one" } }"#,
        r#"{ name = "ambiguous", version = "1.0.0" }"#,
    );
    assert!(toml::from_str::<Lock>(&ambiguous).is_err());
    assert!(Lock::from_canonical_toml(&ambiguous).is_err());
    Ok(())
}

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
    -exclude-newer-span = "P7D"
    +exclude-newer = { span = "P7D" }

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
        .write_str(&upgraded.replace("relative = { span = \"P7D\" }", "relative = {}"))?;
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
    -    { name = "forked", version = "1.0.0", source = { virtual = "forked-v1" }, marker = "sys_platform == 'win32'" },
    -    { name = "forked", version = "2.0.0", source = { virtual = "forked-v2" }, marker = "sys_platform != 'win32'" },
    -    { name = "plain" },
    +    { name = "forked", version = "1.0.0", marker = "sys_platform == 'win32'" },
    +    { name = "forked", version = "2.0.0", marker = "sys_platform != 'win32'" },
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
    @@ -9,22 +8,17 @@
     [options]
     exclude-newer = "2024-03-25T00:00:00Z"

    -[manifest]
    +[workspace]
     members = [
         "member",
     ]

    -[manifest.dependency-groups]
    -docs = [{ name = "member", marker = "python_full_version >= '3.12'", virtual = "member" }]
    -empty = []
    -inherited = [{ name = "member", marker = "python_full_version >= '3.12' and python_full_version < '3.14'", virtual = "member" }]
    +[workspace.dependency-groups]
    +docs = { requires-python = ">=3.12", dependencies = [{ name = "member", marker = "python_full_version >= '3.12'", virtual = "member" }] }
    +empty = { requires-python = ">=3.13", dependencies = [] }
    +inherited = { requires-python = ">=3.12,<3.14", dependencies = [{ name = "member", marker = "python_full_version >= '3.12' and python_full_version < '3.14'", virtual = "member" }] }
     plain = []

    -[manifest.group-requires-python]
    -docs = ">=3.12"
    -empty = ">=3.13"
    -inherited = ">=3.12,<3.14"
    -
     [[package]]
     name = "leaf"
     version = "1.0.0"
    @@ -35,22 +29,12 @@
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
    -
    -[package.metadata]
    +[package.dependency-groups]
    +docs = { requires-python = ">=3.12", dependencies = ["leaf"] }
    +empty = { requires-python = ">=3.13", dependencies = [] }
    +inherited = { requires-python = ">=3.12,<3.14", dependencies = [{ name = "leaf", marker = "python_full_version < '3.14'" }] }

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

/// Script requirements use the same field name as package dependency declarations.
#[test]
fn lockfile_v2_workspace_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context.temp_dir.child("script.py").write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = ["child"]
        # [tool.uv.sources]
        # child = { path = "child" }
        # ///
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
    context
        .lock()
        .args(["--script", "script.py", "--offline"])
        .assert()
        .success();
    let original = context.read("script.py.lock");
    context
        .lock()
        .args([
            "--script",
            "script.py",
            "--offline",
            "--preview-features",
            "lockfile-v2",
        ])
        .assert()
        .success();
    let upgraded = context.read("script.py.lock");
    assert_snapshot!(diff_snapshot(&original, &upgraded, 3), @r#"
    --- old
    +++ new
    @@ -1,12 +1,11 @@
    -version = 1
    -revision = 5
    +version = 2
     requires-python = ">=3.12"

     [options]
     exclude-newer = "2024-03-25T00:00:00Z"

    -[manifest]
    -requirements = [{ name = "child", virtual = "child" }]
    +[workspace]
    +dependencies = [{ name = "child", virtual = "child" }]

     [[package]]
     name = "child"
    "#);
    assert_eq!(Lock::from_canonical_toml(&upgraded)?.to_toml()?, upgraded);
    assert_eq!(toml::from_str::<Lock>(&upgraded)?.to_toml()?, upgraded);
    context
        .lock()
        .args([
            "--script",
            "script.py",
            "--offline",
            "--locked",
            "--preview-features",
            "lockfile-v2",
        ])
        .assert()
        .success();
    Ok(())
}

/// Shorthand declarations round-trip in package metadata and both forms of workspace groups,
/// without dropping any qualifiers from other requirements.
#[test]
fn lockfile_v2_requirement_shorthand() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "1.0.0"
        requires-python = ">=3.12"
    "#})?;
    let input = indoc! {r#"
        version = 2
        requires-python = ">=3.12"

        [manifest]
        dependencies = [{ name = "plain", specifier = ">=2" }]

        constraints = [{ name = "plain", specifier = "<3" }]
        overrides = [
            { name = "bounded", specifier = ">=1" },
            { package = { name = "parent", version = "1.0.0" }, dependencies = [{ name = "plain", specifier = ">=2" }] },
        ]
        build-constraints = [
            { name = "setuptools", specifier = ">=70" },
            { name = "wheel", specifier = "==0.45.1", hashes = ["sha256:1234"] },
        ]

        [manifest.dependency-groups]
        dev = [{ name = "plain", extras = ["feature"], specifier = ">=2", marker = "sys_platform == 'linux'" }]
        docs = { requires-python = ">=3.13", dependencies = [{ name = "plain", specifier = ">=2" }] }

        [[package]]
        name = "project"
        version = "1.0.0"
        source = { virtual = "." }

        [package.metadata]
        requires-dist = [
            { name = "plain" },
            { name = "bounded", specifier = ">=1" },
            { name = "combined", extras = ["foo", "bar"], specifier = ">=1,<3", marker = "sys_platform == 'linux' or sys_platform == 'darwin'" },
            { name = "conflicting", specifier = ">=2", conflict = { package = "project", extra = "foo" } },
            { name = "direct", url = "https://example.com/direct-1.0.0.tar.gz" },
            { name = "extra", extras = ["feature"] },
            { name = "group", groups = ["dev"] },
            { name = "marker", marker = "sys_platform == 'linux'" },
            { name = "index", index = "https://example.com/simple" },
            { name = "source", virtual = "child" },
        ]

        [package.metadata.dependency-groups]
        dev = [{ name = "plain", extras = ["feature"], specifier = ">=2", marker = "sys_platform == 'linux'" }]
    "#};
    let lock = toml::from_str::<Lock>(input)?.to_toml()?;
    assert_snapshot!(lock, @r#"
    version = 2
    requires-python = ">=3.12"

    [workspace]
    dependencies = ["plain>=2"]
    constraints = ["plain<3"]
    overrides = [
        { package = { name = "parent", version = "1.0.0" }, dependencies = ["plain>=2"] },
        "bounded>=1",
    ]
    build-constraints = [
        "setuptools>=70",
        { name = "wheel", specifier = "==0.45.1", hashes = ["sha256:1234"] },
    ]

    [workspace.dependency-groups]
    dev = ["plain[feature]>=2 ; sys_platform == 'linux'"]
    docs = { requires-python = ">=3.13", dependencies = ["plain>=2"] }

    [[package]]
    name = "project"
    version = "1.0.0"
    source = { virtual = "." }

    [package.metadata]
    requires-dist = [
        "bounded>=1",
        "combined[foo,bar]>=1,<3 ; sys_platform == 'darwin' or sys_platform == 'linux'",
        { name = "conflicting", specifier = ">=2", conflict = { package = "project", extra = "foo" } },
        { name = "direct", url = "https://example.com/direct-1.0.0.tar.gz" },
        "extra[feature]",
        { name = "group", groups = ["dev"] },
        { name = "index", index = "https://example.com/simple" },
        "marker ; sys_platform == 'linux'",
        "plain",
        { name = "source", virtual = "child" },
    ]

    [package.metadata.dependency-groups]
    dev = ["plain[feature]>=2 ; sys_platform == 'linux'"]
    "#);
    assert_eq!(Lock::from_canonical_toml(&lock)?.to_toml()?, lock);
    assert_eq!(toml::from_str::<Lock>(&lock)?.to_toml()?, lock);
    context.temp_dir.child("uv.lock").write_str(&lock)?;
    context
        .lock()
        .args(["--frozen", "--preview-features", "lockfile-v2"])
        .assert()
        .success();

    // V1 continues to write tables even when its reader accepts shorthand.
    let legacy = toml::from_str::<Lock>(&lock.replace("version = 2", "version = 1"))?.to_toml()?;
    assert_snapshot!(diff_snapshot(&legacy, &lock, 3), @r#"
    --- old
    +++ new
    @@ -1,24 +1,21 @@
    -version = 1
    +version = 2
     requires-python = ">=3.12"

    -[manifest]
    -requirements = [{ name = "plain", specifier = ">=2" }]
    -constraints = [{ name = "plain", specifier = "<3" }]
    +[workspace]
    +dependencies = ["plain>=2"]
    +constraints = ["plain<3"]
     overrides = [
    -    { package = { name = "parent", version = "1.0.0" }, dependencies = [{ name = "plain", specifier = ">=2" }] },
    -    { name = "bounded", specifier = ">=1" },
    +    { package = { name = "parent", version = "1.0.0" }, dependencies = ["plain>=2"] },
    +    "bounded>=1",
     ]
     build-constraints = [
    -    { name = "setuptools", specifier = ">=70" },
    +    "setuptools>=70",
         { name = "wheel", specifier = "==0.45.1", hashes = ["sha256:1234"] },
     ]

    -[manifest.dependency-groups]
    -dev = [{ name = "plain", extras = ["feature"], marker = "sys_platform == 'linux'", specifier = ">=2" }]
    -docs = [{ name = "plain", specifier = ">=2" }]
    -
    -[manifest.group-requires-python]
    -docs = ">=3.13"
    +[workspace.dependency-groups]
    +dev = ["plain[feature]>=2 ; sys_platform == 'linux'"]
    +docs = { requires-python = ">=3.13", dependencies = ["plain>=2"] }

     [[package]]
     name = "project"
    @@ -27,17 +24,17 @@

     [package.metadata]
     requires-dist = [
    -    { name = "bounded", specifier = ">=1" },
    -    { name = "combined", extras = ["foo", "bar"], marker = "sys_platform == 'darwin' or sys_platform == 'linux'", specifier = ">=1,<3" },
    +    "bounded>=1",
    +    "combined[foo,bar]>=1,<3 ; sys_platform == 'darwin' or sys_platform == 'linux'",
         { name = "conflicting", specifier = ">=2", conflict = { package = "project", extra = "foo" } },
         { name = "direct", url = "https://example.com/direct-1.0.0.tar.gz" },
    -    { name = "extra", extras = ["feature"] },
    +    "extra[feature]",
         { name = "group", groups = ["dev"] },
         { name = "index", index = "https://example.com/simple" },
    -    { name = "marker", marker = "sys_platform == 'linux'" },
    -    { name = "plain" },
    +    "marker ; sys_platform == 'linux'",
    +    "plain",
         { name = "source", virtual = "child" },
     ]

    -[package.metadata.requires-dev]
    -dev = [{ name = "plain", extras = ["feature"], marker = "sys_platform == 'linux'", specifier = ">=2" }]
    +[package.metadata.dependency-groups]
    +dev = ["plain[feature]>=2 ; sys_platform == 'linux'"]
    "#);
    Ok(())
}

/// Registry PEP 508 declarations remain valid during offline lockfile upgrades and revalidation.
#[test]
#[cfg(feature = "test-pypi")]
fn lockfile_v2_pep508_locked() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["idna>=3,<4"]
    "#})?;
    context.lock().assert().success();
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

     [options]
    @@ -19,8 +18,8 @@
     version = "1.0.0"
     source = { virtual = "." }
     dependencies = [
    -    { name = "idna" },
    +    "idna",
     ]

     [package.metadata]
    -requires-dist = [{ name = "idna", specifier = ">=3,<4" }]
    +requires-dist = ["idna>=3,<4"]
    "#);
    assert_eq!(Lock::from_canonical_toml(&upgraded)?.to_toml()?, upgraded);
    assert_eq!(toml::from_str::<Lock>(&upgraded)?.to_toml()?, upgraded);
    context
        .lock()
        .args([
            "--locked",
            "--offline",
            "--no-cache",
            "--preview-features",
            "lockfile-v2",
        ])
        .assert()
        .success();
    context
        .tree()
        .args(["--frozen", "--offline", "--preview-features", "lockfile-v2"])
        .assert()
        .success();
    assert_eq!(context.read("uv.lock"), upgraded);
    Ok(())
}

/// URL requirements retain their structured source representation.
#[test]
fn lockfile_v2_pep508_url_rejected() {
    let input = indoc! {r#"
        version = 2
        requires-python = ">=3.12"

        [workspace]
        dependencies = ["requests @ https://example.com/requests-2.32.3.tar.gz"]
    "#};
    assert_snapshot!(Lock::from_canonical_toml(input).unwrap_err(), @"failed to deserialize canonical lock: URL requirements must use a table in the lockfile");
    assert_snapshot!(toml::from_str::<Lock>(input).unwrap_err(), @r#"
    TOML parse error at line 5, column 17
      |
    5 | dependencies = ["requests @ https://example.com/requests-2.32.3.tar.gz"]
      |                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    URL requirements must use a table in the lockfile
    "#);
}

/// Git checkout settings round-trip in package identities, dependency edges, and declarations.
#[test]
fn lockfile_v2_git_sources() -> Result<()> {
    let input = indoc! {r#"
        version = 1
        requires-python = ">=3.12"

        [manifest]
        constraints = [{ name = "child", git = "https://example.com/repo?branch=main" }]
        overrides = [{ name = "child", git = "https://example.com/repo?tag=v1" }]
        build-constraints = [{ name = "child", git = "https://example.com/repo?rev=main", hashes = ["sha256:1234"] }]

        [[package]]
        name = "child"
        version = "1.0.0"
        source = { git = "https://example.com/repo?subdirectory=python&lfs=true&branch=feature%2Fwork#0123456789012345678901234567890123456789" }

        [[package]]
        name = "child"
        version = "2.0.0"
        source = { git = "https://example.com/repo?tag=v2#abcdefabcdefabcdefabcdefabcdefabcdefabcd" }

        [[package]]
        name = "project"
        version = "1.0.0"
        source = { virtual = "." }
        dependencies = [
            { name = "child", version = "1.0.0", source = { git = "https://example.com/repo?subdirectory=python&lfs=true&branch=feature%2Fwork#0123456789012345678901234567890123456789" } },
            { name = "child", version = "2.0.0", source = { git = "https://example.com/repo?tag=v2#abcdefabcdefabcdefabcdefabcdefabcdefabcd" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "child", extras = ["feature"], marker = "sys_platform == 'linux'", git = "https://example.com/repo?subdirectory=python&lfs=true&branch=feature%2Fwork" },
            { name = "archive", git = "https://example.com/repo?path=dist%2Farchive-1.0.0.tar.gz&lfs=true&rev=main" },
            { name = "pinned", git = "https://example.com/repo?rev=0123456789012345678901234567890123456789#0123456789012345678901234567890123456789" },
        ]
    "#};
    let original = toml::from_str::<Lock>(input)?.to_toml()?;
    let expected = toml::from_str::<Lock>(&input.replace("version = 1\n", "version = 2\n"))?;
    let upgraded = expected.to_toml()?;
    assert_snapshot!(diff_snapshot(&original, &upgraded, 3), @r#"
    --- old
    +++ new
    @@ -1,33 +1,33 @@
    -version = 1
    +version = 2
     requires-python = ">=3.12"

    -[manifest]
    -constraints = [{ name = "child", git = "https://example.com/repo?branch=main" }]
    -overrides = [{ name = "child", git = "https://example.com/repo?tag=v1" }]
    -build-constraints = [{ name = "child", git = "https://example.com/repo?rev=main", hashes = ["sha256:1234"] }]
    +[workspace]
    +constraints = [{ name = "child", git = "https://example.com/repo", branch = "main" }]
    +overrides = [{ name = "child", git = "https://example.com/repo", tag = "v1" }]
    +build-constraints = [{ name = "child", hashes = ["sha256:1234"], git = "https://example.com/repo", rev = "main" }]

     [[package]]
     name = "child"
     version = "1.0.0"
    -source = { git = "https://example.com/repo?subdirectory=python&lfs=true&branch=feature%2Fwork#0123456789012345678901234567890123456789" }
    +source = { git = "https://example.com/repo", branch = "feature/work", commit = "0123456789012345678901234567890123456789", subdirectory = "python", lfs = true }

     [[package]]
     name = "child"
     version = "2.0.0"
    -source = { git = "https://example.com/repo?tag=v2#abcdefabcdefabcdefabcdefabcdefabcdefabcd" }
    +source = { git = "https://example.com/repo", tag = "v2", commit = "abcdefabcdefabcdefabcdefabcdefabcdefabcd" }

     [[package]]
     name = "project"
     version = "1.0.0"
     source = { virtual = "." }
     dependencies = [
    -    { name = "child", version = "1.0.0", source = { git = "https://example.com/repo?subdirectory=python&lfs=true&branch=feature%2Fwork#0123456789012345678901234567890123456789" } },
    -    { name = "child", version = "2.0.0", source = { git = "https://example.com/repo?tag=v2#abcdefabcdefabcdefabcdefabcdefabcdefabcd" } },
    +    { name = "child", version = "1.0.0" },
    +    { name = "child", version = "2.0.0" },
     ]

     [package.metadata]
     requires-dist = [
    -    { name = "archive", git = "https://example.com/repo?path=dist%2Farchive-1.0.0.tar.gz&rev=main&lfs=true" },
    -    { name = "child", extras = ["feature"], marker = "sys_platform == 'linux'", git = "https://example.com/repo?subdirectory=python&lfs=true&branch=feature%2Fwork" },
    -    { name = "pinned", git = "https://example.com/repo?rev=0123456789012345678901234567890123456789#0123456789012345678901234567890123456789" },
    +    { name = "archive", git = "https://example.com/repo", rev = "main", path = "dist/archive-1.0.0.tar.gz", lfs = true },
    +    { name = "child", extras = ["feature"], marker = "sys_platform == 'linux'", git = "https://example.com/repo", branch = "feature/work", subdirectory = "python", lfs = true },
    +    { name = "pinned", git = "https://example.com/repo", rev = "0123456789012345678901234567890123456789", commit = "0123456789012345678901234567890123456789" },
     ]
    "#);
    assert_eq!(Lock::from_canonical_toml(&upgraded)?, expected);
    assert_eq!(toml::from_str::<Lock>(&upgraded)?, expected);
    assert_eq!(Lock::from_canonical_toml(&upgraded)?.to_toml()?, upgraded);
    Ok(())
}

/// Explicit Git fields must identify one checkout and a valid pinned commit.
#[test]
fn lockfile_v2_invalid_git_sources() {
    for fields in [
        r#"branch = "main""#,
        r#"commit = "not-a-commit""#,
        r#"branch = "main", tag = "v1", commit = "0123456789012345678901234567890123456789""#,
        r#"rev = "abcdefabcdefabcdefabcdefabcdefabcdefabcd", commit = "0123456789012345678901234567890123456789""#,
        r#"path = "child-1.0.0.tar.gz", subdirectory = "python", commit = "0123456789012345678901234567890123456789""#,
        r#"path = "child-1.0.0.tar.gz", lfs = "true", commit = "0123456789012345678901234567890123456789""#,
    ] {
        let input = format!(
            "version = 2\nrequires-python = \">=3.12\"\n\n[[package]]\nname = \"child\"\nversion = \"1.0.0\"\nsource = {{ git = \"https://example.com/repo\", {fields} }}\n"
        );
        assert!(toml::from_str::<Lock>(&input).is_err(), "accepted {fields}");
        assert!(
            Lock::from_canonical_toml(&input).is_err(),
            "accepted {fields}"
        );
    }
}

/// A generated Git lock can be revalidated after reading its structured checkout fields.
#[test]
#[cfg(all(feature = "test-git", feature = "test-universal"))]
fn lockfile_v2_git_locked() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let repository = context.temp_dir.child("repository");
    repository
        .child("python/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "child"
        version = "1.0.0"
        requires-python = ">=3.12"
    "#})?;
    Command::new("git")
        .args(["init", "--initial-branch=main"])
        .arg(repository.path())
        .assert()
        .success();
    Command::new("git")
        .arg("-C")
        .arg(repository.path())
        .args(["add", "."])
        .assert()
        .success();
    Command::new("git")
        .arg("-C")
        .arg(repository.path())
        .args([
            "-c",
            "user.name=Example",
            "-c",
            "user.email=example@example.com",
            "commit",
            "-m",
            "Initial commit",
        ])
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
        .assert()
        .success();
    let repository_url = Url::from_directory_path(repository.path())
        .map_err(|()| anyhow::anyhow!("failed to convert repository path to file URL"))?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [tool.uv.sources]
        child = {{ git = "{repository_url}", branch = "main", subdirectory = "python" }}
    "#})?;
    context
        .lock()
        .args(["--no-index", "--preview-features", "lockfile-v2"])
        .assert()
        .success();
    let lock = context.read("uv.lock");
    assert_eq!(Lock::from_canonical_toml(&lock)?.to_toml()?, lock);
    assert_eq!(toml::from_str::<Lock>(&lock)?.to_toml()?, lock);
    uv_snapshot!(context.filters(), context.lock().args(["--locked", "--offline", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Resolved 2 packages in [TIME]
    ");
    assert_eq!(context.read("uv.lock"), lock);
    Ok(())
}

/// Supported and required environments round-trip and remain valid under `--locked`.
#[test]
fn lockfile_v2_environments() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "1.0.0"
        requires-python = ">=3.12"

        [tool.uv]
        environments = ["sys_platform == 'linux'", "sys_platform == 'darwin'"]
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
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
    @@ -1,15 +1,14 @@
    -version = 1
    -revision = 5
    +version = 2
     requires-python = ">=3.12"
     resolution-markers = [
         "sys_platform == 'linux'",
         "sys_platform == 'darwin'",
     ]
    -supported-markers = [
    +supported-environments = [
         "sys_platform == 'linux'",
         "sys_platform == 'darwin'",
     ]
    -required-markers = [
    +required-environments = [
         "platform_machine == 'x86_64' and sys_platform == 'linux'",
     ]
    "#);
    assert_eq!(Lock::from_canonical_toml(&upgraded)?.to_toml()?, upgraded);
    assert_eq!(toml::from_str::<Lock>(&upgraded)?.to_toml()?, upgraded);
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked", "--preview-features", "lockfile-v2"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `lockfile-v2` feature is highly experimental. The lockfile format may change incompatibly in patch releases.
    Resolved 1 package in [TIME]
    ");
    assert_eq!(context.read("uv.lock"), upgraded);
    Ok(())
}

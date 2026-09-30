use std::collections::BTreeMap;

use anyhow::{Context, Result};
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_snapshot;
use uv_lock::Lock;
use uv_normalize::DefaultGroups;
use uv_test::uv_snapshot;

/// Record nonstandard workspace member defaults and update them when the selection changes.
#[test]
fn member_default_groups_in_lockfile() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "root"
        version = "1.0.0"
        requires-python = ">=3.12"

        [tool.uv]
        package = false

        [tool.uv.workspace]
        members = ["plain", "custom", "empty", "all"]
    "#})?;

    // Cover implicit defaults, a normalized list, no defaults, and all groups.
    context
        .temp_dir
        .child("plain/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "plain"
        version = "1.0.0"

        [tool.uv]
        package = false

        [dependency-groups]
        dev = []
        docs = []
    "#})?;
    let custom = context.temp_dir.child("custom/pyproject.toml");
    custom.write_str(indoc! {r#"
        [project]
        name = "custom"
        version = "1.0.0"

        [tool.uv]
        package = false
        default-groups = ["docs", "dev", "docs"]

        [dependency-groups]
        dev = []
        docs = []
    "#})?;
    context
        .temp_dir
        .child("empty/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "empty"
        version = "1.0.0"

        [tool.uv]
        package = false
        default-groups = []

        [dependency-groups]
        dev = []
        docs = []
    "#})?;
    context
        .temp_dir
        .child("all/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "all"
        version = "1.0.0"

        [tool.uv]
        package = false
        default-groups = "all"

        [dependency-groups]
        dev = []
        docs = []
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 5 packages in [TIME]
    ");
    let contents = context.read("uv.lock");
    assert_snapshot!(contents, @r#"
    version = 1
    revision = 5
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [manifest]
    members = [
        "all",
        "custom",
        "empty",
        "plain",
        "root",
    ]

    [[package]]
    name = "all"
    version = "1.0.0"
    source = { virtual = "all" }
    default-groups = "all"

    [package.metadata]

    [package.metadata.requires-dev]
    dev = []
    docs = []

    [[package]]
    name = "custom"
    version = "1.0.0"
    source = { virtual = "custom" }
    default-groups = ["dev", "docs"]

    [package.metadata]

    [package.metadata.requires-dev]
    dev = []
    docs = []

    [[package]]
    name = "empty"
    version = "1.0.0"
    source = { virtual = "empty" }
    default-groups = []

    [package.metadata]

    [package.metadata.requires-dev]
    dev = []
    docs = []

    [[package]]
    name = "plain"
    version = "1.0.0"
    source = { virtual = "plain" }

    [package.metadata]

    [package.metadata.requires-dev]
    dev = []
    docs = []

    [[package]]
    name = "root"
    version = "1.0.0"
    source = { virtual = "." }
    "#);

    // Both parsers accept the recorded defaults and preserve their canonical serialization.
    let lock = Lock::from_canonical_toml(&contents)?;
    assert_eq!(Lock::from_toml(&contents)?, lock);
    assert_eq!(toml::from_str::<Lock>(&contents)?, lock);
    assert_eq!(lock.to_toml()?, contents);
    assert_eq!(
        lock.member_default_groups(&"plain".parse()?),
        Some(DefaultGroups::List(vec!["dev".parse()?]))
    );
    assert_eq!(lock.member_default_groups(&"missing".parse()?), None);
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 5 packages in [TIME]
    ");

    // Changing only the default selection makes the lock stale.
    custom.write_str(indoc! {r#"
        [project]
        name = "custom"
        version = "1.0.0"

        [tool.uv]
        package = false
        default-groups = []

        [dependency-groups]
        dev = []
        docs = []
    "#})?;
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 5 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    assert_eq!(context.read("uv.lock"), contents);

    // Relocking updates the recorded defaults.
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 5 packages in [TIME]
    ");
    let updated = Lock::from_canonical_toml(&context.read("uv.lock"))?;
    insta::assert_json_snapshot!(updated.configured_member_default_groups().map(Iterator::collect::<BTreeMap<_, _>>), @r#"
    {
      "all": "all",
      "custom": [],
      "empty": []
    }
    "#);
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 5 packages in [TIME]
    ");

    // Explicit `dev` matches the implicit default and does not need a lockfile entry.
    custom.write_str(indoc! {r#"
        [project]
        name = "custom"
        version = "1.0.0"

        [tool.uv]
        package = false
        default-groups = ["dev"]

        [dependency-groups]
        dev = []
        docs = []
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 5 packages in [TIME]
    ");
    let updated = Lock::from_canonical_toml(&context.read("uv.lock"))?;
    insta::assert_json_snapshot!(updated.configured_member_default_groups().map(Iterator::collect::<BTreeMap<_, _>>), @r#"
    {
      "all": "all",
      "empty": []
    }
    "#);
    Ok(())
}

/// Record effective group Python requirements, including those inherited from other groups.
#[test]
fn member_group_python_requirements_in_lockfile() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Give the root an included group and the member its own Python requirement.
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(indoc! {r#"
        [project]
        name = "root"
        version = "1.0.0"
        requires-python = ">=3.12"

        [dependency-groups]
        base = []
        combined = [{ include-group = "base" }]
        unrestricted = []

        [tool.uv]
        package = false
        default-groups = ["combined"]

        [tool.uv.workspace]
        members = ["member"]

        [tool.uv.dependency-groups]
        base = { requires-python = ">=3.13" }
        combined = { requires-python = "<3.15" }
    "#})?;
    let member = context.temp_dir.child("member");
    member.create_dir_all()?;
    member.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "member"
        version = "1.0.0"

        [dependency-groups]
        docs = []

        [tool.uv]
        package = false
        default-groups = []

        [tool.uv.dependency-groups]
        docs = { requires-python = ">=3.14" }
    "#})?;

    // Record the declared bounds, including the combined group's inherited requirement.
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let contents = context.read("uv.lock");
    assert_snapshot!(contents, @r#"
    version = 1
    revision = 5
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [manifest]
    members = [
        "member",
        "root",
    ]

    [[package]]
    name = "member"
    version = "1.0.0"
    source = { virtual = "member" }
    default-groups = []

    [package.group-requires-python]
    docs = ">=3.14"

    [package.metadata]

    [package.metadata.requires-dev]
    docs = []

    [[package]]
    name = "root"
    version = "1.0.0"
    source = { virtual = "." }
    default-groups = ["combined"]

    [package.group-requires-python]
    base = ">=3.13"
    combined = ">=3.13,<3.15"

    [package.metadata]

    [package.metadata.requires-dev]
    base = []
    combined = []
    unrestricted = []
    "#);
    let lock = Lock::from_canonical_toml(&contents)?;
    assert_eq!(Lock::from_toml(&contents)?, lock);
    assert_eq!(toml::from_str::<Lock>(&contents)?, lock);
    assert_eq!(lock.to_toml()?, contents);
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Changing the included group's bound makes the lock stale.
    pyproject.write_str(indoc! {r#"
        [project]
        name = "root"
        version = "1.0.0"
        requires-python = ">=3.12"

        [dependency-groups]
        base = []
        combined = [{ include-group = "base" }]
        unrestricted = []

        [tool.uv]
        package = false
        default-groups = ["combined"]

        [tool.uv.workspace]
        members = ["member"]

        [tool.uv.dependency-groups]
        base = { requires-python = ">=3.14" }
        combined = { requires-python = "<3.15" }
    "#})?;
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    // Relocking records the new bound and makes the lock current again.
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    Ok(())
}

/// Record root group bounds independently of the resolved dependency markers.
#[test]
fn non_project_group_python_requirements_in_lockfile() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["member"]

        [tool.uv]
        default-groups = ["combined"]

        [dependency-groups]
        base = []
        combined = [{ include-group = "base" }]
        unrestricted = []

        [tool.uv.dependency-groups]
        base = { requires-python = ">=3.13" }
        combined = { requires-python = "<3.15" }
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
    "#})?;

    // Empty groups retain their own bounds and requirements inherited through include-group.
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    let contents = context.read("uv.lock");
    assert_snapshot!(contents, @r#"
    version = 1
    revision = 5
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [manifest]
    members = [
        "member",
    ]
    default-groups = ["combined"]

    [manifest.dependency-groups]
    base = []
    combined = []
    unrestricted = []

    [manifest.group-requires-python]
    base = ">=3.13"
    combined = ">=3.13,<3.15"

    [[package]]
    name = "member"
    version = "1.0.0"
    source = { virtual = "member" }
    "#);

    // Canonical and general TOML parsing retain the same root metadata.
    let lock = Lock::from_canonical_toml(&contents)?;
    assert_eq!(Lock::from_toml(&contents)?, lock);
    assert_eq!(toml::from_str::<Lock>(&contents)?, lock);
    assert_eq!(lock.to_toml()?, contents);

    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    // An older lock must be updated when it omits the root's Python requirements.
    context.temp_dir.child("uv.lock").write_str(indoc! {r#"
        version = 1
        revision = 5
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = ["member"]

        [[package]]
        name = "member"
        version = "1.0.0"
        source = { virtual = "member" }
    "#})?;

    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    // Changing an included group's bound makes the lock stale even without dependencies.
    pyproject.write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["member"]

        [tool.uv]
        default-groups = ["combined"]

        [dependency-groups]
        base = []
        combined = [{ include-group = "base" }]
        unrestricted = []

        [tool.uv.dependency-groups]
        base = { requires-python = ">=3.14" }
        combined = { requires-python = "<3.15" }
    "#})?;

    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    // Omitting package declaration metadata retains the root's defaults and group requirements.
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--preview-features", "lock-without-metadata"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    assert_snapshot!(context.read("uv.lock"), @r#"
    version = 1
    revision = 5
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [manifest]
    members = [
        "member",
    ]
    default-groups = ["combined"]

    [manifest.dependency-groups]
    base = []
    combined = []
    unrestricted = []

    [manifest.group-requires-python]
    base = ">=3.14"
    combined = ">=3.14,<3.15"

    [[package]]
    name = "member"
    version = "1.0.0"
    source = { virtual = "member" }
    "#);

    // Removing a requirement also makes the recorded root metadata stale.
    pyproject.write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["member"]

        [tool.uv]
        default-groups = ["combined"]

        [dependency-groups]
        base = []
        combined = [{ include-group = "base" }]
        unrestricted = []
    "#})?;

    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    Ok(())
}

/// Record canonical root defaults and retain support for older locks without them.
#[test]
fn non_project_default_groups_in_lockfile() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["member"]

        [tool.uv]
        default-groups = ["lint", "docs", "docs"]

        [dependency-groups]
        docs = []
        lint = []
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
    "#})?;

    // Root defaults are sorted and deduplicated, and empty root groups remain selectable.
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    assert_snapshot!(context.read("uv.lock"), @r#"
    version = 1
    revision = 5
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [manifest]
    members = [
        "member",
    ]
    default-groups = ["docs", "lint"]

    [manifest.dependency-groups]
    docs = []
    lint = []

    [[package]]
    name = "member"
    version = "1.0.0"
    source = { virtual = "member" }
    "#);

    // An older lock without recorded defaults remains usable with its workspace manifest.
    context.temp_dir.child("uv.lock").write_str(indoc! {r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = ["member"]

        [[package]]
        name = "member"
        version = "1.0.0"
        source = { virtual = "member" }
    "#})?;

    let legacy = Lock::from_canonical_toml(&context.read("uv.lock"))?;
    assert_eq!(legacy.workspace_default_groups(), None);
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    // Restore the recorded defaults before checking changes to the selection.
    context.temp_dir.child("uv.lock").write_str(indoc! {r#"
        version = 1
        revision = 5
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = ["member"]
        default-groups = ["docs", "lint"]

        [manifest.dependency-groups]
        docs = []
        lint = []

        [[package]]
        name = "member"
        version = "1.0.0"
        source = { virtual = "member" }
    "#})?;

    // An explicit empty default selection is recorded independently of the defined groups.
    pyproject.write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["member"]

        [tool.uv]
        default-groups = []

        [dependency-groups]
        docs = []
        lint = []
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    let lock = Lock::from_canonical_toml(&context.read("uv.lock"))?;
    assert_eq!(
        lock.workspace_default_groups(),
        Some(DefaultGroups::List(vec![]))
    );

    // Switching to all groups invalidates the recorded default selection.
    pyproject.write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["member"]

        [tool.uv]
        default-groups = "all"

        [dependency-groups]
        docs = []
        lint = []
    "#})?;

    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    let lock = Lock::from_canonical_toml(&context.read("uv.lock"))?;
    assert_eq!(lock.workspace_default_groups(), Some(DefaultGroups::All));

    // The implicit dev default is omitted, even when the root has no dev group.
    pyproject.write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["member"]

        [dependency-groups]
        docs = []
        lint = []
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    assert_snapshot!(context.read("uv.lock"), @r#"
    version = 1
    revision = 5
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [manifest]
    members = [
        "member",
    ]

    [manifest.dependency-groups]
    docs = []
    lint = []

    [[package]]
    name = "member"
    version = "1.0.0"
    source = { virtual = "member" }
    "#);
    let lock = Lock::from_canonical_toml(&context.read("uv.lock"))?;
    assert_eq!(
        lock.workspace_default_groups(),
        Some(DefaultGroups::List(vec!["dev".parse()?]))
    );
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    // Changing the implicit default still invalidates the lockfile.
    pyproject.write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["member"]

        [tool.uv]
        default-groups = ["docs"]

        [dependency-groups]
        docs = []
        lint = []
    "#})?;

    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    Ok(())
}

/// A metadata-free lock records group requirements without an explicit manifest parent table.
#[test]
fn metadata_free_group_python_requirements_in_lockfile() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "1.0.0"
        requires-python = ">=3.12"

        [dependency-groups]
        bar = []

        [tool.uv]
        package = false

        [tool.uv.dependency-groups]
        bar = { requires-python = ">=3.13" }
    "#})?;

    uv_snapshot!(context.filters(), context.lock().args([
        "--offline",
        "--preview-features",
        "lock-without-metadata",
    ]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    let contents = context.read("uv.lock");
    assert_snapshot!(contents, @r#"
    version = 1
    revision = 5
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [[package]]
    name = "project"
    version = "1.0.0"
    source = { virtual = "." }

    [package.dev-dependencies]
    bar = []

    [package.group-requires-python]
    bar = ">=3.13"
    "#);

    // Both parsers retain group requirements when package declaration metadata is omitted.
    let lock = Lock::from_canonical_toml(&contents)?;
    assert_eq!(toml::from_str::<Lock>(&contents)?, lock);
    assert_eq!(lock.to_toml()?, contents);
    assert_eq!(
        lock.member_default_groups(&"project".parse()?),
        Some(DefaultGroups::List(vec!["dev".parse()?]))
    );
    uv_snapshot!(context.filters(), context.lock().args([
        "--offline",
        "--locked",
        "--preview-features",
        "lock-without-metadata",
    ]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    Ok(())
}

/// Existing revision 3 lockfiles remain valid until a new resolution is needed.
#[test]
fn legacy_default_groups_in_lockfile() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
            [project]
            name = "root"
            version = "1.0.0"
            requires-python = ">=3.12"

            [tool.uv]
            package = false
            default-groups = ["docs"]

            [dependency-groups]
            docs = []
        "#})?;
    context.lock().arg("--offline").assert().success();
    let current = context.read("uv.lock");

    let legacy = indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "root"
        version = "1.0.0"
        source = { virtual = "." }

        [package.metadata]

        [package.metadata.requires-dev]
        docs = []
    "#};
    let old_lock = Lock::from_canonical_toml(legacy)?;
    assert_eq!(old_lock.member_default_groups(&"root".parse()?), None);
    assert!(old_lock.configured_member_default_groups().is_none());
    context.temp_dir.child("uv.lock").write_str(legacy)?;
    context
        .lock()
        .args(["--offline", "--locked"])
        .assert()
        .success();
    context.lock().arg("--offline").assert().success();
    assert_eq!(context.read("uv.lock"), legacy);
    context
        .lock()
        .args(["--offline", "--upgrade"])
        .assert()
        .success();
    assert_eq!(context.read("uv.lock"), current);
    Ok(())
}

/// Metadata-free locks do not require a particular revision.
#[test]
fn legacy_metadata_free_lockfile() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "root"
        version = "1.0.0"
        requires-python = ">=3.12"

        [project.optional-dependencies]
        feature = []

        [dependency-groups]
        docs = []

        [tool.uv]
        package = false
    "#})?;
    let legacy = indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "root"
        version = "1.0.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        feature = []

        [package.dev-dependencies]
        docs = []
    "#};
    let lock = Lock::from_canonical_toml(legacy)?;
    assert_eq!(lock.to_toml()?, legacy);
    context.temp_dir.child("uv.lock").write_str(legacy)?;
    context
        .lock()
        .args(["--offline", "--locked"])
        .assert()
        .failure();
    context
        .lock()
        .args([
            "--offline",
            "--locked",
            "--preview-features",
            "lock-without-metadata",
        ])
        .assert()
        .success();
    context
        .export()
        .args(["--offline", "--frozen", "--extra", "feature"])
        .assert()
        .success();
    context
        .export()
        .args(["--offline", "--frozen", "--only-group", "docs"])
        .assert()
        .success();
    // Unknown extras and groups are rejected even without requirement metadata.
    uv_snapshot!(context.filters(), context.export()
        .args(["--offline", "--frozen", "--extra", "missing"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Extra `missing` is not defined in the `optional-dependencies` table for `root`
    ");
    uv_snapshot!(context.filters(), context.export()
        .args(["--offline", "--frozen", "--only-group", "missing"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Group `missing` is not defined in the project's `dependency-groups` table
    ");
    assert_eq!(context.read("uv.lock"), legacy);
    Ok(())
}

/// Revision 4 preview locks do not record member defaults or group Python requirements.
#[test]
fn metadata_free_default_groups_in_lockfile() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "root"
        version = "1.0.0"
        requires-python = ">=3.12"

        [tool.uv]
        package = false
        default-groups = ["docs"]

        [dependency-groups]
        docs = []
    "#})?;
    let legacy = indoc! {r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "root"
        version = "1.0.0"
        source = { virtual = "." }

        [package.dev-dependencies]
        docs = []
    "#};
    let old_lock = Lock::from_canonical_toml(legacy)?;
    assert_eq!(old_lock.member_default_groups(&"root".parse()?), None);
    assert!(old_lock.member_group_metadata().is_none());
    context.temp_dir.child("uv.lock").write_str(legacy)?;
    context
        .lock()
        .args([
            "--offline",
            "--locked",
            "--preview-features",
            "lock-without-metadata",
        ])
        .assert()
        .success();
    assert_eq!(context.read("uv.lock"), legacy);
    context
        .lock()
        .args([
            "--offline",
            "--upgrade",
            "--preview-features",
            "lock-without-metadata",
        ])
        .assert()
        .success();
    let updated = Lock::from_canonical_toml(&context.read("uv.lock"))?;
    assert_eq!(
        updated.member_default_groups(&"root".parse()?),
        Some(DefaultGroups::List(vec!["docs".parse()?]))
    );
    context
        .lock()
        .args([
            "--offline",
            "--locked",
            "--preview-features",
            "lock-without-metadata",
        ])
        .assert()
        .success();

    // Changing defaults also invalidates a lock without requirement metadata.
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "root"
        version = "1.0.0"
        requires-python = ">=3.12"

        [tool.uv]
        package = false
        default-groups = []

        [dependency-groups]
        docs = []
    "#})?;
    uv_snapshot!(context.filters(), context.lock().args([
        "--offline",
        "--locked",
        "--preview-features",
        "lock-without-metadata",
    ]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 1 package in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    uv_snapshot!(context.filters(), context.lock().args([
        "--offline",
        "--preview-features",
        "lock-without-metadata",
    ]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    let updated = Lock::from_canonical_toml(&context.read("uv.lock"))?;
    insta::assert_json_snapshot!(updated.configured_member_default_groups().map(Iterator::collect::<BTreeMap<_, _>>), @r#"
    {
      "root": []
    }
    "#);

    Ok(())
}

/// Compare recorded default groups without depending on their order or duplicate entries.
#[test]
fn unordered_default_groups_in_lockfile() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(indoc! {r#"
        [project]
        name = "root"
        version = "1.0.0"
        requires-python = ">=3.12"

        [tool.uv]
        package = false
        default-groups = ["dev", "docs"]

        [dependency-groups]
        dev = []
        docs = []
    "#})?;
    let noncanonical = indoc! {r#"
        version = 1
        revision = 5
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "root"
        version = "1.0.0"
        source = { virtual = "." }
        default-groups = ["docs", "dev", "docs"]

        [package.metadata]

        [package.metadata.requires-dev]
        dev = []
        docs = []
    "#};
    let lockfile = context.temp_dir.child("uv.lock");
    lockfile.write_str(noncanonical)?;
    context
        .lock()
        .args(["--offline", "--check"])
        .assert()
        .success();
    context.lock().arg("--offline").assert().success();
    assert_eq!(context.read("uv.lock"), noncanonical);

    pyproject.write_str(indoc! {r#"
        [project]
        name = "root"
        version = "1.0.0"
        requires-python = ">=3.12"

        [tool.uv]
        package = false
        default-groups = ["dev"]

        [dependency-groups]
        dev = []
        docs = []
    "#})?;
    context
        .lock()
        .args(["--offline", "--check"])
        .assert()
        .failure();
    let explicit_dev = indoc! {r#"
        version = 1
        revision = 5
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "root"
        version = "1.0.0"
        source = { virtual = "." }
        default-groups = ["dev", "dev"]

        [package.metadata]

        [package.metadata.requires-dev]
        dev = []
        docs = []
    "#};
    lockfile.write_str(explicit_dev)?;
    context
        .lock()
        .args(["--offline", "--check"])
        .assert()
        .success();
    context.lock().arg("--offline").assert().success();
    assert_eq!(context.read("uv.lock"), explicit_dev);
    Ok(())
}

/// A lockfile without a workspace member list still records its root project's defaults.
#[test]
fn single_project_default_groups_in_lockfile() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "root"
        version = "1.0.0"
        requires-python = ">=3.12"

        [dependency-groups]
        docs = []

        [tool.uv]
        package = false
        default-groups = ["docs"]
    "#})?;
    context.lock().arg("--offline").assert().success();
    assert_snapshot!(context.read("uv.lock"), @r#"
    version = 1
    revision = 5
    requires-python = ">=3.12"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [[package]]
    name = "root"
    version = "1.0.0"
    source = { virtual = "." }
    default-groups = ["docs"]

    [package.metadata]

    [package.metadata.requires-dev]
    docs = []
    "#);
    let lock = Lock::from_canonical_toml(&context.read("uv.lock"))?;
    assert!(lock.members().is_empty());
    assert_eq!(
        lock.member_default_groups(&"root".parse()?),
        Some(DefaultGroups::List(vec!["docs".parse()?]))
    );
    Ok(())
}

/// Member defaults coexist with non-project workspace groups.
#[test]
fn non_project_workspace_default_groups_in_lockfile() -> Result<()> {
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
        root-only = ["member"]
    "#})?;
    context
        .temp_dir
        .child("member/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "member"
        version = "1.0.0"

        [dependency-groups]
        docs = []

        [tool.uv]
        package = false
        default-groups = ["docs"]
    "#})?;
    context.lock().arg("--offline").assert().success();
    let contents = context.read("uv.lock");
    let lock = Lock::from_canonical_toml(&contents)?;
    assert_eq!(lock.to_toml()?, contents);
    let lockfile = toml::from_str::<toml::Value>(&contents)?;
    assert!(
        lockfile["manifest"]["dependency-groups"]
            .get("root-only")
            .is_some()
    );
    assert_eq!(
        lock.member_default_groups(&"member".parse()?),
        Some(DefaultGroups::List(vec!["docs".parse()?]))
    );
    Ok(())
}

/// Explicit defaults must name declared groups, even when they match the implicit default.
#[test]
fn invalid_default_groups_in_lockfile() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["member"]
    "#})?;
    let member = context.temp_dir.child("member/pyproject.toml");
    member.write_str(indoc! {r#"
        [project]
        name = "member"
        version = "1.0.0"
        requires-python = ">=3.12"

        [tool.uv]
        package = false
    "#})?;
    context.lock().arg("--offline").assert().success();

    member.write_str(indoc! {r#"
        [project]
        name = "member"
        version = "1.0.0"
        requires-python = ">=3.12"

        [tool.uv]
        package = false
        default-groups = ["dev"]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Default group `dev` (from `tool.uv.default-groups`) is not defined in the project's `dependency-groups` table
    ");
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--check"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Default group `dev` (from `tool.uv.default-groups`) is not defined in the project's `dependency-groups` table
    ");

    member.write_str(indoc! {r#"
        [project]
        name = "member"
        version = "1.0.0"
        requires-python = ">=3.12"

        [dependency-groups]
        dev = []

        [tool.uv]
        package = false
        default-groups = ["dev"]
    "#})?;
    context.lock().arg("--offline").assert().success();
    let lock = Lock::from_canonical_toml(&context.read("uv.lock"))?;
    assert!(
        lock.configured_member_default_groups()
            .context("Missing default groups")?
            .next()
            .is_none()
    );
    Ok(())
}

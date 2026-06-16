use anyhow::Result;
#[cfg(feature = "test-pypi")]
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::indoc;

use uv_static::EnvVars;
use uv_test::uv_snapshot;

#[test]
fn check_project() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x: int = 1
    "})?;

    uv_snapshot!(context.filters(), context.check(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    ");

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn check_uses_exact_ty_version_from_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        typing = ["ty>=0.0.1"]
        dev = [{ include-group = "typing" }]

        [tool.uv]
        constraint-dependencies = ["ty==0.0.17"]

        [tool.uv.sources]
        ty = { path = "does-not-exist" }
    "#})?;
    context
        .lock()
        .arg("--no-sources")
        .env(EnvVars::UV_EXCLUDE_NEWER, "2026-02-15T00:00:00Z")
        .assert()
        .success();
    context.temp_dir.child("main.py").write_str("x = 1")?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-sources")
            .arg("--no-sync")
            .arg("--no-dev")
            .arg("--no-build-package")
            .arg("ty")
            .arg("--no-binary-package")
            .arg("ty")
            .arg("--verbose")
            .env(EnvVars::RUST_LOG, "uv::commands::project::check::ty=debug"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    DEBUG `--exclude-newer` is ignored for pinned version `0.0.17`
    DEBUG Using `ty==0.0.17`
    DEBUG Passing workspace metadata to `ty check` via stdin
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn check_uses_exact_ty_version_from_legacy_dev_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = ["ty<0.0.18"]

        [tool.uv]
        dev-dependencies = ["ty>=0.0.17"]
    "#})?;
    context.temp_dir.child("main.py").write_str("x = 1")?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-dev")
            .arg("--exclude-newer")
            .arg("2026-02-15T00:00:00Z")
            .arg("--verbose")
            .env(EnvVars::RUST_LOG, "uv::commands::project::check::ty=debug"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: The `tool.uv.dev-dependencies` field (used in `pyproject.toml`) is deprecated and will be removed in a future release; use `dependency-groups.dev` instead
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    DEBUG `--exclude-newer` is ignored for pinned version `0.0.17`
    DEBUG Using `ty==0.0.17`
    DEBUG Passing workspace metadata to `ty check` via stdin
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn check_uses_applicable_ty_version_from_forked_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = []

        [dependency-groups]
        dev = [
            "ty==0.0.16 ; python_version < '3.12'",
            "ty==0.0.17 ; python_version >= '3.12'",
        ]
    "#})?;
    context.temp_dir.child("main.py").write_str("x = 1")?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-dev")
            .arg("--verbose")
            .env(EnvVars::RUST_LOG, "uv::commands::project::check::ty=debug"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    DEBUG `--exclude-newer` is ignored for pinned version `0.0.17`
    DEBUG Using `ty==0.0.17`
    DEBUG Passing workspace metadata to `ty check` via stdin
    "
    );

    Ok(())
}

#[test]
fn check_ignores_inactive_ty_declaration() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = ["ty>=999 ; python_version < '3.12'"]
    "#})?;
    context.temp_dir.child("main.py").write_str("x = 1")?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--verbose")
            .env(EnvVars::RUST_LOG, "uv::commands::project::check::ty=debug"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    DEBUG Resolved `ty@>=0.0, <0.1` to `ty==0.0.17`
    DEBUG Passing workspace metadata to `ty check` via stdin
    "
    );

    Ok(())
}

#[test]
fn check_rejects_non_registry_ty_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = ["ty"]

        [tool.uv.sources]
        ty = { path = "does-not-exist" }
    "#})?;

    uv_snapshot!(context.filters(), context.check(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    error: The active `ty` development dependency uses the non-registry source `does-not-exist`, but `uv check` can only install standalone `ty` releases by version; use a registry source, `--ty-version`, or the `TY` environment variable
    ");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = ["ty @ https://example.com/ty.whl"]
    "#})?;

    uv_snapshot!(context.filters(), context.check(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    error: The active `ty` development dependency uses the direct URL `https://example.com/ty.whl`, but `uv check` can only install standalone `ty` releases by version; use a registry requirement, `--ty-version`, or the `TY` environment variable
    ");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = ["ty @ https://example.com/ty.whl"]

        [tool.uv.sources]
        ty = { index = "test" }

        [[tool.uv.index]]
        name = "test"
        url = "https://example.com/simple"
        explicit = true
    "#})?;

    // The registry override makes the inline URL registry-backed, so selection proceeds to the
    // missing-lock error instead of rejecting the declaration as a direct URL.
    uv_snapshot!(context.filters(), context.check().arg("--no-sync"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    error: The active `ty` development dependency requires an existing lockfile when `--no-sync` is used; update `uv.lock`, remove `--no-sync`, or use `--ty-version` or the `TY` environment variable
    ");

    Ok(())
}

#[test]
fn check_active_ty_requires_lock_with_no_sync() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = ["ty==0.0.17"]
    "#})?;

    uv_snapshot!(context.filters(), context.check().arg("--no-sync"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    error: The active `ty` development dependency requires an existing lockfile when `--no-sync` is used; update `uv.lock`, remove `--no-sync`, or use `--ty-version` or the `TY` environment variable
    ");

    Ok(())
}

#[test]
fn check_active_ty_rejects_stale_frozen_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;
    context.lock().assert().success();

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = ["ty==0.0.17"]
    "#})?;

    uv_snapshot!(context.filters(), context.check().arg("--frozen"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    error: The active `ty` development dependency is not present in the lockfile for the selected Python environment; update `uv.lock`, or use `--ty-version` or the `TY` environment variable
    ");

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn check_virtual_root_does_not_use_member_ty() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["member"]
    "#})?;
    let member = context.temp_dir.child("member");
    member.create_dir_all()?;
    member.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "member"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = ["ty==0.0.17"]
    "#})?;
    context
        .lock()
        .arg("--exclude-newer")
        .arg("2026-02-15T00:00:00Z")
        .assert()
        .success();

    pyproject_toml.write_str(indoc! {r#"
        [dependency-groups]
        dev = ["ty==0.0.17"]

        [tool.uv.workspace]
        members = ["member"]
    "#})?;

    uv_snapshot!(context.filters(), context.check().arg("--frozen"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    error: The active `ty` development dependency is not present in the lockfile for the selected Python environment; update `uv.lock`, or use `--ty-version` or the `TY` environment variable
    ");

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn check_virtual_root_uses_exact_ty_version_from_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [dependency-groups]
        dev = ["ty==0.0.17"]

        [tool.uv.workspace]
        members = ["member"]
    "#})?;
    let member = context.temp_dir.child("member");
    member.create_dir_all()?;
    member.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "member"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;
    context.temp_dir.child("main.py").write_str("x = 1")?;
    context
        .lock()
        .arg("--exclude-newer")
        .arg("2026-02-15T00:00:00Z")
        .assert()
        .success();

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-sync")
            .arg("--verbose")
            .env(EnvVars::RUST_LOG, "uv::commands::project::check::ty=debug"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    DEBUG `--exclude-newer` is ignored for pinned version `0.0.17`
    DEBUG Using `ty==0.0.17`
    DEBUG Passing workspace metadata to `ty check` via stdin
    "###
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn check_member_ty_ignores_virtual_root_dev_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [dependency-groups]
        dev = ["iniconfig==2.0.0"]

        [tool.uv.workspace]
        members = ["member"]
    "#})?;
    let member = context.temp_dir.child("member");
    member.create_dir_all()?;
    member.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "member"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = ["ty==0.0.17"]
    "#})?;
    member.child("main.py").write_str("x = 1")?;
    context
        .lock()
        .arg("--exclude-newer")
        .arg("2026-02-15T00:00:00Z")
        .assert()
        .success();

    // These mutually exclusive build settings would make the workspace root's development
    // dependency unusable if it leaked into the member-only version extraction.
    uv_snapshot!(
        context.filters(),
        context
            .check()
            .current_dir(member.path())
            .arg("--no-sync")
            .arg("--no-build-package")
            .arg("iniconfig")
            .arg("--no-binary-package")
            .arg("iniconfig")
            .arg("--verbose")
            .env(EnvVars::RUST_LOG, "uv::commands::project::check::ty=debug"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    DEBUG `--exclude-newer` is ignored for pinned version `0.0.17`
    DEBUG Using `ty==0.0.17`
    DEBUG Passing workspace metadata to `ty check` via stdin
    "
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn check_uses_ty_from_environment() -> Result<()> {
    let context =
        uv_test::test_context!("3.12").with_filter((r"ty 0\.0\.17(?: \([^)]*\))?", "ty 0.0.17"));
    let tool_dir = context.root.child("tools");
    let bin_dir = context.root.child("tool-bin");

    context
        .tool_install()
        .arg("ty==0.0.17")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::UV_EXCLUDE_NEWER, "2026-02-15T00:00:00Z")
        .assert()
        .success();

    let ty = bin_dir.child(format!("ty{}", std::env::consts::EXE_SUFFIX));
    // `TY` takes precedence over both an explicit version and an unsupported project source.
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = ["ty @ https://example.com/ty.whl"]
    "#})?;
    context.temp_dir.child("main.py").write_str("x = 1")?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-sync")
            .arg("--ty-version")
            .arg(">=999.0.0")
            .arg("--verbose")
            .env(EnvVars::RUST_LOG, "uv::commands::project::check::ty=debug")
            .env(EnvVars::TY, ty.as_os_str()),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    DEBUG Using `ty 0.0.17`
    "
    );

    Ok(())
}

#[test]
fn check_passes_workspace_metadata_to_ty() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;
    context.temp_dir.child("main.py").write_str(indoc! {r"
        x: int = 1
    "})?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--ty-version")
            .arg("0.0.17")
            .arg("--verbose")
            .env(EnvVars::RUST_LOG, "uv::commands::project::check::ty=debug"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    DEBUG `--exclude-newer` is ignored for pinned version `0.0.17`
    DEBUG Using `ty==0.0.17`
    DEBUG Passing workspace metadata to `ty check` via stdin
    "
    );

    Ok(())
}

#[test]
fn check_no_sync_ignores_invalid_lockfile() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // An explicit version bypasses both project-source validation and lockfile parsing.
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = ["ty @ https://example.com/ty.whl"]
    "#})?;
    context.temp_dir.child("uv.lock").write_str("invalid")?;
    context.temp_dir.child("main.py").write_str(indoc! {r"
        x: int = 1
    "})?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-sync")
            .arg("--ty-version")
            .arg("0.0.17")
            .env(EnvVars::RUST_LOG, "error"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    "
    );

    Ok(())
}

#[test]
fn check_rejects_tool_arguments() {
    let context = uv_test::test_context_with_versions!(&[]);

    uv_snapshot!(context.filters(), context.check().arg("--").arg("main.py"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: unexpected argument 'main.py' found

    Usage: uv check [OPTIONS]

    For more information, try '--help'.
    ");
}

#[test]
fn check_ty_version_no_match() {
    let context = uv_test::test_context_with_versions!(&[]);
    let context = context.with_filter((
        r"\b[a-z0-9_]+-(?:apple|pc|unknown)-[a-z0-9_]+(?:-[a-z0-9_]+)?\b",
        "[PLATFORM]",
    ));

    uv_snapshot!(
        context.filters(),
        context.check().arg("--ty-version").arg(">=999.0.0"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    error: Failed to find ty version matching: >=999.0.0
      Caused by: No version of ty found matching `>=999.0.0` for platform `[PLATFORM]`
    "
    );
}

#[test]
fn check_ty_version_pinned_verbose() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x: int = 1
    "})?;

    // Narrow verbose logging to the version selection this test exercises.
    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-project")
            .arg("--ty-version")
            .arg("0.0.17")
            .arg("--verbose")
            .env(EnvVars::RUST_LOG, "uv::commands::project::check::ty=debug"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    DEBUG `--exclude-newer` is ignored for pinned version `0.0.17`
    DEBUG Using `ty==0.0.17`
    "
    );

    Ok(())
}

#[test]
fn check_missing_pyproject_toml() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x: int = 1
    "})?;

    uv_snapshot!(context.filters(), context.check(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    ");

    // Project-only settings are ignored without a discovered project.
    uv_snapshot!(context.filters(), context.check().arg("--group").arg("dev").arg("--frozen").arg("--no-sync"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    warning: `--group dev` has no effect when used outside of a project
    warning: `--frozen` has no effect when used outside of a project
    warning: `--no-sync` has no effect when used outside of a project
    ");

    Ok(())
}

#[test]
fn check_no_project() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]).with_filtered_python_sources();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=4.0"
        dependencies = []
    "#})?;

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x: int = 1
    "})?;

    uv_snapshot!(context.filters(), context.check(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    error: No interpreter found for Python >=4.0 in [PYTHON SOURCES]
    ");

    // The unavailable project environment is not initialized when project discovery is disabled.
    uv_snapshot!(context.filters(), context.check().arg("--no-project"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    ");

    // Project-only settings are ignored when project discovery is disabled.
    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-project")
            .arg("--extra")
            .arg("foo")
            .arg("--group")
            .arg("bar")
            .arg("--locked")
            .arg("--no-sync"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    warning: `--extra foo` has no effect when used alongside `--no-project`
    warning: `--group bar` has no effect when used alongside `--no-project`
    warning: `--locked` has no effect when used alongside `--no-project`
    warning: `--no-sync` has no effect when used alongside `--no-project`
    "
    );

    Ok(())
}

#[test]
fn check_isolated_no_project() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=4.0"
        dependencies = []
    "#})?;

    fs_err::write(context.site_packages().join("active_only.py"), "")?;

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        import active_only
    "})?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-project")
            .env(EnvVars::VIRTUAL_ENV, context.venv.as_os_str()),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    "
    );

    let filters = context
        .filters()
        .into_iter()
        .chain([(
            r"info:   4\. \[CACHE_DIR\]/builds-v0/\[TMP\]/site-packages \(site-packages\)\n",
            "",
        )])
        .collect::<Vec<_>>();

    uv_snapshot!(
        filters,
        context
            .command()
            .arg("--isolated")
            .arg("check")
            .arg("--no-project")
            .env(EnvVars::UV_EXCLUDE_NEWER, "2026-02-15T00:00:00Z")
            .env(EnvVars::VIRTUAL_ENV, context.venv.as_os_str()),
        @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `active_only`
     --> main.py:1:8
      |
    1 | import active_only
      |        ^^^^^^^^^^^
      |
    info: Searched in the following paths during module resolution:
    info:   1. [TEMP_DIR]/ (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. [CACHE_DIR]/builds-v0/[TMP]/site-packages (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    "#
    );

    Ok(())
}

#[test]
fn check_type_error() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]);

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r#"
        name: str = "project"
        version: int = name
    "#})?;

    uv_snapshot!(context.filters(), context.check(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-assignment]: Object of type `Literal["project"]` is not assignable to `int`
     --> main.py:2:10
      |
    1 | name: str = "project"
    2 | version: int = name
      |          ---   ^^^^ Incompatible value of type `Literal["project"]`
      |          |
      |          Declared type
      |
    info: rule `invalid-assignment` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    "#);

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn check_with_declared_dependency() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [project.optional-dependencies]
        test = ["typing-extensions"]
    "#})?;

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        import iniconfig
    "})?;

    // ty should resolve the import via the synced virtual environment.
    uv_snapshot!(context.filters(), context.check(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    Installed 1 package in [TIME]
    ");

    context
        .assert_command(
            "from importlib.metadata import distribution; assert distribution('iniconfig').read_text('INSTALLER') == 'uv'",
        )
        .success();
    assert!(
        !context
            .site_packages()
            .join("typing_extensions.py")
            .exists()
    );

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn check_isolated() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
    "#})?;

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        import iniconfig
    "})?;

    let environment_sentinel = context.venv.child("sentinel");
    environment_sentinel.write_str("present")?;

    uv_snapshot!(context.filters(), context.check().arg("--isolated"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    Installed 1 package in [TIME]
    ");

    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.site_packages().join("iniconfig").exists());
    assert!(environment_sentinel.exists());

    // An existing lockfile should not be updated.
    context.lock().assert().success();
    let existing_lock = context.read("uv.lock");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig", "typing-extensions"]
    "#})?;
    main_py.write_str(indoc! {r"
        import iniconfig
        import typing_extensions
    "})?;

    context.check().arg("--isolated").assert().success();
    assert_eq!(existing_lock, context.read("uv.lock"));
    assert!(!context.site_packages().join("iniconfig").exists());
    assert!(
        !context
            .site_packages()
            .join("typing_extensions.py")
            .exists()
    );
    assert!(environment_sentinel.exists());

    Ok(())
}

#[test]
fn check_with_undeclared_dependency() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        import iniconfig
    "})?;

    let filters = context
        .filters()
        .into_iter()
        .chain([(
            r"info:   \d+\. \[VENV\]/lib64/python3\.12/site-packages \(site-packages\)\n",
            "",
        )])
        .collect::<Vec<_>>();

    // ty should report a diagnostic for the unresolvable import.
    uv_snapshot!(filters, context.check(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `iniconfig`
     --> main.py:1:8
      |
    1 | import iniconfig
      |        ^^^^^^^^^
      |
    info: Searched in the following paths during module resolution:
    info:   1. [TEMP_DIR]/ (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. [SITE_PACKAGES]/ (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    ");

    Ok(())
}

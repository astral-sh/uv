use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_snapshot;

use uv_static::EnvVars;
use uv_test::packse::PackseServer;
use uv_test::{diff_snapshot, uv_snapshot};

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
fn check_no_sync_creates_lock_without_sync() -> Result<()> {
    let server = PackseServer::new("simple/single-package.toml");
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a==1.0.0"]
    "#})?;
    context.temp_dir.child("main.py").write_str(indoc! {r"
        x: int = 1
    "})?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-sync")
            .arg("--index")
            .arg(server.index_url())
            .env_remove(EnvVars::UV_EXCLUDE_NEWER)
            .arg("--ty-version")
            .arg("0.0.17"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    "
    );

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:3d2b4c28a4e112f3a1cef1db4dc5efa33fcbbcc38bc11ccc80321097db86c097", upload-time = "2024-03-24T00:00:00Z" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:f936eedc194aa91ca01a4c6c9981136ca6c75ce6df47e3951b12522881dce809", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a" },
        ]

        [package.metadata]
        requires-dist = [{ name = "a", specifier = "==1.0.0" }]
        "#);
    });
    assert!(!context.site_packages().join("a").exists());

    Ok(())
}

#[test]
fn check_no_sync_uses_compatible_lock_interpreter() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12", "3.11"]);

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = []
    "#})?;
    context.temp_dir.child("main.py").write_str(indoc! {r"
        x: int = 1
    "})?;
    context
        .venv()
        .arg("--python")
        .arg("3.12")
        .assert()
        .success();

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-sync")
            .arg("--python")
            .arg("3.11")
            .arg("--ty-version")
            .arg("0.0.17"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    warning: Using incompatible environment (`.venv`) due to `--no-sync` (The project environment's Python version does not satisfy the request: `Python 3.11`)
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    "
    );

    assert!(context.temp_dir.child("uv.lock").exists());
    context
        .assert_command("import sys; assert sys.version_info[:2] == (3, 12)")
        .success();

    Ok(())
}

#[test]
fn check_no_sync_updates_stale_lock_without_sync() -> Result<()> {
    let server = PackseServer::new("simple/single-package.toml");
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a==1.0.0"]
    "#})?;
    context
        .lock()
        .arg("--index")
        .arg(server.index_url())
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .assert()
        .success();
    let stale_lock = context.read("uv.lock");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a==2.0.0"]
    "#})?;
    context.temp_dir.child("main.py").write_str(indoc! {r"
        x: int = 1
    "})?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-sync")
            .arg("--index")
            .arg(server.index_url())
            .env_remove(EnvVars::UV_EXCLUDE_NEWER)
            .arg("--ty-version")
            .arg("0.0.17"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    "
    );

    let updated_lock = context.read("uv.lock");
    let diff = diff_snapshot(&stale_lock, &updated_lock, 10);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(diff, @r#"
        --- old
        +++ new
        @@ -1,23 +1,23 @@
         version = 1
         revision = 3
         requires-python = ">=3.12"

         [[package]]
         name = "a"
        -version = "1.0.0"
        +version = "2.0.0"
         source = { registry = "http://[LOCALHOST]/simple/" }
        -sdist = { url = "http://[LOCALHOST]/files/a-1.0.0.tar.gz", hash = "sha256:3d2b4c28a4e112f3a1cef1db4dc5efa33fcbbcc38bc11ccc80321097db86c097", upload-time = "2024-03-24T00:00:00Z" }
        +sdist = { url = "http://[LOCALHOST]/files/a-2.0.0.tar.gz", hash = "sha256:80ec95a66cff82a78a3333e3f5702e4254cf80533f21762933252eec58c9869a", upload-time = "2024-03-24T00:00:00Z" }
         wheels = [
        -    { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-any.whl", hash = "sha256:f936eedc194aa91ca01a4c6c9981136ca6c75ce6df47e3951b12522881dce809", upload-time = "2024-03-24T00:00:00Z" },
        +    { url = "http://[LOCALHOST]/files/a-2.0.0-py3-none-any.whl", hash = "sha256:833374310e0a15880f3be9e6d082f527c9ac70129b2054d733da9b754315361f", upload-time = "2024-03-24T00:00:00Z" },
         ]

         [[package]]
         name = "project"
         version = "0.1.0"
         source = { virtual = "." }
         dependencies = [
             { name = "a" },
         ]

         [package.metadata]
        -requires-dist = [{ name = "a", specifier = "==1.0.0" }]
        +requires-dist = [{ name = "a", specifier = "==2.0.0" }]
        "#);
    });
    assert!(!context.site_packages().join("a").exists());

    Ok(())
}

#[test]
fn check_no_sync_locked_rejects_stale_lock_without_update() -> Result<()> {
    let server = PackseServer::new("simple/single-package.toml");
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a==1.0.0"]
    "#})?;
    context
        .lock()
        .arg("--index")
        .arg(server.index_url())
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .assert()
        .success();
    let stale_lock = context.read("uv.lock");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a==2.0.0"]
    "#})?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-sync")
            .arg("--locked")
            .arg("--index")
            .arg(server.index_url())
            .env_remove(EnvVars::UV_EXCLUDE_NEWER),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    "
    );

    assert_eq!(stale_lock, context.read("uv.lock"));
    assert!(!context.site_packages().join("a").exists());

    Ok(())
}

#[test]
fn check_no_sync_locked_requires_existing_lock() -> Result<()> {
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

    uv_snapshot!(
        context.filters(),
        context.check().arg("--no-sync").arg("--locked"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    error: Unable to find lockfile at `uv.lock`, but `--locked` was provided. To create a lockfile, run `uv lock` or `uv sync` without the flag.
    "
    );

    assert!(!context.temp_dir.child("uv.lock").exists());

    Ok(())
}

#[test]
fn check_no_sync_frozen_uses_existing_lock_without_update() -> Result<()> {
    let server = PackseServer::new("simple/single-package.toml");
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a==1.0.0"]
    "#})?;
    context
        .lock()
        .arg("--index")
        .arg(server.index_url())
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .assert()
        .success();
    let stale_lock = context.read("uv.lock");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a==2.0.0"]
    "#})?;
    context.temp_dir.child("main.py").write_str(indoc! {r"
        x: int = 1
    "})?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-sync")
            .arg("--frozen")
            .arg("--index")
            .arg(server.index_url())
            .env_remove(EnvVars::UV_EXCLUDE_NEWER)
            .arg("--ty-version")
            .arg("0.0.17"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    "
    );

    assert_eq!(stale_lock, context.read("uv.lock"));
    assert!(!context.site_packages().join("a").exists());

    Ok(())
}

#[test]
fn check_no_sync_frozen_requires_existing_lock() -> Result<()> {
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

    uv_snapshot!(
        context.filters(),
        context.check().arg("--no-sync").arg("--frozen"),
        @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    error: Unable to find lockfile at `uv.lock`, but `--frozen` was provided. To create a lockfile, run `uv lock` or `uv sync` without the flag.
    "
    );

    assert!(!context.temp_dir.child("uv.lock").exists());

    Ok(())
}

#[test]
fn check_no_sync_isolated_does_not_write_lock_or_sync() -> Result<()> {
    let server = PackseServer::new("simple/single-package.toml");
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a==1.0.0"]
    "#})?;
    context.temp_dir.child("main.py").write_str(indoc! {r"
        x: int = 1
    "})?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-sync")
            .arg("--isolated")
            .arg("--index")
            .arg(server.index_url())
            .env_remove(EnvVars::UV_EXCLUDE_NEWER)
            .arg("--ty-version")
            .arg("0.0.17"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    "
    );

    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.site_packages().join("a").exists());

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
    context.temp_dir.child("main.py").write_str("x = 1")?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-project")
            .arg("--ty-version")
            .arg(">=999.0.0")
            .arg("--show-version")
            .env(EnvVars::TY, ty.as_os_str()),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    Using ty 0.0.17
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
fn check_no_sync_errors_on_invalid_lockfile() -> Result<()> {
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
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    error: Failed to parse `uv.lock`
      Caused by: TOML parse error at line 1, column 8
      |
    1 | invalid
      |        ^
    key with no value, expected `=`
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
fn check_ty_version_show_version() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&[]).with_filter((
        r"(?m)^WARN Failed to fetch `ty` from .+; falling back to .+\n",
        "",
    ));

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        x: int = 1
    "})?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-project")
            .arg("--ty-version")
            .arg("0.0.17")
            .arg("--show-version"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    Using ty 0.0.17
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
fn check_with_declared_dependency() -> Result<()> {
    let server = PackseServer::new("extras/extra-does-not-exist-backtrack.toml");
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a==3.0.0"]

        [project.optional-dependencies]
        test = ["b==1.0.0"]
    "#})?;

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        import a
    "})?;

    // ty should resolve the import via the synced virtual environment.
    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--index")
            .arg(server.index_url()),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    Installed 1 package in [TIME]
    "
    );

    context
        .assert_command(
            "from importlib.metadata import distribution; assert distribution('a').read_text('INSTALLER') == 'uv'",
        )
        .success();
    assert!(!context.site_packages().join("b").exists());

    Ok(())
}

#[test]
fn check_isolated() -> Result<()> {
    let server = PackseServer::new("extras/extra-does-not-exist-backtrack.toml");
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a==3.0.0"]
    "#})?;

    let main_py = context.temp_dir.child("main.py");
    main_py.write_str(indoc! {r"
        import a
    "})?;

    let environment_sentinel = context.venv.child("sentinel");
    environment_sentinel.write_str("present")?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--isolated")
            .arg("--index")
            .arg(server.index_url()),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    Installed 1 package in [TIME]
    "
    );

    assert!(!context.temp_dir.child("uv.lock").exists());
    assert!(!context.site_packages().join("a").exists());
    assert!(environment_sentinel.exists());

    // An existing lockfile should not be updated.
    context
        .lock()
        .arg("--index")
        .arg(server.index_url())
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .assert()
        .success();
    let existing_lock = context.read("uv.lock");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a==3.0.0", "b==1.0.0"]
    "#})?;
    main_py.write_str(indoc! {r"
        import a
        import b
    "})?;

    context
        .check()
        .arg("--isolated")
        .arg("--index")
        .arg(server.index_url())
        .assert()
        .success();
    assert_eq!(existing_lock, context.read("uv.lock"));
    assert!(!context.site_packages().join("a").exists());
    assert!(!context.site_packages().join("b").exists());
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

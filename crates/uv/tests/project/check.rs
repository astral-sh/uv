use anyhow::Result;
#[cfg(feature = "test-pypi")]
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_snapshot;

use uv_static::EnvVars;
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
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})?;
    context.temp_dir.child("main.py").write_str(indoc! {r"
        x: int = 1
    "})?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-sync")
            .arg("--exclude-newer")
            .arg("2026-02-15T00:00:00Z")
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

        [options]
        exclude-newer = "2026-02-15T00:00:00Z"

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
        requires-dist = [{ name = "iniconfig", specifier = "==2.0.0" }]
        "#);
    });
    assert!(!context.site_packages().join("iniconfig").exists());

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
            .arg("--exclude-newer")
            .arg("2026-02-15T00:00:00Z")
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
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
    "#})?;
    context
        .lock()
        .arg("--exclude-newer")
        .arg("2026-02-15T00:00:00Z")
        .assert()
        .success();
    let stale_lock = context.read("uv.lock");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})?;
    context.temp_dir.child("main.py").write_str(indoc! {r"
        x: int = 1
    "})?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-sync")
            .arg("--exclude-newer")
            .arg("2026-02-15T00:00:00Z")
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
        @@ -1,48 +1,26 @@
         version = 1
         revision = 3
         requires-python = ">=3.12"

         [options]
         exclude-newer = "2026-02-15T00:00:00Z"

         [[package]]
        -name = "anyio"
        -version = "3.7.0"
        +name = "iniconfig"
        +version = "2.0.0"
         source = { registry = "https://pypi.org/simple" }
        -dependencies = [
        -    { name = "idna" },
        -    { name = "sniffio" },
        -]
        -sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737, upload-time = "2023-05-27T11:12:46.688Z" }
        +sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
         wheels = [
        -    { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873, upload-time = "2023-05-27T11:12:44.474Z" },
        -]
        -
        -[[package]]
        -name = "idna"
        -version = "3.11"
        -source = { registry = "https://pypi.org/simple" }
        -sdist = { url = "https://files.pythonhosted.org/packages/6f/6d/0703ccc57f3a7233505399edb88de3cbd678da106337b9fcde432b65ed60/idna-3.11.tar.gz", hash = "sha256:795dafcc9c04ed0c1fb032c2aa73654d8e8c5023a7df64a53f39190ada629902", size = 194582, upload-time = "2025-10-12T14:55:20.501Z" }
        -wheels = [
        -    { url = "https://files.pythonhosted.org/packages/0e/61/66938bbb5fc52dbdf84594873d5b51fb1f7c7794e9c0f5bd885f30bc507b/idna-3.11-py3-none-any.whl", hash = "sha256:771a87f49d9defaf64091e6e6fe9c18d4833f140bd19464795bc32d966ca37ea", size = 71008, upload-time = "2025-10-12T14:55:18.883Z" },
        +    { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
         ]

         [[package]]
         name = "project"
         version = "0.1.0"
         source = { virtual = "." }
         dependencies = [
        -    { name = "anyio" },
        +    { name = "iniconfig" },
         ]

         [package.metadata]
        -requires-dist = [{ name = "anyio", specifier = "==3.7.0" }]
        -
        -[[package]]
        -name = "sniffio"
        -version = "1.3.1"
        -source = { registry = "https://pypi.org/simple" }
        -sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        -wheels = [
        -    { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        -]
        +requires-dist = [{ name = "iniconfig", specifier = "==2.0.0" }]
        "#);
    });
    assert!(!context.site_packages().join("iniconfig").exists());

    Ok(())
}

#[test]
fn check_no_sync_locked_rejects_stale_lock_without_update() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
    "#})?;
    context
        .lock()
        .arg("--exclude-newer")
        .arg("2026-02-15T00:00:00Z")
        .assert()
        .success();
    let stale_lock = context.read("uv.lock");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
    "#})?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--no-sync")
            .arg("--locked")
            .arg("--exclude-newer")
            .arg("2026-02-15T00:00:00Z"),
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
    assert!(!context.site_packages().join("iniconfig").exists());

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
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
    "#})?;
    context
        .lock()
        .arg("--exclude-newer")
        .arg("2026-02-15T00:00:00Z")
        .assert()
        .success();
    let stale_lock = context.read("uv.lock");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
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
            .arg("--exclude-newer")
            .arg("2026-02-15T00:00:00Z")
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
    assert!(!context.site_packages().join("iniconfig").exists());

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
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]
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
            .arg("--exclude-newer")
            .arg("2026-02-15T00:00:00Z")
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
    assert!(!context.site_packages().join("iniconfig").exists());

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
#[cfg(feature = "test-pypi")]
fn check_script() -> Result<()> {
    let context =
        uv_test::test_context!("3.12").with_filter((r"WARN Failed to fetch `ty`[^\n]*\n", ""));

    // If `ty` accidentally uses the workspace environment, it will see this incompatible stub
    // instead of the script dependency and report that `IniConfig` is not callable.
    let workspace_iniconfig = context.site_packages().join("iniconfig");
    fs_err::create_dir_all(&workspace_iniconfig)?;
    fs_err::write(workspace_iniconfig.join("__init__.pyi"), "IniConfig: int\n")?;

    let script = context.temp_dir.child("-script.py");
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = ["iniconfig"]
        # ///

        import iniconfig

        iniconfig.IniConfig("config.ini")
    "#})?;
    context
        .temp_dir
        .child("unrelated.py")
        .write_str(indoc! {r#"
        value: int = "wrong"
    "#})?;

    uv_snapshot!(context.filters(), context.check().arg("--script").arg(script.path()).arg("--no-sync"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    warning: `uv check` is experimental and may change without warning. Pass `--preview-features check-command` to disable this warning.
    Installed 1 package in [TIME]
    warning: `--no-sync` is a no-op for Python scripts with inline metadata, which always run in isolation
    ");

    assert!(!context.temp_dir.child("-script.py.lock").exists());

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
fn check_script_no_sync_errors_on_invalid_lockfile() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = []
        # ///

        x: int = 1
    "#})?;
    context
        .temp_dir
        .child("script.py.lock")
        .write_str("invalid")?;

    uv_snapshot!(
        context.filters(),
        context
            .check()
            .arg("--script")
            .arg(script.path())
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

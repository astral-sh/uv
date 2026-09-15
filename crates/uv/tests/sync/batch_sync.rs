use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};

use uv_static::EnvVars;
use uv_test::{TestContext, uv_snapshot};

/// The lock contains remote artifacts, but checking their tags requires no downloads.
fn write_workspace(context: &TestContext) -> Result<()> {
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "root"
        version = "0.1.0"
        requires-python = ">=3.11"

        [dependency-groups]
        root-dev = ["root-only"]

        [tool.uv]
        default-groups = ["root-dev"]

        [tool.uv.workspace]
        members = ["app"]
    "#})?;
    let app = context.temp_dir.child("app");
    app.create_dir_all()?;
    app.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "app"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = ["linux-only; sys_platform == 'linux'"]

        [project.optional-dependencies]
        test = ["test-only"]

        [dependency-groups]
        lint = ["lint-only"]

        [tool.uv]
        default-groups = ["lint"]
    "#})?;
    context.temp_dir.child("uv.lock").write_str(indoc! {r#"
        version = 1
        revision = 3
        requires-python = ">=3.11"
        conflicts = [[
            { package = "app", extra = "test" },
            { package = "app", group = "lint" },
        ]]

        [manifest]
        members = ["app", "root"]

        [[package]]
        name = "app"
        version = "0.1.0"
        source = { virtual = "app" }
        dependencies = [
            { name = "linux-only", marker = "sys_platform == 'linux'" },
        ]

        [package.optional-dependencies]
        test = [{ name = "test-only" }]

        [package.dev-dependencies]
        lint = [{ name = "lint-only" }]

        [package.metadata]
        requires-dist = [
            { name = "linux-only", marker = "sys_platform == 'linux'" },
            { name = "test-only", marker = "extra == 'test'" },
        ]
        provides-extras = ["test"]

        [package.metadata.requires-dev]
        lint = [{ name = "lint-only" }]

        [[package]]
        name = "lint-only"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/lint_only-1.0.0-py3-none-any.whl", hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
        ]

        [[package]]
        name = "linux-only"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/linux_only-1.0.0-cp311-cp311-manylinux_2_31_x86_64.whl", hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
        ]

        [[package]]
        name = "root"
        version = "0.1.0"
        source = { virtual = "." }

        [package.dev-dependencies]
        root-dev = [{ name = "root-only" }]

        [package.metadata.requires-dev]
        root-dev = [{ name = "root-only" }]

        [[package]]
        name = "root-only"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/root_only-1.0.0-cp314-cp314-manylinux_2_31_x86_64.whl", hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
        ]

        [[package]]
        name = "test-only"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/test_only-1.0.0-py3-none-any.whl", hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000" },
        ]
    "#})?;
    Ok(())
}

fn batch_sync(context: &TestContext) -> Command {
    let mut command = context.sync();
    command
        .args([
            "--frozen",
            "--dry-run",
            "--batch",
            "matrix.toml",
            "--offline",
        ])
        .args(["--preview-features", "batch-sync", "--python"])
        .arg(&context.python_versions[0].1);
    command
}

fn target_sync(context: &TestContext, python_version: &str, python_platform: &str) -> Command {
    let mut command = context.sync();
    command
        .args(["--frozen", "--dry-run", "--offline", "--package", "app"])
        .args(["--preview-features", "batch-sync"])
        .arg("--python")
        .arg(&context.python_versions[0].1)
        .args(["--python-version", python_version])
        .args(["--python-platform", python_platform]);
    command
}

fn dry_run(context: &TestContext, python_version: &str, python_platform: &str) -> Command {
    let mut command = context.pip_sync();
    command
        .args(["pylock.toml", "--dry-run", "--offline", "--python"])
        .arg(&context.python_versions[0].1)
        .args([
            "--target",
            "baseline-target",
            "--preview-features",
            "pylock",
        ])
        .args(["--python-version", python_version])
        .args(["--python-platform", python_platform])
        .arg("--no-build");
    command
}

/// Match frozen export and pip's target checks using only a Python 3.12 host interpreter.
#[test]
fn batch_sync_matrix() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    write_workspace(&context)?;
    let lock = context.read("uv.lock");
    context.temp_dir.child("matrix.toml").write_str(indoc! {r#"
        [[target]]
        python-version = "3.11"
        python-platform = "x86_64-manylinux_2_31"

        [[target]]
        python-version = "3.11"
        python-platform = "x86_64-pc-windows-msvc"

        [[selection]]
        package = ["app"]

        [[selection]]
        package = ["app"]
        extra = ["test"]
        no-default-groups = true

        [[selection]]
        package = ["app"]
        only-group = ["lint"]
    "#})?;

    uv_snapshot!(context.filters(), batch_sync(&context)
        .env(EnvVars::UV_NO_INSTALL_WORKSPACE, "1"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: When using `--batch`, configure dependency selections in the manifest instead of environment variables
    ");
    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--dry-run", "--batch", "matrix.toml", "--offline", "--python"])
        .arg(&context.python_versions[0].1), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: `uv sync --batch` is experimental and may change without warning. Pass `--preview-features batch-sync` to disable this warning.
    Checked 3 selections across 2 targets
    ");

    // Explicitly enabling the preview feature suppresses the warning.
    uv_snapshot!(context.filters(), batch_sync(&context)
        .env(EnvVars::UV_NO_INSTALL_WORKSPACE, "0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Checked 3 selections across 2 targets
    ");
    assert!(!context.temp_dir.child("pylock.toml").exists());

    // Every matrix cell has the same outcome as the existing two-command pipeline.
    for selection in [
        &[][..],
        &["--extra", "test", "--no-default-groups"][..],
        &["--only-group", "lint"][..],
    ] {
        context
            .export()
            .args(["--frozen", "--offline", "--package", "app"])
            .args(selection)
            .args(["--format", "pylock.toml", "--output-file", "pylock.toml"])
            .assert()
            .success();
        for platform in ["x86_64-manylinux_2_31", "x86_64-pc-windows-msvc"] {
            dry_run(&context, "3.11", platform).assert().success();
            target_sync(&context, "3.11", platform)
                .args(selection)
                .assert()
                .success();
        }
    }

    assert_eq!(context.read("uv.lock"), lock);
    assert!(!context.venv.exists());
    Ok(())
}

/// Python ABI and glibc compatibility use the requested target, not the host's tags.
#[test]
fn batch_sync_incompatible_wheel() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    write_workspace(&context)?;
    let lock = context.read("uv.lock");
    context
        .export()
        .args(["--frozen", "--offline", "--package", "app"])
        .args(["--no-default-groups", "--output-file", "pylock.toml"])
        .assert()
        .success();

    context.temp_dir.child("matrix.toml").write_str(indoc! {r#"
        [[target]]
        python-version = "3.12"
        python-platform = "x86_64-manylinux_2_31"

        [[selection]]
        package = ["app"]
        no-default-groups = true
    "#})?;
    uv_snapshot!(context.filters(), batch_sync(&context), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Selection 1 is not installable for Python 3.12 on x86_64-manylinux_2_31
      cause: Distribution `linux-only==1.0.0 @ registry+https://pypi.org/simple` can't be installed because it doesn't have a source distribution or wheel for the current platform

    hint: You're using CPython 3.12 (`cp312`), but `linux-only` (v1.0.0) only has wheels with the following Python ABI tag: `cp311`
    ");
    dry_run(&context, "3.12", "x86_64-manylinux_2_31")
        .assert()
        .code(2);

    context.temp_dir.child("matrix.toml").write_str(indoc! {r#"
        [[target]]
        python-version = "3.11"
        python-platform = "x86_64-manylinux_2_28"

        [[selection]]
        package = ["app"]
        no-default-groups = true
    "#})?;
    uv_snapshot!(context.filters(), batch_sync(&context), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Selection 1 is not installable for Python 3.11 on x86_64-manylinux_2_28
      cause: Distribution `linux-only==1.0.0 @ registry+https://pypi.org/simple` can't be installed because it doesn't have a source distribution or wheel for the current platform

    hint: You're on Linux (`manylinux_2_28_x86_64`), but `linux-only` (v1.0.0) only has wheels for the following platform: `manylinux_2_31_x86_64`; consider adding "sys_platform == 'linux' and platform_machine == 'x86_64'" to `tool.uv.required-environments` to ensure uv resolves to a version with compatible wheels
    "#);
    dry_run(&context, "3.11", "x86_64-manylinux_2_28")
        .assert()
        .code(2);

    assert_eq!(context.read("uv.lock"), lock);
    assert!(!context.venv.exists());
    Ok(())
}

#[test]
fn batch_sync_invalid_manifest() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    write_workspace(&context)?;
    context.temp_dir.child("matrix.toml").write_str(indoc! {r"
        target = []
        selection = []
    "})?;
    uv_snapshot!(context.filters(), batch_sync(&context), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Sync manifest must contain at least one `[[target]]` and `[[selection]]
    ");

    context.temp_dir.child("matrix.toml").write_str(indoc! {r#"
        [[target]]
        python-version = "3.11"
        python-platform = "x86_64-manylinux_2_31"

        [[selection]]
        packages = ["app"]
    "#})?;
    uv_snapshot!(context.filters(), batch_sync(&context), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to parse sync manifest `matrix.toml`
      cause: TOML parse error at line 6, column 1
               |
             6 | packages = ["app"]
               | ^^^^^^^^
             unknown field `packages`, expected one of `package`, `extra`, `no-extra`, `all-extras`, `group`, `no-group`, `only-group`, `all-groups`, `no-default-groups`, `no-install-project`, `no-install-workspace`
    "#);

    let mut diagnostics = String::new();
    for (selection, arguments) in [
        (
            "extra = ['test']\nall-extras = true",
            &["--extra", "test", "--all-extras"][..],
        ),
        (
            "only-group = ['lint']\nextra = ['test']",
            &["--only-group", "lint", "--extra", "test"][..],
        ),
    ] {
        context
            .temp_dir
            .child("matrix.toml")
            .write_str(&formatdoc! {r#"
            [[target]]
            python-version = "3.11"
            python-platform = "x86_64-manylinux_2_31"

            [[selection]]
            package = ["app"]
            {selection}
        "#})?;
        let checked = batch_sync(&context).output()?;
        let baseline = target_sync(&context, "3.11", "x86_64-manylinux_2_31")
            .args(arguments)
            .output()?;
        assert_eq!(checked.status.code(), Some(2));
        assert_eq!(checked.status.code(), baseline.status.code());
        diagnostics.push_str(&String::from_utf8(checked.stderr)?);
    }
    insta::assert_snapshot!(diagnostics, @"
    error: `all-extras` cannot be combined with `extra`
    error: `only-group` cannot be combined with `extra` or `all-extras`
    ");
    assert!(!context.venv.exists());
    Ok(())
}

#[test]
fn batch_sync_invalid_selection() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    write_workspace(&context)?;
    context.temp_dir.child("matrix.toml").write_str(indoc! {r#"
        [[target]]
        python-version = "3.11"
        python-platform = "x86_64-manylinux_2_31"

        [[selection]]
        package = ["missing"]
    "#})?;
    uv_snapshot!(context.filters(), batch_sync(&context), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Invalid selection 1
      cause: Package `missing` not found in workspace
    ");

    // Selecting an extra must still reject a conflicting default group.
    context.temp_dir.child("matrix.toml").write_str(indoc! {r#"
        [[target]]
        python-version = "3.11"
        python-platform = "x86_64-manylinux_2_31"

        [[selection]]
        package = ["app"]
        extra = ["test"]
    "#})?;
    uv_snapshot!(context.filters(), batch_sync(&context), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Selection 1 is not installable for Python 3.11 on x86_64-manylinux_2_31
      cause: Extra `test` and group `lint` (enabled by default) are incompatible with the declared conflicts: {`app[test]`, `app:lint`}
    ");

    // Frozen checks require every discovered member to be locked, even an unselected root.
    context.temp_dir.child("pyproject.toml").write_str(
        &context
            .read("pyproject.toml")
            .replace("name = \"root\"", "name = \"unlocked\""),
    )?;
    uv_snapshot!(context.filters(), batch_sync(&context), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: The lockfile at `uv.lock` needs to be updated, but `--frozen` was provided: Missing workspace member `unlocked`.

    hint: To update the lockfile, run `uv lock`.
    ");

    fs_err::remove_file(context.temp_dir.child("uv.lock"))?;
    uv_snapshot!(context.filters(), batch_sync(&context), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Unable to find lockfile at `uv.lock`, but `--frozen` was provided. To create a lockfile, run `uv lock` or `uv sync` without the flag.
    ");
    assert!(!context.venv.exists());
    Ok(())
}

/// A selected local package still needs a build unless it is excluded from installation.
#[test]
fn batch_sync_local_build_policy() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    write_workspace(&context)?;
    context.temp_dir.child("uv.lock").write_str(
        &context
            .read("uv.lock")
            .replace("{ virtual = \"app\" }", "{ editable = \"app\" }"),
    )?;
    context
        .temp_dir
        .child("app/pyproject.toml")
        .write_str(&format!(
            "{}\n{}",
            context.read("app/pyproject.toml"),
            indoc! {r#"
            [build-system]
            requires = []
            build-backend = "backend"
        "#},
        ))?;
    let lock = context.read("uv.lock");
    context.temp_dir.child("matrix.toml").write_str(indoc! {r#"
        [[target]]
        python-version = "3.11"
        python-platform = "x86_64-pc-windows-msvc"

        [[selection]]
        package = ["app"]
        no-default-groups = true
    "#})?;
    uv_snapshot!(context.filters(), batch_sync(&context).arg("--no-build"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Selection 1 is not installable for Python 3.11 on x86_64-pc-windows-msvc
      cause: Building `app` is disabled by `--no-build`
    ");

    context
        .export()
        .args(["--frozen", "--offline", "--package", "app"])
        .args(["--no-default-groups", "--output-file", "pylock.toml"])
        .assert()
        .success();
    uv_snapshot!(context.filters(), dry_run(&context, "3.11", "x86_64-pc-windows-msvc"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    error: Package `app` can't be installed because it is marked as `--no-build` but has no binary distribution
    ");

    context.temp_dir.child("matrix.toml").write_str(indoc! {r#"
        [[target]]
        python-version = "3.11"
        python-platform = "x86_64-pc-windows-msvc"

        [[selection]]
        package = ["app"]
        no-default-groups = true
        no-install-workspace = true
    "#})?;
    uv_snapshot!(context.filters(), batch_sync(&context).arg("--no-build"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Checked 1 selections across 1 targets
    ");
    assert_eq!(context.read("uv.lock"), lock);
    assert!(!context.venv.exists());
    Ok(())
}

/// A target Python version changes wheel selection without requiring that interpreter.
#[test]
fn sync_target_python_version() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    write_workspace(&context)?;
    let lock = context.read("uv.lock");

    uv_snapshot!(context.filters(), target_sync(&context, "3.11", "x86_64-manylinux_2_31"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Would create project environment at: .venv
    Would download 2 packages
    Would install 2 packages
     + lint-only==1.0.0
     + linux-only==1.0.0
    ");
    assert_eq!(context.read("uv.lock"), lock);
    assert!(!context.venv.exists());
    assert!(!context.temp_dir.child("pylock.toml").exists());
    Ok(())
}

/// An installed matching version cannot hide an incompatible artifact in the lockfile.
#[test]
fn sync_target_checks_satisfied_environment() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    write_workspace(&context)?;
    let lock = context.read("uv.lock");
    let distribution = ChildPath::new(context.site_packages()).child("linux_only-1.0.0.dist-info");
    distribution.create_dir_all()?;
    distribution.child("METADATA").write_str(indoc! {r"
        Metadata-Version: 2.3
        Name: linux-only
        Version: 1.0.0
    "})?;
    distribution.child("INSTALLER").write_str("uv\n")?;
    distribution.child("RECORD").touch()?;

    uv_snapshot!(context.filters(), target_sync(&context, "3.11", "x86_64-manylinux_2_31")
        .arg("--no-default-groups"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Would use project environment at: .venv
    Checked 1 package in [TIME]
    Would make no changes
    ");

    uv_snapshot!(context.filters(), target_sync(&context, "3.12", "x86_64-manylinux_2_31")
        .arg("--no-default-groups"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Would use project environment at: .venv
    error: Distribution `linux-only==1.0.0 @ registry+https://pypi.org/simple` can't be installed because it doesn't have a source distribution or wheel for the current platform

    hint: You're using CPython 3.12 (`cp312`), but `linux-only` (v1.0.0) only has wheels with the following Python ABI tag: `cp311`
    ");
    assert_eq!(context.read("uv.lock"), lock);
    assert!(distribution.child("METADATA").exists());
    Ok(())
}

#[test]
fn batch_sync_requires_frozen_dry_run() {
    let context = uv_test::test_context_with_versions!(&[]);
    uv_snapshot!(context.filters(), context.sync()
        .args(["--batch", "matrix.toml", "--frozen"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the following required arguments were not provided:
      --dry-run

    Usage: uv sync --frozen --dry-run --cache-dir [CACHE_DIR] --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");
    uv_snapshot!(context.filters(), context.sync()
        .args(["--batch", "matrix.toml", "--dry-run"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the following required arguments were not provided:
      --frozen

    Usage: uv sync --frozen --dry-run --cache-dir [CACHE_DIR] --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");
    assert!(!context.venv.exists());
}

#[test]
fn batch_sync_rejects_command_line_selection() {
    let context = uv_test::test_context_with_versions!(&[]);
    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--dry-run", "--batch", "matrix.toml", "--package", "app"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the argument '--batch <BATCH>' cannot be used with '--package <PACKAGE>'

    Usage: uv sync --cache-dir [CACHE_DIR] --frozen --dry-run --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");
    assert!(!context.venv.exists());
}

#[test]
fn sync_target_python_version_requires_frozen_dry_run() {
    let context = uv_test::test_context_with_versions!(&[]);
    uv_snapshot!(context.filters(), context.sync()
        .args(["--python-version", "3.11", "--frozen"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the following required arguments were not provided:
      --dry-run

    Usage: uv sync --frozen --dry-run --cache-dir [CACHE_DIR] --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");
    uv_snapshot!(context.filters(), context.sync()
        .args(["--python-version", "3.11", "--dry-run"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: the following required arguments were not provided:
      --frozen

    Usage: uv sync --frozen --dry-run --cache-dir [CACHE_DIR] --exclude-newer <EXCLUDE_NEWER>

    For more information, try '--help'.
    ");
    assert!(!context.venv.exists());
}

#[test]
fn sync_target_python_version_outside_lock() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.12"]);
    write_workspace(&context)?;
    uv_snapshot!(context.filters(), target_sync(&context, "3.10", "x86_64-manylinux_2_31"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Would create project environment at: .venv
    error: The current Python version (3.10.0) is not compatible with the locked Python requirement: `>=3.11`
    ");
    assert!(!context.venv.exists());
    Ok(())
}

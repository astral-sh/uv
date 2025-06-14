use crate::common::{TestContext, uv_snapshot, venv_bin_path};
use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::{FileWriteStr, PathChild};
use flate2::bufread::GzDecoder;
use fs_err::File;
use indoc::indoc;
use std::env;
use std::io::BufReader;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use uv_static::EnvVars;

const BUILT_BY_UV_TEST_SCRIPT: &str = indoc! {r#"
    from built_by_uv import greet
    from built_by_uv.arithmetic.circle import area

    print(greet())
    print(f"Area of a circle with r=2: {area(2)}")
"#};

/// Test that build backend works if we invoke it directly.
///
/// We can't test end-to-end here including the PEP 517 bridge code since we don't have a uv wheel.
#[test]
#[cfg(feature = "pypi")]
fn built_by_uv_direct_wheel() -> Result<()> {
    let context = TestContext::new("3.12");
    let built_by_uv = Path::new("../../scripts/packages/built-by-uv");

    let temp_dir = TempDir::new()?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(temp_dir.path())
        .current_dir(built_by_uv), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    built_by_uv-0.1.0-py3-none-any.whl

    ----- stderr -----
    "###);

    context
        .pip_install()
        .arg(temp_dir.path().join("built_by_uv-0.1.0-py3-none-any.whl"))
        .assert()
        .success();

    uv_snapshot!(context
        .run()
        .arg("python")
        .arg("-c")
        .arg(BUILT_BY_UV_TEST_SCRIPT)
        // Python on windows
        .env(EnvVars::PYTHONUTF8, "1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello 👋
    Area of a circle with r=2: 12.56636

    ----- stderr -----
    "###);

    uv_snapshot!(Command::new("say-hi")
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hi from a script!

    ----- stderr -----
    "###);

    Ok(())
}

/// Test that source tree -> source dist -> wheel works.
///
/// We can't test end-to-end here including the PEP 517 bridge code since we don't have a uv wheel,
/// so we call the build backend directly.
#[test]
#[cfg(feature = "pypi")]
fn built_by_uv_direct() -> Result<()> {
    let context = TestContext::new("3.12");
    let built_by_uv = Path::new("../../scripts/packages/built-by-uv");

    let sdist_dir = TempDir::new()?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-sdist")
        .arg(sdist_dir.path())
        .current_dir(built_by_uv), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    built_by_uv-0.1.0.tar.gz

    ----- stderr -----
    "###);

    let sdist_tree = TempDir::new()?;

    let sdist_reader = BufReader::new(File::open(
        sdist_dir.path().join("built_by_uv-0.1.0.tar.gz"),
    )?);
    tar::Archive::new(GzDecoder::new(sdist_reader)).unpack(sdist_tree.path())?;

    drop(sdist_dir);

    let wheel_dir = TempDir::new()?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(wheel_dir.path())
        .current_dir(sdist_tree.path().join("built_by_uv-0.1.0")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    built_by_uv-0.1.0-py3-none-any.whl

    ----- stderr -----
    "###);

    drop(sdist_tree);

    context
        .pip_install()
        .arg(wheel_dir.path().join("built_by_uv-0.1.0-py3-none-any.whl"))
        .assert()
        .success();

    drop(wheel_dir);

    uv_snapshot!(context
        .run()
        .arg("python")
        .arg("-c")
        .arg(BUILT_BY_UV_TEST_SCRIPT)
        // Python on windows
        .env(EnvVars::PYTHONUTF8, "1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello 👋
    Area of a circle with r=2: 12.56636

    ----- stderr -----
    "###);

    Ok(())
}

/// Test that editables work.
///
/// We can't test end-to-end here including the PEP 517 bridge code since we don't have a uv wheel,
/// so we call the build backend directly.
#[test]
#[cfg(feature = "pypi")]
fn built_by_uv_editable() -> Result<()> {
    let context = TestContext::new("3.12");
    let built_by_uv = Path::new("../../scripts/packages/built-by-uv");

    // Without the editable, pytest fails.
    context.pip_install().arg("pytest").assert().success();
    Command::new(context.interpreter())
        .arg("-m")
        .arg("pytest")
        .current_dir(built_by_uv)
        .assert()
        .failure();

    // Build and install the editable. Normally, this should be one step with the editable never
    // been seen, but we have to split it for the test.
    let wheel_dir = TempDir::new()?;
    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(wheel_dir.path())
        .current_dir(built_by_uv), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    built_by_uv-0.1.0-py3-none-any.whl

    ----- stderr -----
    "###);
    context
        .pip_install()
        .arg(wheel_dir.path().join("built_by_uv-0.1.0-py3-none-any.whl"))
        .assert()
        .success();

    drop(wheel_dir);

    // Now, pytest passes.
    uv_snapshot!(Command::new(context.interpreter())
        .arg("-m")
        .arg("pytest")
        // Avoid showing absolute paths and column dependent layout
        .arg("--quiet")
        .arg("--capture=no")
        .current_dir(built_by_uv), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    ..
    2 passed in [TIME]

    ----- stderr -----
    "###);

    Ok(())
}

#[cfg(all(unix, feature = "git"))]
#[test]
fn preserve_executable_bit() -> Result<()> {
    use std::io::Write;

    let context = TestContext::new("3.12");

    let project_dir = context.temp_dir.path().join("preserve_executable_bit");
    context
        .init()
        .arg("--build-backend")
        .arg("uv")
        .arg("--preview")
        .arg(&project_dir)
        .assert()
        .success();

    fs_err::OpenOptions::new()
        .write(true)
        .append(true)
        .open(project_dir.join("pyproject.toml"))?
        .write_all(
            indoc! {r#"
            [tool.uv.build-backend.data]
            scripts = "scripts"
        "#}
            .as_bytes(),
        )?;

    fs_err::create_dir(project_dir.join("scripts"))?;
    fs_err::write(
        project_dir.join("scripts").join("greet.sh"),
        indoc! {r#"
        echo "Hi from the shell"
    "#},
    )?;

    context
        .build_backend()
        .arg("build-wheel")
        .arg(context.temp_dir.path())
        .current_dir(project_dir)
        .assert()
        .success();

    let wheel = context
        .temp_dir
        .path()
        .join("preserve_executable_bit-0.1.0-py3-none-any.whl");
    context.pip_install().arg(wheel).assert().success();

    uv_snapshot!(Command::new("greet.sh")
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hi from the shell

    ----- stderr -----
    "###);

    Ok(())
}

/// Test `tool.uv.build-backend.module-name`.
///
/// We include only the module specified by `module-name`, ignoring the project name and all other
/// potential modules.
#[test]
fn rename_module() -> Result<()> {
    let context = TestContext::new("3.12");
    let temp_dir = TempDir::new()?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "foo"
        version = "1.0.0"

        [tool.uv.build-backend]
        module-name = "bar"

        [build-system]
        requires = ["uv_build>=0.5,<0.8"]
        build-backend = "uv_build"
    "#})?;

    // This is the module we would usually include, but due to the renaming by `module-name` must
    // ignore.
    context
        .temp_dir
        .child("src/foo/__init__.py")
        .write_str(r#"print("Hi from foo")"#)?;
    // This module would be ignored from just `project.name`, but is selected due to the renaming.
    context
        .temp_dir
        .child("src/bar/__init__.py")
        .write_str(r#"print("Hi from bar")"#)?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(temp_dir.path())
        .env("UV_PREVIEW", "1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    foo-1.0.0-py3-none-any.whl

    ----- stderr -----
    "###);

    context
        .pip_install()
        .arg(temp_dir.path().join("foo-1.0.0-py3-none-any.whl"))
        .assert()
        .success();

    // Importing the module with the `module-name` name succeeds.
    uv_snapshot!(Command::new(context.interpreter())
        .arg("-c")
        .arg("import bar")
        // Python on windows
        .env(EnvVars::PYTHONUTF8, "1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hi from bar

    ----- stderr -----
    "###);

    // Importing the package name fails, it was overridden by `module-name`.
    uv_snapshot!(Command::new(context.interpreter())
        .arg("-c")
        .arg("import foo")
        // Python on windows
        .env(EnvVars::PYTHONUTF8, "1"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Traceback (most recent call last):
      File "<string>", line 1, in <module>
    ModuleNotFoundError: No module named 'foo'
    "###);

    Ok(())
}

/// Test `tool.uv.build-backend.module-name` for editable builds.
#[test]
fn rename_module_editable_build() -> Result<()> {
    let context = TestContext::new("3.12");
    let temp_dir = TempDir::new()?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "foo"
        version = "1.0.0"

        [tool.uv.build-backend]
        module-name = "bar"

        [build-system]
        requires = ["uv_build>=0.5,<0.8"]
        build-backend = "uv_build"
    "#})?;

    context
        .temp_dir
        .child("src/bar/__init__.py")
        .write_str(r#"print("Hi from bar")"#)?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-editable")
        .arg(temp_dir.path())
        .env("UV_PREVIEW", "1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    foo-1.0.0-py3-none-any.whl

    ----- stderr -----
    "###);

    context
        .pip_install()
        .arg(temp_dir.path().join("foo-1.0.0-py3-none-any.whl"))
        .assert()
        .success();

    // Importing the module with the `module-name` name succeeds.
    uv_snapshot!(Command::new(context.interpreter())
        .arg("-c")
        .arg("import bar")
        // Python on windows
        .env(EnvVars::PYTHONUTF8, "1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hi from bar

    ----- stderr -----
    "###);

    Ok(())
}

/// Check that the build succeeds even if the module name mismatches by case.
#[test]
fn build_module_name_normalization() -> Result<()> {
    let context = TestContext::new("3.12");

    let wheel_dir = context.temp_dir.path().join("dist");
    fs_err::create_dir(&wheel_dir)?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "django-plugin"
        version = "1.0.0"

        [build-system]
        requires = ["uv_build>=0.5,<0.8"]
        build-backend = "uv_build"

        [tool.uv.build-backend]
        module-name = "Django_plugin"
    "#})?;
    fs_err::create_dir_all(context.temp_dir.join("src"))?;

    // Error case 1: No matching module.
    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(&wheel_dir), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Missing module directory for `Django_plugin` in `src`. Found: ``
    ");

    fs_err::create_dir_all(context.temp_dir.join("src/Django_plugin"))?;
    // Error case 2: A matching module, but no `__init__.py`.
    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(&wheel_dir), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Expected a Python module directory at: `src/Django_plugin/__init__.py`
    ");

    // Use `Django_plugin` instead of `django_plugin`
    context
        .temp_dir
        .child("src/Django_plugin/__init__.py")
        .write_str(r#"print("Hi from bar")"#)?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(&wheel_dir), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    django_plugin-1.0.0-py3-none-any.whl

    ----- stderr -----
    ");

    context
        .pip_install()
        .arg("--no-index")
        .arg("--find-links")
        .arg(&wheel_dir)
        .arg("django-plugin")
        .assert()
        .success();

    uv_snapshot!(Command::new(context.interpreter())
        .arg("-c")
        .arg("import Django_plugin")
        // Python on windows
        .env(EnvVars::PYTHONUTF8, "1"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Hi from bar

    ----- stderr -----
    ");

    // Former error case 3, now accepted: Multiple modules a matching name.
    // Requires a case-sensitive filesystem.
    #[cfg(target_os = "linux")]
    {
        context
            .temp_dir
            .child("src/django_plugin/__init__.py")
            .write_str(r#"print("Hi from bar")"#)?;

        uv_snapshot!(context
            .build_backend()
            .arg("build-wheel")
            .arg(&wheel_dir), @r"
        success: true
        exit_code: 0
        ----- stdout -----
        django_plugin-1.0.0-py3-none-any.whl

        ----- stderr -----
        ");
    }

    Ok(())
}

#[test]
fn build_sdist_with_long_path() -> Result<()> {
    let context = TestContext::new("3.12");
    let temp_dir = TempDir::new()?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "foo"
        version = "1.0.0"

        [build-system]
        requires = ["uv_build>=0.7,<0.8"]
        build-backend = "uv_build"
    "#})?;
    context
        .temp_dir
        .child("src/foo/__init__.py")
        .write_str(r#"print("Hi from foo")"#)?;

    let long_path = format!("src/foo/l{}ng/__init__.py", "o".repeat(100));
    context
        .temp_dir
        .child(long_path)
        .write_str(r#"print("Hi from foo")"#)?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-sdist")
        .arg(temp_dir.path())
        .env("UV_PREVIEW", "1"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    foo-1.0.0.tar.gz

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn sdist_error_without_module() -> Result<()> {
    let context = TestContext::new("3.12");
    let temp_dir = TempDir::new()?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "foo"
        version = "1.0.0"

        [build-system]
        requires = ["uv_build>=0.7,<0.8"]
        build-backend = "uv_build"
    "#})?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-sdist")
        .arg(temp_dir.path())
        .env("UV_PREVIEW", "1"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Missing source directory at: `src`
    ");

    fs_err::create_dir(context.temp_dir.join("src"))?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-sdist")
        .arg(temp_dir.path())
        .env("UV_PREVIEW", "1"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Missing module directory for `foo` in `src`. Found: ``
    ");

    Ok(())
}

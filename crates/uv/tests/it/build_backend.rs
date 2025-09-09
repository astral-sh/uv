use crate::common::{TestContext, uv_snapshot, venv_bin_path};
use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::{FileTouch, FileWriteStr, PathChild, PathCreateDir};
use flate2::bufread::GzDecoder;
use fs_err::File;
use indoc::{formatdoc, indoc};
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

    uv_snapshot!(context.python_command()
        .arg("-c")
        .arg(BUILT_BY_UV_TEST_SCRIPT), @r###"
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

    uv_snapshot!(context.python_command()
        .arg("-c")
        .arg(BUILT_BY_UV_TEST_SCRIPT), @r###"
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
    context
        .python_command()
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
    uv_snapshot!(context.python_command()
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
        .arg("--lib")
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
        requires = ["uv_build>=0.7,<10000"]
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
        .arg(temp_dir.path()), @r###"
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
    uv_snapshot!(context.python_command()
        .arg("-c")
        .arg("import bar"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Hi from bar

    ----- stderr -----
    "###);

    // Importing the package name fails, it was overridden by `module-name`.
    uv_snapshot!(context.python_command()
        .arg("-c")
        .arg("import foo"), @r###"
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
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;

    context
        .temp_dir
        .child("src/bar/__init__.py")
        .write_str(r#"print("Hi from bar")"#)?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-editable")
        .arg(temp_dir.path()), @r###"
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
    uv_snapshot!(context.python_command()
        .arg("-c")
        .arg("import bar"), @r###"
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
        requires = ["uv_build>=0.7,<10000"]
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
    error: Expected a Python module at: `src/Django_plugin/__init__.py`
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
    error: Expected a Python module at: `src/Django_plugin/__init__.py`
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

    uv_snapshot!(context.python_command()
        .arg("-c")
        .arg("import Django_plugin"), @r"
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
        requires = ["uv_build>=0.7,<10000"]
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
        .arg(temp_dir.path()), @r###"
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
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-sdist")
        .arg(temp_dir.path()), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Expected a Python module at: `src/foo/__init__.py`
    ");

    fs_err::create_dir(context.temp_dir.join("src"))?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-sdist")
        .arg(temp_dir.path()), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Expected a Python module at: `src/foo/__init__.py`
    ");

    Ok(())
}

#[test]
fn complex_namespace_packages() -> Result<()> {
    let context = TestContext::new("3.12");
    let dist = context.temp_dir.child("dist");
    dist.create_dir_all()?;

    let init_py_a = indoc! {"
        def one():
            return 1
    "};

    let init_py_b = indoc! {"
        from complex_project.part_a import one

        def two():
            return one() + one()
    "};

    let projects = [
        ("complex-project", "part_a", init_py_a),
        ("complex-project", "part_b", init_py_b),
    ];

    for (project_name, part_name, init_py) in projects {
        let project = context
            .temp_dir
            .child(format!("{project_name}-{part_name}"));
        let project_name_dist_info = project_name.replace('-', "_");
        let pyproject_toml = formatdoc! {r#"
            [project]
            name = "{project_name}-{part_name}"
            version = "1.0.0"

            [tool.uv.build-backend]
            module-name = "{project_name_dist_info}.{part_name}"

            [build-system]
            requires = ["uv_build>=0.7,<10000"]
            build-backend = "uv_build"
            "#
        };
        project.child("pyproject.toml").write_str(&pyproject_toml)?;

        project
            .child("src")
            .child(project_name_dist_info)
            .child(part_name)
            .child("__init__.py")
            .write_str(init_py)?;

        context
            .build()
            .arg(project.path())
            .arg("--out-dir")
            .arg(dist.path())
            .assert()
            .success();
    }

    uv_snapshot!(
        context.filters(),
        context
            .pip_install()
            .arg("complex-project-part-a")
            .arg("complex-project-part-b")
            .arg("--offline")
            .arg("--find-links")
            .arg(dist.path()),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + complex-project-part-a==1.0.0
     + complex-project-part-b==1.0.0
    "
    );

    uv_snapshot!(context.python_command()
        .arg("-c")
        .arg("from complex_project.part_b import two; print(two())"),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----
    2

    ----- stderr -----
    "
    );

    // Test editable installs
    uv_snapshot!(
        context.filters(),
        context
            .pip_install()
            .arg("-e")
            .arg("complex-project-part_a")
            .arg("-e")
            .arg("complex-project-part_b")
            .arg("--offline"),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     - complex-project-part-a==1.0.0
     + complex-project-part-a==1.0.0 (from file://[TEMP_DIR]/complex-project-part_a)
     - complex-project-part-b==1.0.0
     + complex-project-part-b==1.0.0 (from file://[TEMP_DIR]/complex-project-part_b)
    "
    );

    uv_snapshot!(context.python_command()
        .arg("-c")
        .arg("from complex_project.part_b import two; print(two())"),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----
    2

    ----- stderr -----
    "
    );
    Ok(())
}

/// Test that a symlinked file (here: license) gets included.
#[test]
#[cfg(unix)]
fn symlinked_file() -> Result<()> {
    let context = TestContext::new("3.12");

    let project = context.temp_dir.child("project");
    context
        .init()
        .arg("--lib")
        .arg(project.path())
        .assert()
        .success();

    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "1.0.0"
        license-files = ["LICENSE"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;

    let license_file = context.temp_dir.child("LICENSE");
    let license_symlink = project.child("LICENSE");

    let license_text = "Project license";
    license_file.write_str(license_text)?;
    fs_err::os::unix::fs::symlink(license_file.path(), license_symlink.path())?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-sdist")
        .arg(context.temp_dir.path())
        .current_dir(project.path()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    project-1.0.0.tar.gz

    ----- stderr -----
    ");

    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(context.temp_dir.path())
        .current_dir(project.path()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    project-1.0.0-py3-none-any.whl

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.pip_install().arg("project-1.0.0-py3-none-any.whl"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==1.0.0 (from file://[TEMP_DIR]/project-1.0.0-py3-none-any.whl)
    ");

    // Check that we included the actual license text and not a broken symlink.
    let installed_license = context
        .site_packages()
        .join("project-1.0.0.dist-info")
        .join("licenses")
        .join("LICENSE");
    assert!(
        fs_err::symlink_metadata(&installed_license)?
            .file_type()
            .is_file()
    );
    let license = fs_err::read_to_string(&installed_license)?;
    assert_eq!(license, license_text);

    Ok(())
}

/// Ignore invalid build backend settings when not building.
///
/// They may be from another `uv_build` version that has a different schema.
#[test]
fn invalid_build_backend_settings_are_ignored() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "built-by-uv"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv.build-backend]
        # Error: `source-include` must be a list
        source-include = "data/build-script.py"

        [build-system]
        requires = ["uv_build>=10000,<10001"]
        build-backend = "uv_build"
    "#})?;

    // Since we are not building, this must pass without complaining about the error in
    // `tool.uv.build-backend`.
    uv_snapshot!(context.filters(), context.lock(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    Ok(())
}

/// Error when there is a relative module root outside the project root, such as
/// `tool.uv.build-backend.module-root = ".."`.
#[test]
fn error_on_relative_module_root_outside_project_root() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv.build-backend]
        module-root = ".."

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;

    context.temp_dir.child("__init__.py").touch()?;

    uv_snapshot!(context.filters(), context.build(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution (uv build backend)...
      × Failed to build `[TEMP_DIR]/`
      ╰─▶ Module root must be inside the project: `..`
    ");

    uv_snapshot!(context.filters(), context.build().arg("--wheel"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building wheel (uv build backend)...
      × Failed to build `[TEMP_DIR]/`
      ╰─▶ Module root must be inside the project: `..`
    ");

    Ok(())
}

/// Error when there is a relative data directory outside the project root, such as
/// `tool.uv.build-backend.data.headers = "../headers"`.
#[test]
fn error_on_relative_data_dir_outside_project_root() -> Result<()> {
    let context = TestContext::new("3.12");

    let project = context.temp_dir.child("project");
    project.create_dir_all()?;

    let pyproject_toml = project.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv.build-backend.data]
        headers = "../header"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;

    let project_module = project.child("src/project");
    project_module.create_dir_all()?;
    project_module.child("__init__.py").touch()?;

    context.temp_dir.child("headers").create_dir_all()?;

    uv_snapshot!(context.filters(), context.build().arg("project"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution (uv build backend)...
      × Failed to build `[TEMP_DIR]/project`
      ╰─▶ The path for the data directory headers must be inside the project: `../header`
    ");

    uv_snapshot!(context.filters(), context.build().arg("project").arg("--wheel"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building wheel (uv build backend)...
      × Failed to build `[TEMP_DIR]/project`
      ╰─▶ The path for the data directory headers must be inside the project: `../header`
    ");

    Ok(())
}

/// Warn for cases where `tool.uv.build-backend` is used without the corresponding build backend
/// entry.
#[test]
fn tool_uv_build_backend_without_build_backend() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"

        [tool.uv]
        package = true

        [tool.uv.build-backend.data]
        data = "assets"
    "#})?;

    uv_snapshot!(context.filters(), context.build().arg("--no-build-logs"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution...
    warning: There are settings for the `uv_build` build backend defined in `tool.uv.build-backend`, but the project does not use the `uv_build` backend: [TEMP_DIR]/pyproject.toml
    Building wheel from source distribution...
    warning: There are settings for the `uv_build` build backend defined in `tool.uv.build-backend`, but the project does not use the `uv_build` backend: [CACHE_DIR]/sdists-v9/[TMP]/pyproject.toml
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");

    uv_snapshot!(context.filters(), context.pip_install().arg("."), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    warning: There are settings for the `uv_build` build backend defined in `tool.uv.build-backend`, but the project does not use the `uv_build` backend: [TEMP_DIR]/pyproject.toml
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    ");

    Ok(())
}

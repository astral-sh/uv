use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::{FileTouch, FileWriteBin, FileWriteStr, PathChild, PathCreateDir};
use flate2::bufread::GzDecoder;
use fs_err::File;
use indoc::{formatdoc, indoc};
use insta::{assert_json_snapshot, assert_snapshot};
use std::io::BufReader;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use uv_static::EnvVars;
use uv_test::{uv_snapshot, venv_bin_path};

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
#[cfg(feature = "test-pypi")]
fn built_by_uv_direct_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let built_by_uv = Path::new("../../test/packages/built-by-uv");

    let temp_dir = TempDir::new()?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(temp_dir.path())
        .current_dir(built_by_uv), @"
    success: true
    exit_code: 0
    ----- stdout -----
    built_by_uv-0.1.0-py3-none-any.whl

    ----- stderr -----
    ");

    context
        .pip_install()
        .arg(temp_dir.path().join("built_by_uv-0.1.0-py3-none-any.whl"))
        .assert()
        .success();

    uv_snapshot!(context.python_command()
        .arg("-c")
        .arg(BUILT_BY_UV_TEST_SCRIPT), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello 👋
    Area of a circle with r=2: 12.56636

    ----- stderr -----
    ");

    uv_snapshot!(Command::new("say-hi")
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hi from a script!

    ----- stderr -----
    ");

    Ok(())
}

/// Test that source tree -> source dist -> wheel works.
///
/// We can't test end-to-end here including the PEP 517 bridge code since we don't have a uv wheel,
/// so we call the build backend directly.
#[test]
#[cfg(feature = "test-pypi")]
fn built_by_uv_direct() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let built_by_uv = Path::new("../../test/packages/built-by-uv");

    let sdist_dir = TempDir::new()?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-sdist")
        .arg(sdist_dir.path())
        .current_dir(built_by_uv), @"
    success: true
    exit_code: 0
    ----- stdout -----
    built_by_uv-0.1.0.tar.gz

    ----- stderr -----
    ");

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
        .current_dir(sdist_tree.path().join("built_by_uv-0.1.0")), @"
    success: true
    exit_code: 0
    ----- stdout -----
    built_by_uv-0.1.0-py3-none-any.whl

    ----- stderr -----
    ");

    drop(sdist_tree);

    context
        .pip_install()
        .arg(wheel_dir.path().join("built_by_uv-0.1.0-py3-none-any.whl"))
        .assert()
        .success();

    drop(wheel_dir);

    uv_snapshot!(context.python_command()
        .arg("-c")
        .arg(BUILT_BY_UV_TEST_SCRIPT), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello 👋
    Area of a circle with r=2: 12.56636

    ----- stderr -----
    ");

    Ok(())
}

/// Test that editables work.
///
/// We can't test end-to-end here including the PEP 517 bridge code since we don't have a uv wheel,
/// so we call the build backend directly.
#[test]
#[cfg(feature = "test-pypi")]
fn built_by_uv_editable() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let built_by_uv = Path::new("../../test/packages/built-by-uv");

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
        .current_dir(built_by_uv), @"
    success: true
    exit_code: 0
    ----- stdout -----
    built_by_uv-0.1.0-py3-none-any.whl

    ----- stderr -----
    ");
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
        .current_dir(built_by_uv), @"
    success: true
    exit_code: 0
    ----- stdout -----
    ..
    2 passed in [TIME]

    ----- stderr -----
    ");

    Ok(())
}

#[cfg(all(unix, feature = "test-git"))]
#[test]
fn preserve_executable_bit() -> Result<()> {
    use std::io::Write;

    let context = uv_test::test_context!("3.12");

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
        .env(EnvVars::PATH, venv_bin_path(&context.venv)), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hi from the shell

    ----- stderr -----
    ");

    Ok(())
}

/// Test `tool.uv.build-backend.module-name`.
///
/// We include only the module specified by `module-name`, ignoring the project name and all other
/// potential modules.
#[test]
fn rename_module() -> Result<()> {
    let context = uv_test::test_context!("3.12");
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
        .arg(temp_dir.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    foo-1.0.0-py3-none-any.whl

    ----- stderr -----
    ");

    context
        .pip_install()
        .arg(temp_dir.path().join("foo-1.0.0-py3-none-any.whl"))
        .assert()
        .success();

    // Importing the module with the `module-name` name succeeds.
    uv_snapshot!(context.python_command()
        .arg("-c")
        .arg("import bar"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hi from bar

    ----- stderr -----
    ");

    // Importing the package name fails, it was overridden by `module-name`.
    uv_snapshot!(context.python_command()
        .arg("-c")
        .arg("import foo"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Traceback (most recent call last):
      File "<string>", line 1, in <module>
    ModuleNotFoundError: No module named 'foo'
    "#);

    Ok(())
}

/// Test `tool.uv.build-backend.module-name` for editable builds.
#[test]
fn rename_module_editable_build() -> Result<()> {
    let context = uv_test::test_context!("3.12");
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
        .arg(temp_dir.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    foo-1.0.0-py3-none-any.whl

    ----- stderr -----
    ");

    context
        .pip_install()
        .arg(temp_dir.path().join("foo-1.0.0-py3-none-any.whl"))
        .assert()
        .success();

    // Importing the module with the `module-name` name succeeds.
    uv_snapshot!(context.python_command()
        .arg("-c")
        .arg("import bar"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hi from bar

    ----- stderr -----
    ");

    Ok(())
}

/// Check that the build succeeds even if the module name mismatches by case.
#[test]
fn build_module_name_normalization() -> Result<()> {
    let context = uv_test::test_context!("3.12");

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
        .arg(&wheel_dir), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Expected a Python module at: src/Django_plugin/__init__.py
    ");

    fs_err::create_dir_all(context.temp_dir.join("src/Django_plugin"))?;
    // Error case 2: A matching module, but no `__init__.py`.
    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(&wheel_dir), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Expected a Python module at: src/Django_plugin/__init__.py
    ");

    // Use `Django_plugin` instead of `django_plugin`
    context
        .temp_dir
        .child("src/Django_plugin/__init__.py")
        .write_str(r#"print("Hi from bar")"#)?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(&wheel_dir), @"
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
        .arg("import Django_plugin"), @"
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
            .arg(&wheel_dir), @"
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
    let context = uv_test::test_context!("3.12");
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
        .arg(temp_dir.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    foo-1.0.0.tar.gz

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn sdist_error_without_module() -> Result<()> {
    let context = uv_test::test_context!("3.12");
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
        .arg(temp_dir.path()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Expected a Python module at: src/foo/__init__.py
    ");

    fs_err::create_dir(context.temp_dir.join("src"))?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-sdist")
        .arg(temp_dir.path()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Expected a Python module at: src/foo/__init__.py
    ");

    Ok(())
}

#[test]
fn complex_namespace_packages() -> Result<()> {
    let context = uv_test::test_context!("3.12");
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
        @"
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
        @"
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
        @"
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
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    2

    ----- stderr -----
    "
    );
    Ok(())
}

#[test]
fn license_glob_without_matches_errors() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project = context.temp_dir.child("missing-license");
    context
        .init()
        .arg("--lib")
        .arg(project.path())
        .assert()
        .success();

    project
        .child("LICENSE.txt")
        .write_str("permissive license")?;

    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "missing-license"
        version = "1.0.0"
        license-files = ["abc", "LICENSE.txt"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(context.temp_dir.path())
        .current_dir(project.path()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid project metadata
      Caused by: `project.license-files` glob `abc` did not match any files
    ");

    Ok(())
}

#[test]
fn license_file_must_be_utf8() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project = context.temp_dir.child("license-utf8");
    context
        .init()
        .arg("--lib")
        .arg(project.path())
        .assert()
        .success();

    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "license-utf8"
        version = "1.0.0"
        license-files = ["LICENSE.bin"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
        "#
    })?;

    project.child("LICENSE.bin").write_binary(&[0xff])?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg(context.temp_dir.path())
        .current_dir(project.path()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Invalid project metadata
      Caused by: License file `LICENSE.bin` must be UTF-8 encoded
    ");

    Ok(())
}

/// Test that a symlinked file (here: license) gets included.
#[test]
#[cfg(unix)]
fn symlinked_file() -> Result<()> {
    let context = uv_test::test_context!("3.12");

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
        .current_dir(project.path()), @"
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
        .current_dir(project.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    project-1.0.0-py3-none-any.whl

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.pip_install().arg("project-1.0.0-py3-none-any.whl"), @"
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
    let context = uv_test::test_context!("3.12");

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
    uv_snapshot!(context.filters(), context.lock(), @"
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
    let context = uv_test::test_context!("3.12");

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

    uv_snapshot!(context.filters(), context.build(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution (uv build backend)...
      × Failed to build `[TEMP_DIR]/`
      ╰─▶ Module root must be inside the project: ..
    ");

    uv_snapshot!(context.filters(), context.build().arg("--wheel"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building wheel (uv build backend)...
      × Failed to build `[TEMP_DIR]/`
      ╰─▶ Module root must be inside the project: ..
    ");

    Ok(())
}

/// Error when there is a relative data directory outside the project root, such as
/// `tool.uv.build-backend.data.headers = "../headers"`.
#[test]
fn error_on_relative_data_dir_outside_project_root() -> Result<()> {
    let context = uv_test::test_context!("3.12");

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

    uv_snapshot!(context.filters(), context.build().arg("project"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution (uv build backend)...
      × Failed to build `[TEMP_DIR]/project`
      ╰─▶ The path for the data directory headers must be inside the project: ../header
    ");

    uv_snapshot!(context.filters(), context.build().arg("project").arg("--wheel"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building wheel (uv build backend)...
      × Failed to build `[TEMP_DIR]/project`
      ╰─▶ The path for the data directory headers must be inside the project: ../header
    ");

    Ok(())
}

/// Show an explicit error when there is a venv in source tree.
#[test]
fn venv_in_source_tree() {
    let context = uv_test::test_context!("3.12");

    context
        .init()
        .arg("--lib")
        .arg("--name")
        .arg("foo")
        .assert()
        .success();

    context
        .venv()
        .arg(context.temp_dir.join("src").join("foo").join(".venv"))
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.build(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution (uv build backend)...
      × Failed to build `[TEMP_DIR]/`
      ╰─▶ Virtual environments must not be added to source distributions or wheels, remove the directory or exclude it from the build: src/foo/.venv
    ");

    uv_snapshot!(context.filters(), context.build().arg("--wheel"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building wheel (uv build backend)...
      × Failed to build `[TEMP_DIR]/`
      ╰─▶ Virtual environments must not be added to source distributions or wheels, remove the directory or exclude it from the build: src/foo/.venv
    ");
}

/// Show a warning when the build backend is passed redundant module names
#[test]
fn warn_on_redundant_module_names() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"

        [tool.uv.build-backend]
        module-name = ["foo", "foo.bar", "foo", "foo.bar.baz", "foobar", "bar", "foobar.baz", "baz.bar"]
    "#})?;

    let foo_module = context.temp_dir.child("src/foo");
    foo_module.create_dir_all()?;
    foo_module.child("__init__.py").touch()?;

    let foobar_module = context.temp_dir.child("src/foobar");
    foobar_module.create_dir_all()?;
    foobar_module.child("__init__.py").touch()?;

    let bazbar_module = context.temp_dir.child("src/baz/bar");
    bazbar_module.create_dir_all()?;
    bazbar_module.child("__init__.py").touch()?;

    let bar_module = context.temp_dir.child("src/bar");
    bar_module.create_dir_all()?;
    bar_module.child("__init__.py").touch()?;

    // Warnings should be printed when invoking `uv build`
    uv_snapshot!(context.filters(), context.build(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution (uv build backend)...
    warning: Ignoring redundant module names in `tool.uv.build-backend.module-name`: `foo.bar`, `foo`, `foo.bar.baz`, `foobar.baz`
    Building wheel from source distribution (uv build backend)...
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");

    // But warnings shouldn't be printed in cases when the user might not
    // control the thing being built. Sources being enabled is a workable proxy
    // for this.
    uv_snapshot!(context.filters(), context.build().arg("--no-sources"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Building source distribution (uv build backend)...
    Building wheel from source distribution (uv build backend)...
    Successfully built dist/project-0.1.0.tar.gz
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");

    Ok(())
}

#[test]
fn invalid_pyproject_toml() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("child")
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = 1
        version = "1.0.0"

        [build-system]
        requires = ["uv_build>=0.9,<10000"]
        build-backend = "uv_build"
    "#})?;

    uv_snapshot!(context.filters(), context.build().arg("child"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Building source distribution (uv build backend)...
      × Failed to build `[TEMP_DIR]/child`
      ├─▶ Invalid metadata format in: child/pyproject.toml
      ╰─▶ TOML parse error at line 2, column 8
            |
          2 | name = 1
            |        ^
          invalid type: integer `1`, expected a string
    ");

    Ok(())
}

#[cfg(feature = "test-pypi")]
#[test]
fn build_with_all_metadata() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let temp_dir = TempDir::new()?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "foo"
        version = "1.0.0"
        description = "A Python package with all metadata fields"
        readme = "Readme.md"
        requires-python = ">=3.12"
        license = "MIT OR Apache-2.0"
        license-files = ["License*"]
        authors = [
            {name = "Jane Doe", email = "jane@example.com"},
            {name = "John Doe"},
            {email = "info@example.com"},
        ]
        maintainers = [
            {name = "ferris", email = "ferris@example.com"},
        ]
        keywords = ["example", "test", "metadata"]
        classifiers = [
            "Development Status :: 4 - Beta",
            "Programming Language :: Python :: 3",
            "Programming Language :: Python :: 3.12",
        ]
        dependencies = [
            "anyio>=4,<5",
        ]

        [project.optional-dependencies]
        dev = ["pytest>=7.0"]

        [project.urls]
        Homepage = "https://octocat.github.io/spoon-knife"
        Repository = "https://github.com/octocat/Spoon-Knife"
        Changelog = "https://github.com/octocat/Spoon-Knife/blob/main/CHANGELOG.md"

        [project.scripts]
        foo-cli = "foo:main"

        [project.gui-scripts]
        foo-gui = "foo:gui_main"

        [project.entry-points."foo.plugins"]
        bar = "foo:bar_plugin"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;
    context
        .temp_dir
        .child("src/foo/__init__.py")
        .write_str(indoc! {r#"
        def main():
            print("Hello from foo!")

        def gui_main():
            print("GUI main")

        def bar_plugin():
            pass
    "#})?;
    context
        .temp_dir
        .child("License.txt")
        .write_str("MIT License")?;
    context
        .temp_dir
        .child("Readme.md")
        .write_str("Hello World!")?;

    uv_snapshot!(context
        .build_backend()
        .arg("build-wheel")
        .arg("--preview-features")
        .arg("metadata-json")
        .arg(temp_dir.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    foo-1.0.0-py3-none-any.whl

    ----- stderr -----
    ");

    context
        .pip_install()
        .arg(temp_dir.path().join("foo-1.0.0-py3-none-any.whl"))
        .assert()
        .success();

    let metadata = fs_err::read_to_string(
        context
            .site_packages()
            .join("foo-1.0.0.dist-info")
            .join("METADATA"),
    )?;
    assert_snapshot!(metadata, @"
    Metadata-Version: 2.4
    Name: foo
    Version: 1.0.0
    Summary: A Python package with all metadata fields
    Keywords: example,test,metadata
    Author: Jane Doe, John Doe
    Author-email: Jane Doe <jane@example.com>, info@example.com
    License-Expression: MIT OR Apache-2.0
    License-File: License.txt
    Classifier: Development Status :: 4 - Beta
    Classifier: Programming Language :: Python :: 3
    Classifier: Programming Language :: Python :: 3.12
    Requires-Dist: anyio>=4,<5
    Requires-Dist: pytest>=7.0 ; extra == 'dev'
    Maintainer: ferris
    Maintainer-email: ferris <ferris@example.com>
    Requires-Python: >=3.12
    Project-URL: Homepage, https://octocat.github.io/spoon-knife
    Project-URL: Repository, https://github.com/octocat/Spoon-Knife
    Project-URL: Changelog, https://github.com/octocat/Spoon-Knife/blob/main/CHANGELOG.md
    Provides-Extra: dev
    Description-Content-Type: text/markdown

    Hello World!
    ");
    let metadata_json = fs_err::read_to_string(
        context
            .site_packages()
            .join("foo-1.0.0.dist-info")
            .join("METADATA.json"),
    )?;
    let metadata_json: serde_json::Value = serde_json::from_str(&metadata_json)?;
    assert_json_snapshot!(metadata_json, @r#"
    {
      "author": "Jane Doe, John Doe",
      "author_email": "Jane Doe <jane@example.com>, info@example.com",
      "classifiers": [
        "Development Status :: 4 - Beta",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.12"
      ],
      "description": "Hello World!",
      "description_content_type": "text/markdown",
      "download_url": null,
      "dynamic": [],
      "home_page": null,
      "keywords": [
        "example",
        "test",
        "metadata"
      ],
      "license": null,
      "license_expression": "MIT OR Apache-2.0",
      "license_files": [
        "License.txt"
      ],
      "maintainer": "ferris",
      "maintainer_email": "ferris <ferris@example.com>",
      "metadata_version": "2.4",
      "name": "foo",
      "obsoletes_dist": [],
      "platforms": [],
      "project_urls": {
        "Changelog": "https://github.com/octocat/Spoon-Knife/blob/main/CHANGELOG.md",
        "Homepage": "https://octocat.github.io/spoon-knife",
        "Repository": "https://github.com/octocat/Spoon-Knife"
      },
      "provides_dist": [],
      "provides_extra": [
        "dev"
      ],
      "requires_dist": [
        "anyio>=4,<5",
        "pytest>=7.0 ; extra == 'dev'"
      ],
      "requires_external": [],
      "requires_python": ">=3.12",
      "summary": "A Python package with all metadata fields",
      "supported_platforms": [],
      "version": "1.0.0"
    }
    "#);
    let wheel = fs_err::read_to_string(
        context
            .site_packages()
            .join("foo-1.0.0.dist-info")
            .join("WHEEL"),
    )?;
    let wheel = wheel.replace(uv_version::version(), "[VERSION]");
    assert_snapshot!(wheel, @"
    Wheel-Version: 1.0
    Generator: uv [VERSION]
    Root-Is-Purelib: true
    Tag: py3-none-any
    ");
    let wheel_json = fs_err::read_to_string(
        context
            .site_packages()
            .join("foo-1.0.0.dist-info")
            .join("WHEEL.json"),
    )?;
    let wheel_json = wheel_json.replace(uv_version::version(), "[VERSION]");
    let wheel_json: serde_json::Value = serde_json::from_str(&wheel_json)?;
    assert_json_snapshot!(wheel_json, @r#"
    {
      "generator": "uv [VERSION]",
      "root-is-purelib": true,
      "tags": [
        "py3-none-any"
      ],
      "wheel-version": "1.0"
    }
    "#);

    Ok(())
}

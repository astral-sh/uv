use crate::common::{TestContext, site_packages_path, uv_snapshot};
use anyhow::Result;
use assert_fs::prelude::{FileWriteStr, PathChild};
use indoc::{formatdoc, indoc};
use uv_static::EnvVars;

/// Test that `find_uv_bin` works in various installation scenarios.
///
/// We combine all these cases into a single test because building the `fake-uv` package is
/// expensive (we need to construct an archive with a pretty large debug binary) and we can share a
/// cached build across all the cases.
///
/// This test requires symlinks, so it's only run on Unix.
#[cfg(unix)]
#[test]
fn find_uv_bin() -> Result<()> {
    let context = TestContext::new("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix();

    // Create a requirements.txt file to avoid forced rebuilds
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "{}",
        context
            .workspace_root
            .join("scripts/packages/fake-uv")
            .display()
    ))?;

    // Install in a virtual environment
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/scripts/packages/fake-uv)
    "
    );

    // We should find the binary in the virtual environment
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg("import uv; print(uv.find_uv_bin())"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    [VENV]/[BIN]/uv

    ----- stderr -----
    "
    );

    // Remove the virtual environment to avoid polluting subsequent tests
    fs_err::remove_dir_all(&context.venv)?;

    // Install in a target directory
    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--target")
        .arg("target"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/scripts/packages/fake-uv)
    "
    );

    // We should find the binary in the target directory
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg("import uv; print(uv.find_uv_bin())")
        .env(EnvVars::PYTHONPATH, context.temp_dir.child("target").path()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    /Users/zb/.local/[BIN]/uv

    ----- stderr -----
    "
    );

    // Install in a prefix directory
    let prefix = context.temp_dir.child("prefix");

    uv_snapshot!(context.filters(), context.pip_install()
        .arg("-r")
        .arg("requirements.txt")
        .arg("--prefix")
        .arg(prefix.path()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/scripts/packages/fake-uv)
    "
    );

    // We should find the binary in the prefix directory
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg("import uv; print(uv.find_uv_bin())")
        .env(
            EnvVars::PYTHONPATH,
            site_packages_path(&context.temp_dir.join("prefix"), "python3.12"),
        ), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    /Users/zb/.local/[BIN]/uv

    ----- stderr -----
    "
    );

    // Create a minimal pyproject.toml
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "test-project"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = []
        "#
    })?;

    // We should find the binary in an ephemeral `--with` environment
    uv_snapshot!(context.filters(), context.run()
        .arg("--with")
        .arg(context.workspace_root.join("scripts/packages/fake-uv"))
        .arg("python")
        .arg("-c")
        .arg("import uv; print(uv.find_uv_bin())"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 1 package in [TIME]
    Audited in [TIME]
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/scripts/packages/fake-uv)
    Traceback (most recent call last):
      File "<string>", line 1, in <module>
      File "[CACHE_DIR]/archive-v0/KAPXYug5EtMBzhbPVqTGD/lib/[PYTHON]/site-packages/uv/_find_uv.py", line 36, in find_uv_bin
        raise FileNotFoundError(path)
    FileNotFoundError: [HOME]/.local/[BIN]/uv
    "#
    );

    // Add the fake-uv package as a dependency
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! { r#"
        [project]
        name = "test-project"
        version = "1.0.0"
        requires-python = ">=3.8"
        dependencies = ["uv"]

        [tool.uv.sources]
        uv = {{ path = "{}" }}
        "#,
        context.workspace_root.join("scripts/packages/fake-uv").display()
    })?;

    // When running in an ephemeral environment, we should find the binary in the project
    // environment
    uv_snapshot!(context.filters(), context.run()
        .arg("--with")
        .arg("anyio")
        .arg("python")
        .arg("-c")
        .arg("import uv; print(uv.find_uv_bin())"),
     @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Installed 1 package in [TIME]
     + uv==0.1.0 (from file://[WORKSPACE]/scripts/packages/fake-uv)
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    Traceback (most recent call last):
      File "<string>", line 1, in <module>
      File "[SITE_PACKAGES]/uv/_find_uv.py", line 36, in find_uv_bin
        raise FileNotFoundError(path)
    FileNotFoundError: [HOME]/.local/[BIN]/uv
    "#
    );

    Ok(())
}

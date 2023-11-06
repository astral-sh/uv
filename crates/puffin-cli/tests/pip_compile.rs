#![cfg(all(feature = "python", feature = "pypi"))]

use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use insta_cmd::_macro_support::insta;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

use common::{BIN_NAME, INSTA_FILTERS};

mod common;

#[test]
fn missing_requirements_in() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let requirements_in = temp_dir.child("requirements.in");

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-compile")
        .arg("requirements.in")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir));

    requirements_in.assert(predicates::path::missing());

    Ok(())
}

#[test]
fn missing_venv() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
        .arg("pip-compile")
        .arg("requirements.in")
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .env("VIRTUAL_ENV", venv.as_os_str())
        .current_dir(&temp_dir));

    venv.assert(predicates::path::missing());

    Ok(())
}

/// Resolve a specific version of Django from a `requirements.in` file.
#[test]
fn compile_requirements_in() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("django==5.0b1")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a specific version of Django from a `pyproject.toml` file.
#[test]
fn compile_pyproject_toml() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[build-system]
requires = ["setuptools", "wheel"]

[project]
name = "project"
dependencies = [
    "django==5.0b1",
]
"#,
    )?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("pyproject.toml")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a package from a `requirements.in` file, with a `constraints.txt` file.
#[test]
fn compile_constraints_txt() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("django==5.0b1")?;

    let constraints_txt = temp_dir.child("constraints.txt");
    constraints_txt.touch()?;
    constraints_txt.write_str("sqlparse<0.4.4")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--constraint")
            .arg("constraints.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a package from a `requirements.in` file, with an inline constraint.
#[test]
fn compile_constraints_inline() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("django==5.0b1")?;
    requirements_in.write_str("-c constraints.txt")?;

    let constraints_txt = temp_dir.child("constraints.txt");
    constraints_txt.touch()?;
    constraints_txt.write_str("sqlparse<0.4.4")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a package from a `requirements.in` file, with a `constraints.txt` file that
/// uses markers.
#[test]
fn compile_constraints_markers() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("anyio")?;

    // Constrain a transitive dependency based on the Python version
    let constraints_txt = temp_dir.child("constraints.txt");
    constraints_txt.touch()?;
    // If constraints are ignored, these will conflict
    constraints_txt.write_str("sniffio==1.2.0;python_version<='3.7'")?;
    constraints_txt.write_str("sniffio==1.3.0;python_version>'3.7'")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--constraint")
            .arg("constraints.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a package from an optional dependency group in a `pyproject.toml` file.
#[test]
fn compile_pyproject_toml_extra() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[build-system]
requires = ["setuptools", "wheel"]

[project]
name = "project"
dependencies = []
optional-dependencies.foo = [
    "django==5.0b1",
]
"#,
    )?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("pyproject.toml")
            .arg("--extra")
            .arg("foo")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a package from an extra with unnormalized names in a `pyproject.toml` file.
#[test]
fn compile_pyproject_toml_extra_name_normalization() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[build-system]
requires = ["setuptools", "wheel"]

[project]
name = "project"
dependencies = []
optional-dependencies."FrIeNdLy-._.-bArD" = [
    "django==5.0b1",
]
"#,
    )?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("pyproject.toml")
            .arg("--extra")
            .arg("FRiENDlY-...-_-BARd")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Request an extra that does not exist as a dependency group in a `pyproject.toml` file.
#[test]
fn compile_pyproject_toml_extra_missing() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[build-system]
requires = ["setuptools", "wheel"]

[project]
name = "project"
dependencies = []
optional-dependencies.foo = [
    "django==5.0b1",
]
"#,
    )?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("pyproject.toml")
            .arg("--extra")
            .arg("bar")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Request multiple extras that do not exist as a dependency group in a `pyproject.toml` file.
#[test]
fn compile_pyproject_toml_extras_missing() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[build-system]
requires = ["setuptools", "wheel"]

[project]
name = "project"
dependencies = []
optional-dependencies.foo = [
    "django==5.0b1",
]
"#,
    )?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("pyproject.toml")
            .arg("--extra")
            .arg("foo")
            .arg("--extra")
            .arg("bar")
            .arg("--extra")
            .arg("foobar")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Request extras when using a `requirements.in` file which does not support extras.
#[test]
fn compile_requirements_file_extra() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("django==5.0b1")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .arg("--all-extras")
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir),
            @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: Requesting extras requires a pyproject.toml input file.
        "###);
    });

    Ok(())
}

/// Request an extra with a name that does not conform to the specification.
#[test]
fn invalid_extra_name() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[build-system]
requires = ["setuptools", "wheel"]

[project]
name = "project"
dependencies = []
optional-dependencies.foo = [
    "django==5.0b1",
]
"#,
    )?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("pyproject.toml")
            .arg("--extra")
            .arg("invalid name!")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a specific Flask wheel via a URL dependency.
#[test]
fn compile_wheel_url_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("flask @ https://files.pythonhosted.org/packages/36/42/015c23096649b908c809c69388a805a571a3bea44362fe87e33fc3afa01f/flask-3.0.0-py3-none-any.whl")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a specific Flask source distribution via a URL dependency.
#[test]
fn compile_sdist_url_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("flask @ https://files.pythonhosted.org/packages/d8/09/c1a7354d3925a3c6c8cfdebf4245bae67d633ffda1ba415add06ffc839c5/flask-3.0.0.tar.gz")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a specific Flask source distribution via a Git HTTPS dependency.
#[test]
#[cfg(feature = "git")]
fn compile_git_https_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("flask @ git+https://github.com/pallets/flask.git")?;

    // In addition to the standard filters, remove the `main` commit, which will change frequently.
    let mut filters = INSTA_FILTERS.to_vec();
    filters.push((r"@(\d|\w){40}", "@[COMMIT]"));

    insta::with_settings!({
        filters => filters
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a specific Flask branch via a Git HTTPS dependency.
#[test]
#[cfg(feature = "git")]
fn compile_git_branch_https_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("flask @ git+https://github.com/pallets/flask.git@1.0.x")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a specific Flask tag via a Git HTTPS dependency.
#[test]
#[cfg(feature = "git")]
fn compile_git_tag_https_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("flask @ git+https://github.com/pallets/flask.git@3.0.0")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a specific Flask commit via a Git HTTPS dependency.
#[test]
#[cfg(feature = "git")]
fn compile_git_long_commit_https_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str(
        "flask @ git+https://github.com/pallets/flask.git@d92b64aa275841b0c9aea3903aba72fbc4275d91",
    )?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a specific Flask commit via a Git HTTPS dependency.
#[test]
#[cfg(feature = "git")]
fn compile_git_short_commit_https_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("flask @ git+https://github.com/pallets/flask.git@d92b64a")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a specific Flask ref via a Git HTTPS dependency.
#[test]
#[cfg(feature = "git")]
fn compile_git_refs_https_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in
        .write_str("flask @ git+https://github.com/pallets/flask.git@refs/pull/5313/head")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a specific Git dependency with a subdirectory.
#[test]
#[cfg(feature = "git")]
fn compile_git_subdirectory_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("example-pkg-a @ git+https://github.com/pypa/sample-namespace-packages.git@df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45#subdirectory=pkg_resources/pkg_a")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve two packages from a `requirements.in` file with the same Git HTTPS dependency.
#[test]
#[cfg(feature = "git")]
fn compile_git_concurrent_access() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in
        .write_str("example-pkg-a @ git+https://github.com/pypa/sample-namespace-packages.git@df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45#subdirectory=pkg_resources/pkg_a\nexample-pkg-b @ git+https://github.com/pypa/sample-namespace-packages.git@df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45#subdirectory=pkg_resources/pkg_b")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve a Git dependency with a declared name that differs from the true name of the package.
#[test]
#[cfg(feature = "git")]
fn compile_git_mismatched_name() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in
        .write_str("flask @ git+https://github.com/pallets/flask.git@2.0.0\ndask @ git+https://github.com/pallets/flask.git@3.0.0")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Request Flask, but include a URL dependency for Werkzeug, which should avoid adding a
/// duplicate dependency from `PyPI`.
#[test]
fn mixed_url_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("flask==3.0.0\nwerkzeug @ https://files.pythonhosted.org/packages/c3/fc/254c3e9b5feb89ff5b9076a23218dafbc99c96ac5941e900b71206e6313b/werkzeug-3.0.1-py3-none-any.whl")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Request Werkzeug via both a version and a URL dependency at a _different_ version, which
/// should result in a conflict.
#[test]
fn conflicting_direct_url_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("werkzeug==3.0.0\nwerkzeug @ https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Request Werkzeug via both a version and a URL dependency at _the same_ version, which
/// should prefer the direct URL dependency.
#[test]
fn compatible_direct_url_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("werkzeug==2.0.0\nwerkzeug @ https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Request Werkzeug via two different URLs at different versions, which should result in a conflict.
#[test]
fn conflicting_repeated_url_dependency_version_mismatch() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("werkzeug @ https://files.pythonhosted.org/packages/bd/24/11c3ea5a7e866bf2d97f0501d0b4b1c9bbeade102bb4b588f0d2919a5212/Werkzeug-2.0.1-py3-none-any.whl\nwerkzeug @ https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Request Werkzeug via two different URLs at the same version. Despite mapping to the same
/// version, it should still result in a conflict.
#[test]
#[cfg(feature = "git")]
fn conflicting_repeated_url_dependency_version_match() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("werkzeug @ git+https://github.com/pallets/werkzeug.git@2.0.0 \nwerkzeug @ https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Request Flask, but include a URL dependency for a conflicting version of Werkzeug.
#[test]
fn conflicting_transitive_url_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("flask==3.0.0\nwerkzeug @ https://files.pythonhosted.org/packages/ff/1d/960bb4017c68674a1cb099534840f18d3def3ce44aed12b5ed8b78e0153e/Werkzeug-2.0.0-py3-none-any.whl")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Request `transitive_url_dependency`, which depends on `git+https://github.com/pallets/werkzeug@2.0.0`.
/// Since this URL isn't declared upfront, we should reject it.
#[test]
#[cfg(feature = "git")]
fn disallowed_transitive_url_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("transitive_url_dependency @ https://github.com/astral-sh/ruff/files/13257454/transitive_url_dependency.zip")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Request `transitive_url_dependency`, which depends on `git+https://github.com/pallets/werkzeug@2.0.0`.
/// Since this URL is declared as a constraint, we should accept it.
#[test]
#[cfg(feature = "git")]
fn allowed_transitive_url_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("transitive_url_dependency @ https://github.com/astral-sh/ruff/files/13257454/transitive_url_dependency.zip")?;

    let constraints_txt = temp_dir.child("constraints.txt");
    constraints_txt.touch()?;
    constraints_txt.write_str("werkzeug @ git+https://github.com/pallets/werkzeug@2.0.0")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--constraint")
            .arg("constraints.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Request `transitive_url_dependency`, which depends on `git+https://github.com/pallets/werkzeug@2.0.0`.
/// Since this `git+https://github.com/pallets/werkzeug@2.0.0.git` is declared as a constraint, and
/// those map to the same canonical URL, we should accept it.
#[test]
#[cfg(feature = "git")]
fn allowed_transitive_canonical_url_dependency() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let requirements_in = temp_dir.child("requirements.in");
    requirements_in.touch()?;
    requirements_in.write_str("transitive_url_dependency @ https://github.com/astral-sh/ruff/files/13257454/transitive_url_dependency.zip")?;

    let constraints_txt = temp_dir.child("constraints.txt");
    constraints_txt.touch()?;
    constraints_txt.write_str("werkzeug @ git+https://github.com/pallets/werkzeug.git@2.0.0")?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("requirements.in")
            .arg("--constraint")
            .arg("constraints.txt")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve packages from all optional dependency groups in a `pyproject.toml` file.
#[test]
fn compile_pyproject_toml_all_extras() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[build-system]
requires = ["setuptools", "wheel"]

[project]
name = "project"
dependencies = ["django==5.0b1"]
optional-dependencies.foo = [
    "anyio==4.0.0",
]
optional-dependencies.bar = [
    "httpcore==0.18.0",
]
"#,
    )?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("pyproject.toml")
            .arg("--all-extras")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Resolve packages from all optional dependency groups in a `pyproject.toml` file.
#[test]
fn compile_does_not_allow_both_extra_and_all_extras() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[build-system]
requires = ["setuptools", "wheel"]

[project]
name = "project"
dependencies = ["django==5.0b1"]
optional-dependencies.foo = [
    "anyio==4.0.0",
]
optional-dependencies.bar = [
    "httpcore==0.18.0",
]
"#,
    )?;

    insta::with_settings!({
        filters => INSTA_FILTERS.to_vec()
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("pyproject.toml")
            .arg("--all-extras")
            .arg("--extra")
            .arg("foo")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir),
            @r###"
        success: false
        exit_code: 2
        ----- stdout -----

        ----- stderr -----
        error: the argument '--all-extras' cannot be used with '--extra <EXTRA>'

        Usage: puffin pip-compile --all-extras --cache-dir [CACHE_DIR]

        For more information, try '--help'.
        "###);
    });

    Ok(())
}

/// Compile requirements that cannot be solved due to conflict in a `pyproject.toml` fil;e.
#[test]
fn compile_unsolvable_requirements() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[build-system]
requires = ["setuptools", "wheel"]

[project]
name = "my-project"
dependencies = ["django==5.0b1", "django==5.0a1"]
"#,
    )?;

    insta::with_settings!({
        filters => vec![
            (r"\d(ms|s)", "[TIME]"),
            (r"#    .* pip-compile", "#    [BIN_PATH] pip-compile"),
            (r"--cache-dir .*", "--cache-dir [CACHE_DIR]"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("pyproject.toml")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

/// Compile requirements in a `pyproject.toml` file that cannot be resolved due to
/// a requirement with a version that is not available online.
#[test]
fn compile_unsolvable_requirements_version_not_available() -> Result<()> {
    let temp_dir = assert_fs::TempDir::new()?;
    let cache_dir = assert_fs::TempDir::new()?;
    let venv = temp_dir.child(".venv");

    Command::new(get_cargo_bin(BIN_NAME))
        .arg("venv")
        .arg(venv.as_os_str())
        .arg("--cache-dir")
        .arg(cache_dir.path())
        .current_dir(&temp_dir)
        .assert()
        .success();
    venv.assert(predicates::path::is_dir());

    let pyproject_toml = temp_dir.child("pyproject.toml");
    pyproject_toml.touch()?;
    pyproject_toml.write_str(
        r#"[build-system]
requires = ["setuptools", "wheel"]

[project]
name = "my-project"
dependencies = ["django==300.1.4"]
"#,
    )?;

    insta::with_settings!({
        filters => vec![
            (r"\d(ms|s)", "[TIME]"),
            (r"#    .* pip-compile", "#    [BIN_PATH] pip-compile"),
            (r"--cache-dir .*", "--cache-dir [CACHE_DIR]"),
        ]
    }, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .arg("pip-compile")
            .arg("pyproject.toml")
            .arg("--cache-dir")
            .arg(cache_dir.path())
            .env("VIRTUAL_ENV", venv.as_os_str())
            .current_dir(&temp_dir));
    });

    Ok(())
}

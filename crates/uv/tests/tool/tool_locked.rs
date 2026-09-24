use std::collections::BTreeMap;

#[cfg(feature = "test-pypi")]
use anyhow::Context;
use anyhow::Result;
#[cfg(feature = "test-pypi")]
use assert_cmd::assert::OutputAssertExt;
#[cfg(feature = "test-pypi")]
use assert_fs::fixture::FileWriteStr;
use assert_fs::fixture::{FileWriteBin, PathChild, PathCreateDir};
use indoc::{formatdoc, indoc};
use insta::allow_duplicates;
#[cfg(feature = "test-pypi")]
use insta::assert_snapshot;

use uv_static::EnvVars;
use uv_test::packse::generate_wheel_with_files;
use uv_test::{TestContext, uv_snapshot};

#[cfg(feature = "test-pypi")]
fn dependency(context: &TestContext, version: &str) -> Result<String> {
    context
        .temp_dir
        .child("requirements.in")
        .write_str(&format!("idna=={version}"))?;
    context
        .pip_compile()
        .args(["requirements.in", "--no-deps", "-o", "pylock.toml"])
        .assert()
        .success();
    Ok(context.read("pylock.toml"))
}

fn tool(
    context: &TestContext,
    version: &str,
    lock: Option<&str>,
    python: Option<&str>,
) -> Result<()> {
    let pylock_path = format!("locked_tool-{version}.dist-info/pylock.toml");
    let entrypoints_path = format!("locked_tool-{version}.dist-info/entry_points.txt");
    let mut files = vec![
        (
            entrypoints_path.as_str(),
            "[console_scripts]\nlocked-tool = locked_tool.cli:main\n",
        ),
        (
            "locked_tool/cli.py",
            "from importlib.metadata import version\ndef main(): print(version('idna'))\n",
        ),
    ];
    if let Some(lock) = lock {
        files.push((&pylock_path, lock));
    }
    let (filename, bytes) = generate_wheel_with_files(
        &"locked-tool".parse()?,
        &version.parse()?,
        &["idna>=3.3,<3.5".parse()?],
        &BTreeMap::new(),
        python.map(str::parse).transpose()?.as_ref(),
        "py3-none-any",
        &files,
    );
    context
        .temp_dir
        .child("wheels")
        .child(filename)
        .write_binary(&bytes)?;
    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn packaged_lock_install_run_upgrade() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let bin_dir = context.temp_dir.child("bin");
    let lock = dependency(&context, "3.3")?;
    let newer_lock = dependency(&context, "3.4")?;
    tool(&context, "1.0.0", Some(&lock), None)?;
    // Compatibility may eliminate a release before its lock is considered.
    tool(&context, "2.0.0", None, Some(">=3.13"))?;

    uv_snapshot!(context.filters(), context.tool_install()
        .args(["locked-tool", "--find-links", "wheels"])
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + idna==3.4
     + locked-tool==1.0.0
    Installed 1 executable: locked-tool
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "locked-tool"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    3.3

    ----- stderr -----
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + idna==3.3
     + locked-tool==1.0.0
    ");

    // An existing installation resolved normally must be brought back to the packaged pins.
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "locked-tool", "--find-links", "wheels"])
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - idna==3.4
     + idna==3.3
    Installed 1 executable: locked-tool
    ");
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(context.read("tools/locked-tool/uv-receipt.toml"), @r#"
        [tool]
        locked = true
        requirements = [{ name = "locked-tool" }]
        entrypoints = [
            { name = "locked-tool", install-path = "[TEMP_DIR]/bin/locked-tool", from = "locked-tool" },
        ]

        [tool.options]
        find-links = ["file://[TEMP_DIR]/wheels"]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    tool(&context, "1.1.0", Some(&newer_lock), None)?;
    // The receipt keeps upgrades locked even without repeating --locked.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .args(["--preview-features", "locked-tools", "locked-tool"])
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Updated locked-tool v1.0.0 -> v1.1.0
     - idna==3.3
     + idna==3.4
     - locked-tool==1.0.0
     + locked-tool==1.1.0
    Installed 1 executable: locked-tool
    ");
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .args(["--locked", "--preview-features", "locked-tools", "locked-tool"])
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Nothing to upgrade
    ");
    Ok(())
}

#[test]
fn packaged_lock_required() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let lock = indoc! {r#"
        lock-version = "1.0"
        created-by = "test"
        requires-python = ">=3.12"
        packages = []
    "#};
    tool(&context, "1.0.0", Some(lock), None)?;
    tool(&context, "2.0.0", None, None)?;

    uv_snapshot!(context.filters(), context.tool_install().args(["--locked", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `--locked` for tools requires the `locked-tools` preview feature
    ");
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `--locked` for tools requires the `locked-tools` preview feature
    ");
    uv_snapshot!(context.filters(), context.tool_upgrade().args(["--locked", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `--locked` for tools requires the `locked-tools` preview feature
    ");
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "python"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `--locked` requires a tool package with a bundled lock, not a Python interpreter
    ");
    // A compatible release without a lock cannot cause a fallback to an older release.
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--no-index", "--find-links", "wheels", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `locked-tool==2.0.0` does not contain `pylock.toml` in its `.dist-info` directory; `--locked` requires a packaged lock
    ");

    tool(&context, "3.0.0", Some("not valid TOML"), None)?;
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "--no-index", "--find-links", "wheels", "locked-tool==3.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `locked-tool==3.0.0` contains an invalid `pylock.toml`
      cause: TOML parse error at line 1, column 5
               |
             1 | not valid TOML
               |     ^
             key with no value, expected `=`
    ");

    tool(
        &context,
        "4.0.0",
        Some(&lock.replace(">=3.12", ">=3.13")),
        None,
    )?;
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--no-index", "--find-links", "wheels", "locked-tool==4.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The requested interpreter resolved to Python 3.12.[X], which is incompatible with the `pylock.toml`'s Python requirement: `>=3.13`
    ");

    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--no-index", "--find-links", "wheels", "--with", "locked-dependency", "locked-tool==1.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `--locked` requires a single tool package and cannot be combined with `--with`
    ");
    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn packaged_lock_hashes() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let lock = dependency(&context, "3.3")?;
    tool(&context, "1.0.0", Some(&lock), None)?;
    // Change the expected digest while retaining a valid lock and artifact.
    let mut lock: toml::Value = toml::from_str(&lock)?;
    lock["packages"][0]["wheels"][0]["hashes"]["sha256"] = toml::Value::String("0".repeat(64));
    tool(&context, "2.0.0", Some(&toml::to_string(&lock)?), None)?;
    let bin_dir = context.temp_dir.child("bin");
    context
        .tool_install()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "locked-tool==1.0.0",
        ])
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "locked-tool==2.0.0"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to download `idna==3.3`
      cause: Hash mismatch for `idna==3.3`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000

             Computed:
               sha256:84d9dd047ffa80596e0f246e2eab0b391788b0503584e8945f2368256d2735ff
    ");

    lock["packages"][0]["wheels"][0]["hashes"] = toml::Value::Table(toml::Table::new());
    tool(&context, "3.0.0", Some(&toml::to_string(&lock)?), None)?;
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "locked-tool==3.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: Empty hash tables in `pylock.toml` will be rejected in a future uv version. Rerun the original `uv export` or `uv pip compile` command to regenerate the file.
    error: The packaged lock for `locked-tool==3.0.0` is missing artifact hashes; regenerate the lock before publishing the package
    ");
    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn packaged_lock_artifact_urls() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let lock = dependency(&context, "3.3")?;

    let mut redirected: toml::Value = toml::from_str(&lock)?;
    redirected["packages"][0]["wheels"][0]["url"] =
        toml::Value::String("https://example.invalid/idna-3.3-py3-none-any.whl".to_owned());
    // An index named in the lock is not authoritative for the artifact URLs.
    redirected["packages"][0]
        .as_table_mut()
        .context("Expected a package table")?
        .insert(
            "index".to_owned(),
            toml::Value::String("https://example.invalid/simple".to_owned()),
        );
    tool(
        &context,
        "1.0.0",
        Some(&toml::to_string(&redirected)?),
        None,
    )?;
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "locked-tool==1.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==1.0.0` contains unverified artifacts
      cause: URL for `idna==3.3` is not listed by PyPI: https://example.invalid/idna-3.3-py3-none-any.whl
    ");

    let mut wrong_path: toml::Value = toml::from_str(&lock)?;
    wrong_path["packages"][0]["wheels"][0]["url"] = toml::Value::String(
        "https://files.pythonhosted.org/packages/invalid/idna-3.3-py3-none-any.whl".to_owned(),
    );
    tool(
        &context,
        "2.0.0",
        Some(&toml::to_string(&wrong_path)?),
        None,
    )?;
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "locked-tool==2.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==2.0.0` contains unverified artifacts
      cause: URL for `idna==3.3` is not listed by PyPI: https://files.pythonhosted.org/packages/invalid/idna-3.3-py3-none-any.whl
    ");

    let mut unselected: toml::Value = toml::from_str(&lock)?;
    unselected["packages"][0]["sdist"]["url"] =
        toml::Value::String("https://example.invalid/idna-3.3.tar.gz".to_owned());
    tool(
        &context,
        "3.0.0",
        Some(&toml::to_string(&unselected)?),
        None,
    )?;
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "locked-tool==3.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==3.0.0` contains unverified artifacts
      cause: URL for `idna==3.3` is not listed by PyPI: https://example.invalid/idna-3.3.tar.gz
    ");

    let newer_lock: toml::Value = toml::from_str(&dependency(&context, "3.4")?)?;
    let mut wrong_version: toml::Value = toml::from_str(&lock)?;
    wrong_version["packages"][0]["sdist"]["url"] =
        newer_lock["packages"][0]["sdist"]["url"].clone();
    tool(
        &context,
        "4.0.0",
        Some(&toml::to_string(&wrong_version)?),
        None,
    )?;
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "locked-tool==4.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==4.0.0` contains unverified artifacts
      cause: URL for `idna==3.3` is not listed by PyPI: https://files.pythonhosted.org/packages/8b/e1/43beb3d38dba6cb420cefa297822eac205a277ab43e5ba5d5c46faf96438/idna-3.4.tar.gz
    ");

    tool(&context, "5.0.0", Some(&lock), None)?;
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--no-index", "--find-links", "wheels", "locked-tool==5.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==5.0.0` contains unverified artifacts
      cause: idna isn't available locally, but making network requests to registries was banned
    ");
    Ok(())
}

#[test]
fn packaged_lock_non_registry_sources() -> Result<()> {
    // None of these sources may be accessed while validating a packaged lock.
    for source in [
        r#"archive = { url = "https://example.invalid/idna-3.3.tar.gz", hashes = { sha256 = "0000000000000000000000000000000000000000000000000000000000000000" } }"#,
        r#"directory = { path = "dependency" }"#,
        r#"vcs = { type = "git", url = "https://example.invalid/idna", commit-id = "0123456789012345678901234567890123456789" }"#,
        r#"wheels = [{ url = "https://files.pythonhosted.org/idna-3.3-py3-none-any.whl", path = "idna-3.3-py3-none-any.whl", hashes = { sha256 = "0000000000000000000000000000000000000000000000000000000000000000" } }]"#,
        r#"sdist = { path = "idna-3.3.tar.gz", hashes = { sha256 = "0000000000000000000000000000000000000000000000000000000000000000" } }"#,
    ] {
        let context = uv_test::test_context!("3.12").with_tool_dirs();
        context.temp_dir.child("wheels").create_dir_all()?;
        let lock = formatdoc! {r#"
            lock-version = "1.0"
            created-by = "test"
            [[packages]]
            name = "idna"
            version = "3.3"
            {source}
        "#};
        tool(&context, "1.0.0", Some(&lock), None)?;
        allow_duplicates! {
            uv_snapshot!(context.filters(), context.tool_install()
                .args(["--locked", "--preview-features", "locked-tools", "--no-index", "--find-links", "wheels", "locked-tool==1.0.0"]), @"
            exit_code: 2 (failure)
            ----- stderr -----
            error: The packaged lock for `locked-tool==1.0.0` contains unverified artifacts
              cause: `idna==3.3` must use wheel or source distribution URLs from PyPI
            ");
        }
    }
    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn packaged_lock_from_build_backend() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    context.temp_dir.child("src/locked_tool").create_dir_all()?;
    context
        .temp_dir
        .child("src/locked_tool/__init__.py")
        .write_str("import idna\ndef main(): print(idna.__version__)\n")?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["idna>=3.4"]
        [project.scripts]
        locked-tool = "locked_tool:main"
        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"
    "#})?;
    context
        .lock()
        .args(["--resolution", "lowest-direct"])
        .assert()
        .success();
    context
        .build_backend()
        .args(["--preview-features", "locked-tools", "build-wheel", "."])
        .assert()
        .success();
    let bin_dir = context.temp_dir.child("bin");
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--from", "./locked_tool-1.0.0-py3-none-any.whl", "locked-tool"])
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + idna==3.4
     + locked-tool==1.0.0 (from file://[TEMP_DIR]/locked_tool-1.0.0-py3-none-any.whl)
    Installed 1 executable: locked-tool
    ");
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "--offline", "--from", "./locked_tool-1.0.0-py3-none-any.whl", "locked-tool"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    3.4

    ----- stderr -----
    Installed [N] packages in [TIME]
     + idna==3.4
     + locked-tool==1.0.0 (from file://[TEMP_DIR]/locked_tool-1.0.0-py3-none-any.whl)
    ");
    Ok(())
}

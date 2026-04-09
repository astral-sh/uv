use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::PathChild;
use fs_err as fs;
use insta::assert_snapshot;
use uv_static::EnvVars;
use uv_test::uv_snapshot;

#[test]
fn tool_list() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_list()
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0
    - black
    - blackd

    ----- stderr -----
    ");
}

#[test]
fn tool_list_paths() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_list().arg("--show-paths")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)

    ----- stderr -----
    ");
}

#[cfg(windows)]
#[test]
fn tool_list_paths_windows() {
    let context = uv_test::test_context!("3.12")
        .clear_filters()
        .with_filtered_windows_temp_dir();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    uv_snapshot!(context.filters_without_standard_filters(), context.tool_list().arg("--show-paths")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 ([TEMP_DIR]\tools\black)
    - black ([TEMP_DIR]\bin\black.exe)
    - blackd ([TEMP_DIR]\bin\blackd.exe)

    ----- stderr -----
    "###);
}

#[test]
fn tool_list_empty() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_list()
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    No tools installed
    ");
}

#[test]
fn tool_list_outdated_empty() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // With no tools installed, `--outdated` should produce the same output as the base case.
    uv_snapshot!(context.filters(), context.tool_list()
    .arg("--outdated")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    No tools installed
    ");
}

#[test]
fn tool_list_outdated() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install an older version of `black`.
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // With `--outdated`, the installed (older) version should be listed with the latest version.
    uv_snapshot!(context.filters(), context.tool_list()
    .arg("--outdated")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [latest: 24.3.0]
    - black
    - blackd

    ----- stderr -----
    ");
}

#[test]
fn tool_list_outdated_respects_exclude_newer() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` with a persisted `exclude-newer` cutoff.
    context
        .tool_install()
        .arg("black")
        .arg("--exclude-newer")
        .arg("2024-03-25T00:00:00Z")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // `--outdated` should respect the stored tool settings and avoid flagging upgrades that
    // `uv tool upgrade` would intentionally skip.
    uv_snapshot!(context.filters(), context.tool_list()
    .arg("--outdated")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");
}

#[test]
fn tool_list_outdated_recomputes_relative_exclude_newer() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` with a relative `exclude-newer` cutoff that initially resolves to 2024-03-01.
    context
        .tool_install()
        .arg("black")
        .arg("--exclude-newer")
        .arg("3 weeks")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2024-03-22T00:00:00Z")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Recompute the stored span at a later time so `black` is considered outdated.
    uv_snapshot!(context.filters(), context.tool_list()
    .arg("--outdated")
    .env_remove(EnvVars::UV_EXCLUDE_NEWER)
    .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2024-04-15T00:00:00Z")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [latest: 24.3.0]
    - black
    - blackd

    ----- stderr -----
    ");
}

#[test]
fn tool_list_outdated_cli_exclude_newer() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install an older version of `black`.
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // `--exclude-newer` should filter out releases newer than the cutoff when determining the
    // latest available tool version.
    uv_snapshot!(context.filters(), context.tool_list()
    .arg("--outdated")
    .arg("--exclude-newer")
    .arg("2024-03-01T00:00:00Z")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");
}

#[test]
fn tool_list_missing_receipt() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    fs_err::remove_file(tool_dir.join("black").join("uv-receipt.toml")).unwrap();

    uv_snapshot!(context.filters(), context.tool_list()
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Ignoring malformed tool `black` (run `uv tool uninstall black` to remove)
    ");
}

#[test]
fn tool_list_bad_environment() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_python_names()
        .with_filtered_virtualenv_bin()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Install `ruff`
    context
        .tool_install()
        .arg("ruff==0.3.4")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    let venv_path = uv_test::venv_bin_path(tool_dir.path().join("black"));
    // Remove the python interpreter for black
    fs::remove_dir_all(venv_path.clone())?;

    uv_snapshot!(
        context.filters(),
        context
            .tool_list()
            .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
            .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    ruff v0.3.4
    - ruff

    ----- stderr -----
    warning: Invalid environment at `tools/black`: missing Python executable at `tools/black/[BIN]/[PYTHON]` (run `uv tool install black --reinstall` to reinstall)
    "
    );

    Ok(())
}

#[test]
fn tool_list_deprecated() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    context
        .tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Ensure that we have a modern tool receipt.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black", specifier = "==24.2.0" }]

        [[package]]
        name = "black"
        version = "24.2.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "mypy-extensions" },
            { name = "packaging" },
            { name = "pathspec" },
            { name = "platformdirs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/29/69/f3ab49cdb938b3eecb048fa64f86bdadb1fac26e92c435d287181d543b0a/black-24.2.0.tar.gz", hash = "sha256:bce4f25c27c3435e4dace4815bcb2008b87e167e3bf4ee47ccdc5ce906eb4894", size = 631598, upload-time = "2024-02-12T20:21:26.969Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/43/1e/67c87a1fb39592aa944f35cc26892946ebe0a10aa324b87f9380b8753862/black-24.2.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:d84f29eb3ee44859052073b7636533ec995bd0f64e2fb43aeceefc70090e752b", size = 1585288, upload-time = "2024-02-12T20:37:13.8Z" },
            { url = "https://files.pythonhosted.org/packages/5e/62/6437212cf40e40b74dbc7e134700a21cb21a9ac7e46ade940b5d4826456f/black-24.2.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:1e08fb9a15c914b81dd734ddd7fb10513016e5ce7e6704bdd5e1251ceee51ac9", size = 1417360, upload-time = "2024-02-12T20:34:56.41Z" },
            { url = "https://files.pythonhosted.org/packages/36/8f/de0d339ae683422a8e15d6f74b8022d4947009c347d8c2178c303c68cc4d/black-24.2.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:810d445ae6069ce64030c78ff6127cd9cd178a9ac3361435708b907d8a04c693", size = 1739406, upload-time = "2024-02-12T20:23:59.596Z" },
            { url = "https://files.pythonhosted.org/packages/3e/58/89e5f5a1c4c5b66dc74eabe6337623d53b4d1c27fbbbe16defee53397f60/black-24.2.0-cp312-cp312-win_amd64.whl", hash = "sha256:ba15742a13de85e9b8f3239c8f807723991fbfae24bad92d34a2b12e81904982", size = 1373310, upload-time = "2024-02-12T20:25:27.243Z" },
            { url = "https://files.pythonhosted.org/packages/47/15/b3770bc3328685a53bc9c041136240146c5cd866a1f020c2cf47f2ff9683/black-24.2.0-py3-none-any.whl", hash = "sha256:e8a6ae970537e67830776488bca52000eaa37fa63b9988e8c487458d9cd5ace6", size = 200610, upload-time = "2024-02-12T20:21:17.657Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black", specifier = "==24.2.0" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Replace with a legacy receipt.
    fs::write(
        tool_dir.join("black").join("uv-receipt.toml"),
        r#"
        [tool]
        requirements = ["black==24.2.0"]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]
        "#,
    )?;

    // Ensure that we can still list the tool.
    uv_snapshot!(context.filters(), context.tool_list()
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0
    - black
    - blackd

    ----- stderr -----
    ");

    // Replace with an invalid receipt.
    fs::write(
        tool_dir.join("black").join("uv-receipt.toml"),
        r#"
        [tool]
        requirements = ["black<>24.2.0"]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]
        "#,
    )?;

    // Ensure that listing fails.
    uv_snapshot!(context.filters(), context.tool_list()
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Ignoring malformed tool `black` (run `uv tool uninstall black` to remove)
    ");

    Ok(())
}

#[test]
fn tool_list_show_version_specifiers() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` with a version specifier
    context
        .tool_install()
        .arg("black<24.3.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Install `flask`
    context
        .tool_install()
        .arg("flask")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_list().arg("--show-version-specifiers")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [required: <24.3.0]
    - black
    - blackd
    flask v3.0.2
    - flask

    ----- stderr -----
    ");

    // with paths
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-version-specifiers").arg("--show-paths")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [required: <24.3.0] ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)
    flask v3.0.2 ([TEMP_DIR]/tools/flask)
    - flask ([TEMP_DIR]/bin/flask)

    ----- stderr -----
    ");
}

#[test]
fn tool_list_show_with() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` without additional requirements
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Install `flask` with additional requirements
    context
        .tool_install()
        .arg("flask")
        .arg("--with")
        .arg("requests")
        .arg("--with")
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Install `ruff` with version specifier and additional requirements
    context
        .tool_install()
        .arg("ruff==0.3.4")
        .arg("--with")
        .arg("requests")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Test with --show-with
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-with")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0
    - black
    - blackd
    flask v3.0.2 [with: requests, black==24.2.0]
    - flask
    ruff v0.3.4 [with: requests]
    - ruff

    ----- stderr -----
    ");

    // Test with both --show-with and --show-paths
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-with").arg("--show-paths")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)
    flask v3.0.2 [with: requests, black==24.2.0] ([TEMP_DIR]/tools/flask)
    - flask ([TEMP_DIR]/bin/flask)
    ruff v0.3.4 [with: requests] ([TEMP_DIR]/tools/ruff)
    - ruff ([TEMP_DIR]/bin/ruff)

    ----- stderr -----
    ");

    // Test with both --show-with and --show-version-specifiers
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-with").arg("--show-version-specifiers")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [required: ==24.2.0]
    - black
    - blackd
    flask v3.0.2 [with: requests, black==24.2.0]
    - flask
    ruff v0.3.4 [required: ==0.3.4] [with: requests]
    - ruff

    ----- stderr -----
    ");

    // Test with all flags
    uv_snapshot!(context.filters(), context.tool_list()
    .arg("--show-with")
    .arg("--show-version-specifiers")
    .arg("--show-paths")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [required: ==24.2.0] ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)
    flask v3.0.2 [with: requests, black==24.2.0] ([TEMP_DIR]/tools/flask)
    - flask ([TEMP_DIR]/bin/flask)
    ruff v0.3.4 [required: ==0.3.4] [with: requests] ([TEMP_DIR]/tools/ruff)
    - ruff ([TEMP_DIR]/bin/ruff)

    ----- stderr -----
    ");
}

#[test]
fn tool_list_show_extras() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` without extras
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Install `flask` with extras and additional requirements
    context
        .tool_install()
        .arg("flask[async,dotenv]")
        .arg("--with")
        .arg("requests")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Test with --show-extras only
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-extras")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0
    - black
    - blackd
    flask v3.0.2 [extras: async, dotenv]
    - flask

    ----- stderr -----
    ");

    // Test with both --show-extras and --show-with
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-extras").arg("--show-with")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0
    - black
    - blackd
    flask v3.0.2 [extras: async, dotenv] [with: requests]
    - flask

    ----- stderr -----
    ");

    // Test with --show-extras and --show-paths
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-extras").arg("--show-paths")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)
    flask v3.0.2 [extras: async, dotenv] ([TEMP_DIR]/tools/flask)
    - flask ([TEMP_DIR]/bin/flask)

    ----- stderr -----
    ");

    // Test with --show-extras and --show-version-specifiers
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-extras").arg("--show-version-specifiers")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [required: ==24.2.0]
    - black
    - blackd
    flask v3.0.2 [extras: async, dotenv]
    - flask

    ----- stderr -----
    ");

    // Test with all flags including --show-extras
    uv_snapshot!(context.filters(), context.tool_list()
    .arg("--show-extras")
    .arg("--show-with")
    .arg("--show-version-specifiers")
    .arg("--show-paths")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [required: ==24.2.0] ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)
    flask v3.0.2 [extras: async, dotenv] [with: requests] ([TEMP_DIR]/tools/flask)
    - flask ([TEMP_DIR]/bin/flask)

    ----- stderr -----
    ");
}

#[test]
fn tool_list_show_python() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` with python 3.12
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Test with --show-python
    uv_snapshot!(context.filters(), context.tool_list().arg("--show-python")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [CPython 3.12.[X]]
    - black
    - blackd

    ----- stderr -----
    ");
}

#[test]
fn tool_list_show_all() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` without extras
    context
        .tool_install()
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Install `flask` with extras and additional requirements
    context
        .tool_install()
        .arg("flask[async,dotenv]")
        .arg("--with")
        .arg("requests")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .assert()
        .success();

    // Test with all flags
    uv_snapshot!(context.filters(), context.tool_list()
    .arg("--show-extras")
    .arg("--show-with")
    .arg("--show-version-specifiers")
    .arg("--show-paths")
    .arg("--show-python")
    .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
    .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black v24.2.0 [required: ==24.2.0] [CPython 3.12.[X]] ([TEMP_DIR]/tools/black)
    - black ([TEMP_DIR]/bin/black)
    - blackd ([TEMP_DIR]/bin/blackd)
    flask v3.0.2 [extras: async, dotenv] [with: requests] [CPython 3.12.[X]] ([TEMP_DIR]/tools/flask)
    - flask ([TEMP_DIR]/bin/flask)

    ----- stderr -----
    ");
}

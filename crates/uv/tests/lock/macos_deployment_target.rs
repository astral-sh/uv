use std::fmt::Write;

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;

use uv_static::EnvVars;
use uv_test::packse::{PackseServer, scenario::Scenario};
use uv_test::uv_snapshot;

/// A macOS baseline retains newer wheels and their hashes, including from unhashed indexes.
#[test]
fn minimum_macos_version() -> Result<()> {
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "macos-deployment-target-supported-environments"

        [root]

        [expected]
        satisfiable = true

        [packages.a.versions."1.0.0"]
        sdist = false
        wheel_tags = ["py3-none-macosx_14_0_universal2", "py3-none-macosx_16_0_arm64", "py3-none-manylinux_2_17_x86_64", "py3-none-win_amd64"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let context = uv_test::test_context!("3.12").with_filters(
        server
            .files()
            .map(|(filename, hash)| (hash.to_owned(), format!("[SHA256:{filename}]"))),
    );
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a"]

        [tool.uv]
        required-environments = ["sys_platform == 'darwin' and platform_machine == 'arm64'"]

        [[tool.uv.index]]
        url = "{}"
    "#, server.index_url()})?;

    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.pip_compile()
        .args(["pyproject.toml", "--universal", "--generate-hashes", "--no-header", "--no-annotate", "-o", "requirements.txt"]), @r"
    exit_code: 0 (success)
    ----- stdout -----
    a==1.0.0 \
        --hash=sha256:[SHA256:a-1.0.0-py3-none-manylinux_2_17_x86_64.whl] \
        --hash=sha256:[SHA256:a-1.0.0-py3-none-macosx_14_0_universal2.whl] \
        --hash=sha256:[SHA256:a-1.0.0-py3-none-macosx_16_0_arm64.whl] \
        --hash=sha256:[SHA256:a-1.0.0-py3-none-win_amd64.whl]

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    // Adding a baseline invalidates the lock without removing newer wheels or their hashes.
    pyproject_toml.write_str(&context.read("pyproject.toml").replace(
        "[tool.uv]",
        indoc! {r#"
            [tool.uv]
            minimum-macos-version = "15.0"
        "#},
    ))?;
    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        required-markers = [
            "platform_machine == 'arm64' and sys_platform == 'darwin'",
        ]

        [options]
        minimum-macos-version = "15.0"
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-macosx_14_0_universal2.whl", hash = "sha256:[SHA256:a-1.0.0-py3-none-macosx_14_0_universal2.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-macosx_16_0_arm64.whl", hash = "sha256:[SHA256:a-1.0.0-py3-none-macosx_16_0_arm64.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-manylinux_2_17_x86_64.whl", hash = "sha256:[SHA256:a-1.0.0-py3-none-manylinux_2_17_x86_64.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-win_amd64.whl", hash = "sha256:[SHA256:a-1.0.0-py3-none-win_amd64.whl]", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a" },
        ]

        [package.metadata]
        requires-dist = [{ name = "a" }]
        "#);
    });
    uv_snapshot!(context.filters(), context.lock().args(["--locked", "--offline"])
        .env(EnvVars::MACOSX_DEPLOYMENT_TARGET, "10.9"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--python-platform", "aarch64-apple-darwin"])
        .env(EnvVars::MACOSX_DEPLOYMENT_TARGET, "15.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + a==1.0.0
    ");
    assert_snapshot!(fs_err::read_to_string(context.site_packages().join("a-1.0.0.dist-info/WHEEL"))?, @"
    Wheel-Version: 1.0
    Generator: uv-test
    Root-Is-Purelib: true
    Tag: py3-none-macosx_14_0_universal2
    ");

    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--reinstall", "--python-platform", "aarch64-apple-darwin"])
        .env(EnvVars::MACOSX_DEPLOYMENT_TARGET, "16.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ a==1.0.0
    ");
    assert_snapshot!(fs_err::read_to_string(context.site_packages().join("a-1.0.0.dist-info/WHEEL"))?, @"
    Wheel-Version: 1.0
    Generator: uv-test
    Root-Is-Purelib: true
    Tag: py3-none-macosx_16_0_arm64
    ");
    uv_snapshot!(context.filters(), context.pip_compile()
        .args(["pyproject.toml", "--universal", "--generate-hashes", "--no-header", "--no-annotate", "-o", "requirements.txt"]), @r"
    exit_code: 0 (success)
    ----- stdout -----
    a==1.0.0 \
        --hash=sha256:[SHA256:a-1.0.0-py3-none-manylinux_2_17_x86_64.whl] \
        --hash=sha256:[SHA256:a-1.0.0-py3-none-macosx_14_0_universal2.whl] \
        --hash=sha256:[SHA256:a-1.0.0-py3-none-macosx_16_0_arm64.whl] \
        --hash=sha256:[SHA256:a-1.0.0-py3-none-win_amd64.whl]

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    let mut index = String::new();
    for (filename, _) in server.files() {
        writeln!(
            index,
            "<a href=\"{}\">{filename}</a>",
            server.file_url(filename)
        )?;
    }
    context.temp_dir.child("index.html").write_str(&index)?;
    uv_snapshot!(context.filters(), context.pip_compile()
        .args(["pyproject.toml", "--universal", "--generate-hashes", "--no-header", "--no-annotate", "-o", "requirements.txt", "--no-index", "--find-links", "index.html"]), @r"
    exit_code: 0 (success)
    ----- stdout -----
    a==1.0.0 \
        --hash=sha256:[SHA256:a-1.0.0-py3-none-manylinux_2_17_x86_64.whl] \
        --hash=sha256:[SHA256:a-1.0.0-py3-none-macosx_14_0_universal2.whl] \
        --hash=sha256:[SHA256:a-1.0.0-py3-none-macosx_16_0_arm64.whl] \
        --hash=sha256:[SHA256:a-1.0.0-py3-none-win_amd64.whl]

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    Ok(())
}

/// The environment fallback changes version selection and is recorded in the lock.
#[test]
fn macos_deployment_target() -> Result<()> {
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "macos-deployment-target"

        [root]

        [expected]
        satisfiable = true

        [packages.a.versions."1.0.0"]
        sdist = false
        wheel_tags = ["py3-none-macosx_10_9_universal2", "py3-none-macosx_16_0_arm64"]

        [packages.a.versions."2.0.0"]
        sdist = false
        wheel_tags = ["py3-none-macosx_16_0_universal2"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let context = uv_test::test_context!("3.12").with_filters(
        server
            .files()
            .map(|(filename, hash)| (hash.to_owned(), format!("[SHA256:{filename}]"))),
    );
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a"]

        [tool.uv]
        environments = ["sys_platform == 'darwin'"]
        required-environments = [
            "sys_platform == 'darwin' and platform_machine == 'arm64'",
            "sys_platform == 'darwin' and platform_machine == 'x86_64'",
        ]

        [[tool.uv.index]]
        url = "{}"
    "#, server.index_url()})?;

    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.lock()
        .env(EnvVars::MACOSX_DEPLOYMENT_TARGET, "15.0")
        .arg("--locked"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    uv_snapshot!(context.filters(), context.lock()
        .env(EnvVars::MACOSX_DEPLOYMENT_TARGET, "15.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Updated a v2.0.0 -> v1.0.0
    ");

    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        supported-markers = [
            "sys_platform == 'darwin'",
        ]
        required-markers = [
            "platform_machine == 'arm64' and sys_platform == 'darwin'",
            "platform_machine == 'x86_64' and sys_platform == 'darwin'",
        ]

        [options]
        minimum-macos-version = "15.0"
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-macosx_10_9_universal2.whl", hash = "sha256:[SHA256:a-1.0.0-py3-none-macosx_10_9_universal2.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-macosx_16_0_arm64.whl", hash = "sha256:[SHA256:a-1.0.0-py3-none-macosx_16_0_arm64.whl]", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "a" },
        ]

        [package.metadata]
        requires-dist = [{ name = "a" }]
        "#);
    });

    // Equivalent spellings reuse the lock, including without access to the index.
    uv_snapshot!(context.filters(), context.lock()
        .env(EnvVars::MACOSX_DEPLOYMENT_TARGET, "15")
        .arg("--locked").arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");

    uv_snapshot!(context.filters(), context.pip_compile()
        .arg("pyproject.toml").arg("--universal").arg("--no-header")
        .env(EnvVars::MACOSX_DEPLOYMENT_TARGET, "15.0"), @"
    exit_code: 0 (success)
    ----- stdout -----
    a==1.0.0 ; sys_platform == 'darwin'
        # via project (pyproject.toml)

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    Ok(())
}

/// Direct URLs and flat indexes must check the deployment target, including compressed tags.
#[test]
fn macos_deployment_target_wheel_url() -> Result<()> {
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "macos-deployment-target-wheel-url"

        [root]

        [expected]
        satisfiable = false

        [packages.a.versions."1.0.0"]
        sdist = false
        wheel_tags = ["py3-none-macosx_16_0_arm64.manylinux_2_17_x86_64"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let context = uv_test::test_context!("3.12");
    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain([(
            r"\nhint: The resolution failed for an environment that is not the current one[^\n]*",
            "",
        )])
        .collect();
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a @ {}"]

        [tool.uv]
        environments = ["sys_platform == 'darwin' and platform_machine == 'arm64'"]
    "#, server.file_url("a-1.0.0-py3-none-macosx_16_0_arm64.manylinux_2_17_x86_64.whl")})?;

    uv_snapshot!(filters, context.lock()
        .env(EnvVars::MACOSX_DEPLOYMENT_TARGET, "15.0"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies for split (markers: platform_machine == 'arm64' and sys_platform == 'darwin')
      cause: Because only a==1.0.0 is available and a==1.0.0 has no `platform_machine == 'arm64' and sys_platform == 'darwin'`-compatible wheels, we can conclude that all versions of a cannot be used.
             And because your project depends on a, we can conclude that your project's requirements are unsatisfiable.
    ");

    context
        .temp_dir
        .child("index.html")
        .write_str(&formatdoc! {r#"
        <a href="{}">a</a>
    "#, server.file_url("a-1.0.0-py3-none-macosx_16_0_arm64.manylinux_2_17_x86_64.whl")})?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a"]

        [tool.uv]
        environments = ["sys_platform == 'darwin' and platform_machine == 'arm64'"]
        find-links = ["index.html"]
        no-index = true
    "#})?;

    uv_snapshot!(filters, context.lock()
        .env(EnvVars::MACOSX_DEPLOYMENT_TARGET, "15.0"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies for split (markers: platform_machine == 'arm64' and sys_platform == 'darwin')
      cause: Because a==1.0.0 has no `platform_machine == 'arm64' and sys_platform == 'darwin'`-compatible wheels and only a==1.0.0 is available, we can conclude that all versions of a cannot be used.
             And because your project depends on a, we can conclude that your project's requirements are unsatisfiable.
    ");

    Ok(())
}

/// Source distributions and pure wheels remain usable, and libc coverage is checked independently.
#[test]
fn macos_deployment_target_fallbacks() -> Result<()> {
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "macos-deployment-target-fallbacks"

        [root]

        [expected]
        satisfiable = true

        [packages.a.versions."1.0.0"]
        sdist = true
        wheel_tags = ["py3-none-macosx_16_0_arm64"]

        [packages.b.versions."1.0.0"]
        sdist = false
        wheel_tags = ["py3-none-macosx_16_0_arm64", "py3-none-any"]

        [packages.c.versions."1.0.0"]
        sdist = false
        wheel_tags = ["py3-none-manylinux_2_17_x86_64", "py3-none-win_amd64"]

        [packages.c.versions."2.0.0"]
        sdist = false
        wheel_tags = ["py3-none-manylinux_2_34_x86_64", "py3-none-win_amd64"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a", "b", "c; sys_platform != 'darwin'"]

        [tool.uv]
        minimum-libc-version = { glibc = "2.31" }
        required-environments = [
            "sys_platform == 'darwin' and platform_machine == 'arm64'",
            "sys_platform == 'linux' and platform_machine == 'x86_64'",
            "sys_platform == 'win32' and platform_machine == 'AMD64'",
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.lock()
        .arg("--index-url").arg(server.index_url())
        .args(["--preview-features", "minimum-libc-version"])
        .env(EnvVars::MACOSX_DEPLOYMENT_TARGET, "15.0"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 5 packages in [TIME]
    ");

    Ok(())
}

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;

use uv_test::packse::PackseServer;
use uv_test::uv_snapshot;

/// The macOS floor affects the selected pin, while newer wheels for that pin remain available.
#[test]
fn minimum_macos_version_invalidates_lock() -> Result<()> {
    let server = PackseServer::from_toml(indoc! {r#"
        name = "required-environment-macos-release"

        [root]

        [expected]
        satisfiable = true

        [packages.a.versions."1.0.0"]
        sdist = false
        wheel_tags = ["py3-none-macosx_14_0_arm64", "py3-none-macosx_26_0_arm64"]

        [packages.a.versions."2.0.0"]
        sdist = false
        wheel_tags = ["py3-none-macosx_26_0_arm64"]
    "#})?;
    let context = uv_test::test_context!("3.12").with_filters(
        server
            .files()
            .map(|(filename, hash)| (hash.to_owned(), format!("[SHA256:{filename}]"))),
    );
    let pyproject = indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a"]

        [tool.uv]
        environments = ["sys_platform == 'darwin' and platform_machine == 'arm64'"]
        required-environments = ["sys_platform == 'darwin' and platform_machine == 'arm64'"]
        preview-features = ["minimum-macos-version"]
        minimum-macos-version = "26.0"
    "#};
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(pyproject)?;

    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let original = context.read("uv.lock");
    assert_eq!(uv_lock::Lock::from_toml(&original)?.to_toml()?, original);
    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--no-header", "--no-hashes", "--no-annotate"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    a==2.0.0 ; platform_machine == 'arm64' and sys_platform == 'darwin'
    ");

    pyproject_toml.write_str(&pyproject.replace(
        r#"minimum-macos-version = "26.0""#,
        r#"minimum-macos-version = "14.0""#,
    ))?;
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--index-url").arg(server.index_url()), @"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    assert_eq!(context.read("uv.lock"), original);

    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Updated a v2.0.0 -> v1.0.0
    ");
    let lock = context.read("uv.lock");
    assert_eq!(uv_lock::Lock::from_toml(&lock)?.to_toml()?, lock);
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 5
        requires-python = ">=3.12"
        resolution-markers = [
            "platform_machine == 'arm64' and sys_platform == 'darwin'",
        ]
        supported-markers = [
            "platform_machine == 'arm64' and sys_platform == 'darwin'",
        ]
        required-markers = [
            "platform_machine == 'arm64' and sys_platform == 'darwin'",
        ]

        [options]
        minimum-macos-version = "14.0"
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "a"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-macosx_14_0_arm64.whl", hash = "sha256:[SHA256:a-1.0.0-py3-none-macosx_14_0_arm64.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/a-1.0.0-py3-none-macosx_26_0_arm64.whl", hash = "sha256:[SHA256:a-1.0.0-py3-none-macosx_26_0_arm64.whl]", upload-time = "2024-03-24T00:00:00Z" },
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

    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--index-url").arg(server.index_url()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--no-header", "--no-hashes", "--no-annotate"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    a==1.0.0 ; platform_machine == 'arm64' and sys_platform == 'darwin'
    ");
    context
        .export()
        .args(["--frozen", "--format", "pylock.toml"])
        .assert()
        .success();

    Ok(())
}

/// A direct wheel URL must cover the baseline even when no other candidate is available.
#[test]
fn minimum_macos_version_direct_url() -> Result<()> {
    let server = PackseServer::from_toml(indoc! {r#"
        name = "minimum-macos-direct-url"
        [root]
        [expected]
        satisfiable = true
        [packages.a.versions."2.0.0"]
        sdist = false
        wheel_tags = ["py3-none-macosx_26_0_arm64"]
    "#})?;
    let wheel = server.file_url("a-2.0.0-py3-none-macosx_26_0_arm64.whl");
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a @ {wheel}"]

        [tool.uv]
        environments = ["sys_platform == 'darwin' and platform_machine == 'arm64'"]
        required-environments = ["sys_platform == 'darwin' and platform_machine == 'arm64'"]
        preview-features = ["minimum-macos-version"]
        minimum-macos-version = "14.0"
    "#})?;
    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain([(
            r"\nhint: The resolution failed for an environment that is not the current one[^\n]*",
            "",
        )])
        .collect();
    uv_snapshot!(filters, context.lock(), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies for split (markers: platform_machine == 'arm64' and sys_platform == 'darwin')
      cause: Because only a==2.0.0 is available and a==2.0.0 has no `platform_machine == 'arm64' and sys_platform == 'darwin'`-compatible wheels, we can conclude that all versions of a cannot be used.
             And because your project depends on a, we can conclude that your project's requirements are unsatisfiable.
    ");
    Ok(())
}

/// Wheel coverage for the macOS floor is checked independently in each Python fork.
#[test]
fn minimum_macos_version_python_fork() -> Result<()> {
    let server = PackseServer::from_toml(indoc! {r#"
        name = "required-environment-macos-release-python-fork"

        [root]

        [expected]
        satisfiable = true

        [packages.a.versions."1.0.0"]
        sdist = false
        wheel_tags = ["cp312-cp312-macosx_14_0_arm64", "cp313-cp313-macosx_14_0_arm64"]

        [packages.a.versions."2.0.0"]
        sdist = false
        wheel_tags = ["cp312-cp312-macosx_14_0_arm64", "cp313-cp313-macosx_26_0_arm64"]
    "#})?;
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12,<3.14"
        dependencies = ["a==2.0.0; python_version < '3.13'", "a; python_version >= '3.13'"]

        [tool.uv]
        environments = [
            "sys_platform == 'darwin' and platform_machine == 'arm64' and python_version < '3.13'",
            "sys_platform == 'darwin' and platform_machine == 'arm64' and python_version >= '3.13'",
        ]
        required-environments = ["sys_platform == 'darwin' and platform_machine == 'arm64'"]
        preview-features = ["minimum-macos-version"]
        minimum-macos-version = "14.0"
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.export().args(["--locked", "--no-header", "--no-hashes", "--no-annotate"]).arg("--index-url").arg(server.index_url()), @"
    exit_code: 0 (success)
    ----- stdout -----
    a==1.0.0 ; python_full_version >= '3.13' and platform_machine == 'arm64' and sys_platform == 'darwin'
    a==2.0.0 ; python_full_version < '3.13' and platform_machine == 'arm64' and sys_platform == 'darwin'

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    Ok(())
}

/// The installer accepts only minor-zero tags from macOS 11 onwards. A 14.1 wheel
/// must not satisfy a 15.0 floor even though its deployment target is lower.
#[test]
fn minimum_macos_version_rejects_unsupported_minor_tag() -> Result<()> {
    let server = PackseServer::from_toml(indoc! {r#"
        name = "minimum-macos-minor-tag"

        [root]

        [expected]
        satisfiable = true

        [packages.a.versions."1"]
        sdist = false
        wheel_tags = ["py3-none-macosx_14_0_arm64"]

        [packages.a.versions."2"]
        sdist = false
        wheel_tags = ["py3-none-macosx_14_1_arm64"]
    "#})?;
    let context = uv_test::test_context!("3.12");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0"
        requires-python = ">=3.12"
        dependencies = ["a"]

        [tool.uv]
        preview-features = ["minimum-macos-version"]
        required-environments = ["sys_platform == 'darwin' and platform_machine == 'arm64'"]
        minimum-macos-version = "15.0"
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--no-header", "--no-hashes", "--no-annotate"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    a==1
    ");

    Ok(())
}

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};

use uv_test::archive::write_tar_gz;
use uv_test::find_links::FindLinksServer;
use uv_test::packse::generate_wheel;
use uv_test::{TestContext, uv_snapshot};

const LINUX_X86_64: &str = "sys_platform == 'linux' and platform_machine == 'x86_64'";
const LINUX_AARCH64: &str = "sys_platform == 'linux' and platform_machine == 'aarch64'";

fn wheel(context: &TestContext, name: &str, version: &str, tag: &str) -> Result<ChildPath> {
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let (filename, bytes) = generate_wheel(
        &name.parse()?,
        &version.parse()?,
        &[],
        &BTreeMap::new(),
        None,
        tag,
    );
    let wheel = links.child(filename);
    wheel.write_binary(&bytes)?;
    Ok(wheel)
}

fn project(
    context: &TestContext,
    dependencies: &[&str],
    required_environments: &[&str],
    minimum_glibc: Option<&str>,
) -> Result<()> {
    let minimum_glibc = minimum_glibc
        .map(|version| format!("minimum-glibc-version = \"{version}\""))
        .unwrap_or_default();
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = {dependencies}

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = {required_environments}
        {minimum_glibc}
    "#,
            dependencies = serde_json::to_string(dependencies)?,
            required_environments = serde_json::to_string(required_environments)?,
        })?;
    Ok(())
}

fn locked_package(context: &TestContext, name: &str) -> Result<toml::Value> {
    let lock: toml::Value = toml::from_str(&context.read("uv.lock"))?;
    lock.get("package")
        .and_then(toml::Value::as_array)
        .context("lockfile has no packages")?
        .iter()
        .find(|package| package.get("name").and_then(toml::Value::as_str) == Some(name))
        .cloned()
        .with_context(|| format!("lockfile has no package named {name}"))
}

/// Tightening the deployment floor changes the selected version and is recorded in the lock.
#[test]
fn minimum_glibc_backtracks_and_invalidates_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    wheel(
        &context,
        "demo",
        "1.0.0",
        "cp312-cp312-manylinux_2_17_x86_64",
    )?;
    wheel(
        &context,
        "demo",
        "2.0.0",
        "cp312-cp312-manylinux_2_34_x86_64",
    )?;

    project(&context, &["demo"], &[LINUX_X86_64], None)?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r"
        exit_code: 0 (success)
        ----- stderr -----
        Resolved 2 packages in [TIME]
    ");
    assert_eq!(
        locked_package(&context, "demo")?["version"].as_str(),
        Some("2.0.0")
    );
    let original = context.read("uv.lock");

    project(&context, &["demo"], &[LINUX_X86_64], Some("2.31"))?;
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @r"
        exit_code: 1 (failure)
        ----- stderr -----
        Resolved 3 packages in [TIME]
        error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

        hint: To update the lockfile, run `uv lock`.
    ");
    assert_eq!(context.read("uv.lock"), original);
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r"
        exit_code: 0 (success)
        ----- stderr -----
        Resolved 3 packages in [TIME]
        Updated demo v2.0.0 -> v1.0.0, v2.0.0
    ");
    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--no-hashes", "--no-header", "--no-annotate"]), @r"
        exit_code: 0 (success)
        ----- stdout -----
        demo==1.0.0 ; platform_machine == 'x86_64' and sys_platform == 'linux'
        demo==2.0.0 ; platform_machine != 'x86_64' or sys_platform != 'linux'
    ");
    let lock: toml::Value = toml::from_str(&context.read("uv.lock"))?;
    assert_eq!(
        lock["options"]["minimum-glibc-version"].as_str(),
        Some("2.31")
    );
    context
        .lock()
        .args(["--offline", "--locked"])
        .assert()
        .success();

    // Changing the floor invalidates the lock even when the selected wheel remains compatible.
    let original = context.read("uv.lock");
    project(&context, &["demo"], &[LINUX_X86_64], Some("2.17"))?;
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @r"
        exit_code: 1 (failure)
        ----- stderr -----
        Resolved 3 packages in [TIME]
        error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

        hint: To update the lockfile, run `uv lock`.
    ");
    assert_eq!(context.read("uv.lock"), original);

    project(&context, &["demo"], &[LINUX_X86_64], None)?;
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @r"
        exit_code: 1 (failure)
        ----- stderr -----
        Resolved 2 packages in [TIME]
        error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

        hint: To update the lockfile, run `uv lock`.
    ");
    assert_eq!(context.read("uv.lock"), original);
    Ok(())
}

#[test]
fn minimum_glibc_no_compatible_version() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let mut filters = context.filters();
    filters.push((
        r"\nhint: The resolution failed for an environment that is not the current one[^\n]*",
        "",
    ));
    wheel(
        &context,
        "demo",
        "2.0.0",
        "cp312-cp312-manylinux_2_34_x86_64",
    )?;
    project(&context, &["demo"], &[LINUX_X86_64], Some("2.31"))?;

    let output = uv_snapshot!(filters, context.lock().arg("--offline"), @r"
        exit_code: 1 (failure)
        ----- stderr -----
        error: No solution found when resolving dependencies for split (markers: platform_machine == 'x86_64' and sys_platform == 'linux')
          cause: Because demo==2.0.0 has no `platform_machine == 'x86_64' and sys_platform == 'linux'`-compatible wheels and only demo==2.0.0 is available, we can conclude that all versions of demo cannot be used.
                 And because your project depends on demo, we can conclude that your project's requirements are unsatisfiable.
    ");
    assert!(!output.status.success());
    assert!(!context.temp_dir.child("uv.lock").exists());
    Ok(())
}

/// A source distribution remains usable when its wheel requires a newer glibc.
#[test]
fn minimum_glibc_allows_sdist_fallback() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let mut filters = context.filters();
    filters.push((
        r"\nhint: The resolution failed for an environment that is not the current one[^\n]*",
        "",
    ));
    wheel(
        &context,
        "demo",
        "2.0.0",
        "cp312-cp312-manylinux_2_34_x86_64",
    )?;
    write_tar_gz(
        fs_err::File::create(context.temp_dir.child("links/demo-2.0.0.tar.gz").path())?,
        &[
            (
                "demo-2.0.0/PKG-INFO",
                indoc! {r"
                    Metadata-Version: 2.3
                    Name: demo
                    Version: 2.0.0
                    Requires-Python: >=3.12
                "},
            ),
            (
                "demo-2.0.0/pyproject.toml",
                indoc! {r#"
                    [project]
                    name = "demo"
                    version = "2.0.0"
                    requires-python = ">=3.12"
                    dependencies = []
                "#},
            ),
        ],
    )?;
    project(&context, &["demo"], &[LINUX_X86_64], Some("2.31"))?;

    let output = uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r"
        exit_code: 0 (success)
        ----- stderr -----
        Resolved 2 packages in [TIME]
    ");
    assert!(output.status.success());
    let package = locked_package(&context, "demo")?;
    assert_eq!(package["version"].as_str(), Some("2.0.0"));
    assert!(package.get("sdist").is_some());

    // Reconsider the cached flat-index entry when its source distribution cannot be built.
    let output = uv_snapshot!(filters, context.lock().args(["--offline", "--no-build", "--upgrade"]), @r"
        exit_code: 1 (failure)
        ----- stderr -----
        error: No solution found when resolving dependencies for split (markers: platform_machine == 'x86_64' and sys_platform == 'linux')
          cause: Because demo==2.0.0 has no `platform_machine == 'x86_64' and sys_platform == 'linux'`-compatible wheels and only demo==2.0.0 is available, we can conclude that all versions of demo cannot be used.
                 And because your project depends on demo, we can conclude that your project's requirements are unsatisfiable.
    ");
    assert!(!output.status.success());
    Ok(())
}

#[test]
fn minimum_glibc_direct_url() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = wheel(
        &context,
        "demo",
        "2.0.0",
        "cp312-cp312-manylinux_2_34_x86_64",
    )?;
    let server = FindLinksServer::new(context.temp_dir.child("links").path());
    let filename = wheel
        .file_name()
        .context("wheel has no file name")?
        .to_string_lossy();
    let dependency = format!("demo @ {}/{filename}", server.url());
    project(&context, &[&dependency], &[LINUX_X86_64], Some("2.31"))?;

    let output = uv_snapshot!(context.filters(), context.lock(), @r"
        exit_code: 1 (failure)
        ----- stderr -----
        error: No solution found when resolving dependencies
          cause: Because only demo==2.0.0 is available and demo==2.0.0 has no `platform_machine == 'x86_64' and sys_platform == 'linux'`-compatible wheels, we can conclude that all versions of demo cannot be used.
                 And because your project depends on demo, we can conclude that your project's requirements are unsatisfiable.
    ");
    assert!(!output.status.success());
    assert!(!context.temp_dir.child("uv.lock").exists());
    Ok(())
}

/// Every required architecture needs coverage, while unrelated marker branches remain independent.
#[test]
fn minimum_glibc_architectures_and_markers() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    wheel(
        &context,
        "demo",
        "1.0.0",
        "cp312-cp312-manylinux2014_x86_64",
    )?;
    wheel(
        &context,
        "demo",
        "1.0.0",
        "cp312-cp312-manylinux_2_31_aarch64",
    )?;
    wheel(
        &context,
        "demo",
        "2.0.0",
        "cp312-cp312-manylinux_2_17_x86_64",
    )?;
    wheel(
        &context,
        "demo",
        "2.0.0",
        "cp312-cp312-manylinux_2_34_aarch64",
    )?;
    wheel(&context, "demo", "2.0.0", "cp312-cp312-macosx_11_0_arm64")?;
    wheel(&context, "windows-only", "1.0.0", "cp312-cp312-win_amd64")?;
    let dependencies = [
        "demo; sys_platform == 'linux'",
        "demo>=2; sys_platform == 'darwin'",
        "windows-only; sys_platform == 'win32'",
    ];
    project(
        &context,
        &dependencies,
        &[LINUX_X86_64, LINUX_AARCH64],
        Some("2.31"),
    )?;

    let output = uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r"
        exit_code: 0 (success)
        ----- stderr -----
        Resolved 4 packages in [TIME]
    ");
    assert!(output.status.success());
    // The required aarch64 branch needs the older version; x86_64 and macOS retain the newer one.
    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--no-hashes", "--no-header", "--no-annotate"]), @r"
        exit_code: 0 (success)
        ----- stdout -----
        demo==1.0.0 ; platform_machine == 'aarch64' and sys_platform == 'linux'
        demo==2.0.0 ; (platform_machine != 'aarch64' and sys_platform == 'linux') or sys_platform == 'darwin'
        windows-only==1.0.0 ; sys_platform == 'win32'
    ");
    assert_eq!(
        locked_package(&context, "windows-only")?["version"].as_str(),
        Some("1.0.0")
    );

    // The newer version is valid when only its compatible x86_64 wheel is required.
    project(&context, &dependencies, &[LINUX_X86_64], Some("2.31"))?;
    let output = uv_snapshot!(context.filters(), context.lock().args(["--offline", "--upgrade"]), @r"
            exit_code: 0 (success)
            ----- stderr -----
            Resolved 3 packages in [TIME]
            Updated demo v1.0.0, v2.0.0 -> v2.0.0
        ");
    assert!(output.status.success());
    assert_eq!(
        locked_package(&context, "demo")?["version"].as_str(),
        Some("2.0.0")
    );
    Ok(())
}

#[test]
fn minimum_glibc_pip_compile_universal() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    wheel(
        &context,
        "demo",
        "1.0.0",
        "cp312-cp312-manylinux_2_17_x86_64",
    )?;
    wheel(
        &context,
        "demo",
        "2.0.0",
        "cp312-cp312-manylinux_2_34_x86_64",
    )?;
    project(&context, &["demo"], &[LINUX_X86_64], Some("2.31"))?;

    uv_snapshot!(context.filters(), context.pip_compile().args(["pyproject.toml", "--universal", "--offline", "--no-header", "--no-annotate"]), @r"
        exit_code: 0 (success)
        ----- stdout -----
        demo==1.0.0 ; platform_machine == 'x86_64' and sys_platform == 'linux'
        demo==2.0.0 ; platform_machine != 'x86_64' or sys_platform != 'linux'

        ----- stderr -----
        Resolved 2 packages in [TIME]
    ");

    project(&context, &["demo"], &[LINUX_X86_64], None)?;
    uv_snapshot!(context.filters(), context.pip_compile().args(["pyproject.toml", "--universal", "--offline", "--no-header", "--no-annotate"]), @r"
        exit_code: 0 (success)
        ----- stdout -----
        demo==2.0.0

        ----- stderr -----
        Resolved 1 package in [TIME]
    ");
    Ok(())
}

#[test]
fn minimum_glibc_invalid_configuration() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    project(&context, &[], &[LINUX_X86_64], Some("2.31.1"))?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r#"
        exit_code: 2 (failure)
        ----- stderr -----
        warning: Failed to parse `pyproject.toml` during settings discovery:
          TOML parse error at line 11, column 25
             |
          11 | minimum-glibc-version = "2.31.1"
             |                         ^^^^^^^^
          expected a glibc version in the form `<major>.<minor>` (e.g., `2.31`)

        error: Failed to parse: `pyproject.toml`
          cause: TOML parse error at line 11, column 25
                    |
                 11 | minimum-glibc-version = "2.31.1"
                    |                         ^^^^^^^^
                 expected a glibc version in the form `<major>.<minor>` (e.g., `2.31`)
    "#);

    // Like required-environments, the minimum glibc version is a project-only setting.
    project(&context, &[], &[LINUX_X86_64], None)?;
    context.temp_dir.child("uv.toml").write_str(indoc! {r#"
        minimum-glibc-version = "2.31"
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r"
        exit_code: 2 (failure)
        ----- stderr -----
        warning: Found both a `uv.toml` file and a `[tool.uv]` section in an adjacent `pyproject.toml`. The following fields from `[tool.uv]` will be ignored in favor of the `uv.toml` file:
        - no-index
        - find-links
        error: Failed to parse: `uv.toml`. The `minimum-glibc-version` field is not allowed in a `uv.toml` file. `minimum-glibc-version` is only applicable in the context of a project, and should be placed in a `pyproject.toml` file instead.
    ");
    Ok(())
}

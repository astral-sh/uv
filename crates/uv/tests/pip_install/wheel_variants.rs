use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use uv_static::EnvVars;
use uv_test::uv_snapshot;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn variant_wheels_cannot_be_installed() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let wheel = links.child("ok-1.0.0-9-py3-none-any-null.whl");
    fs_err::copy(
        context
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl"),
        wheel.path(),
    )?;

    uv_snapshot!(
        context.filters(),
        context
            .pip_install()
            .arg("--no-index")
            .arg("--find-links")
            .arg(links.path())
            .arg("ok==1.0.0"),
        @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because ok==1.0.0 has only unsupported wheel variants and you require ok==1.0.0, we can conclude that your requirements are unsatisfiable.
    "
    );

    uv_snapshot!(
        context.filters(),
        context
            .pip_install()
            .arg("--no-index")
            .arg("--find-links")
            .arg(links.path())
            .arg("--preview-features")
            .arg("wheel-variants")
            .arg("ok==1.0.0"),
        @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because ok==1.0.0 has only unsupported wheel variants and you require ok==1.0.0, we can conclude that your requirements are unsatisfiable.
    "
    );

    uv_snapshot!(context.filters(), context.pip_install().arg(wheel.path()), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to read `ok @ file://[TEMP_DIR]/links/ok-1.0.0-9-py3-none-any-null.whl`
      cause: Wheel variants are not supported yet: `ok-1.0.0-9-py3-none-any-null.whl`
    ");

    uv_snapshot!(
        context.filters(),
        context
            .pip_install()
            .arg("--preview-features")
            .arg("wheel-variants")
            .arg(wheel.path()),
        @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to read `ok @ file://[TEMP_DIR]/links/ok-1.0.0-9-py3-none-any-null.whl`
      cause: Wheel variants are not supported yet: `ok-1.0.0-9-py3-none-any-null.whl`
    "
    );
    Ok(())
}

#[test]
fn ordinary_wheels_are_selected_over_variants() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    fs_err::copy(
        context
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl"),
        links.child("ok-1.0.0-py3-none-any.whl"),
    )?;
    // A higher build tag and a newer version must not make a variant selectable.
    links
        .child("ok-1.0.0-9-py3-none-any-cpu.whl")
        .write_str("invalid wheel")?;
    links
        .child("ok-2.0.0-py3-none-any-null.whl")
        .write_str("invalid wheel")?;

    uv_snapshot!(
        context.filters(),
        context
            .pip_install()
            .arg("--no-index")
            .arg("--find-links")
            .arg(links.path())
            .arg("--preview-features")
            .arg("wheel-variants")
            .arg("ok"),
        @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0
    "
    );
    Ok(())
}

#[test]
fn universal_lock_excludes_variant_wheels() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    fs_err::copy(
        context
            .workspace_root
            .join("test/links/ok-1.0.0-py3-none-any.whl"),
        links.child("ok-1.0.0-py3-none-any.whl"),
    )?;
    links
        .child("ok-1.0.0-9-py3-none-any-cpu.whl")
        .write_str("invalid wheel")?;
    links
        .child("ok-2.0.0-py3-none-any-null.whl")
        .write_str("invalid wheel")?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "example"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["ok"]
    "#})?;

    uv_snapshot!(
        context.filters(),
        context
            .lock()
            .arg("--no-index")
            .arg("--find-links")
            .arg(links.path())
            .arg("--preview-features")
            .arg("wheel-variants"),
        @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    "
    );
    insta::with_settings!({filters => context.filters()}, {
        insta::assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 5
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "example"
        version = "1.0.0"
        source = { virtual = "." }
        dependencies = [
            { name = "ok" },
        ]

        [package.metadata]
        requires-dist = [{ name = "ok" }]

        [[package]]
        name = "ok"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/links" }
        wheels = [
            { path = "[TEMP_DIR]/links/ok-1.0.0-py3-none-any.whl" },
        ]
        "#);
    });

    // A lockfile containing a variant wheel must not make it installable.
    let lock = context.read("uv.lock").replace(
        "ok-1.0.0-py3-none-any.whl",
        "ok-1.0.0-9-py3-none-any-cpu.whl",
    );
    context.temp_dir.child("uv.lock").write_str(&lock)?;
    uv_snapshot!(context.filters(), context.sync()
        .arg("--frozen")
        .arg("--preview-features")
        .arg("wheel-variants"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Distribution `ok==1.0.0 @ registry+[TEMP_DIR]/links` can't be installed because it doesn't have a source distribution or wheel for the current platform
    ");
    Ok(())
}

#[tokio::test]
async fn registry_variant_wheels_are_not_selectable() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/simple/ok/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"<a href="../../files/ok-2.0.0-py3-none-any-null.whl">ok-2.0.0-py3-none-any-null.whl</a>"#,
            "text/html",
        ))
        .mount(&server)
        .await;
    let context = uv_test::test_context!("3.12").with_filter((server.uri(), "http://[LOCALHOST]"));

    uv_snapshot!(
        context.filters(),
        context
            .pip_install()
            .arg("--index-url")
            .arg(format!("{}/simple/", server.uri()))
            .env_remove(EnvVars::UV_EXCLUDE_NEWER)
            .arg("--preview-features")
            .arg("wheel-variants")
            .arg("ok"),
        @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because ok==2.0.0 has only unsupported wheel variants and only ok==2.0.0 is available, we can conclude that all versions of ok cannot be used.
             And because you require ok, we can conclude that your requirements are unsatisfiable.
    "
    );
    uv_snapshot!(context.filters(), context.pip_install()
        .arg(context.workspace_root.join("test/links/ok-1.0.0-py3-none-any.whl")), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + ok==1.0.0 (from file://[WORKSPACE]/test/links/ok-1.0.0-py3-none-any.whl)
    ");
    uv_snapshot!(context.filters(), context.pip_list()
        .arg("--outdated")
        .arg("--format")
        .arg("json")
        .arg("--index-url")
        .arg(format!("{}/simple/", server.uri()))
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--preview-features")
        .arg("wheel-variants"), @"
    exit_code: 0 (success)
    ----- stdout -----
    []
    ");
    Ok(())
}

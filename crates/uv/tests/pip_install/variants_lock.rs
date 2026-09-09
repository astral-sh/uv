//! Selection of artifacts and dependency markers when installing a variant lockfile.

use std::path::Path;

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;

use uv_test::packse::PackseServer;
use uv_test::packse::scenario::{ArtifactMetadata, Scenario};
use uv_test::uv_snapshot;

/// Switching a locked registry wheel must also switch the digest used to verify its download.
#[test]
fn pep825_project_variant_hashes() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("variants/variants-basic.toml");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["cpu-first"]
    "#})?;
    uv_snapshot!(context.filters(), context.sync()
        .env_remove("UV_VARIANT_LOCK").env_remove("UV_VARIANT_LOCK_INCOMPLETE")
        .arg("--preview-features").arg("wheel-variants")
        .arg("--index-url").arg(server.index_url())
        .env("PROVIDER_CPU_LEVEL", "3"), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + cpu-first==1.0.0
    "###);
    uv_snapshot!(context.filters(), context.python_command().arg("-c").arg(indoc! {r#"
        import json
        from importlib.metadata import distribution
        print(next(iter(json.loads(distribution("cpu-first").read_text("variant.json"))["variants"])))
    "#}), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    closedblas_v3
    "###);
    uv_snapshot!(context.filters(), context.sync()
        .env_remove("UV_VARIANT_LOCK").env_remove("UV_VARIANT_LOCK_INCOMPLETE")
        .arg("--preview-features").arg("wheel-variants")
        .arg("--frozen")
        .arg("--index-url").arg(server.index_url())
        .env("PROVIDER_CPU_LEVEL", "2"), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ cpu-first==1.0.0
    "###);
    uv_snapshot!(context.filters(), context.python_command().arg("-c").arg(indoc! {r#"
        import json
        from importlib.metadata import distribution
        print(next(iter(json.loads(distribution("cpu-first").read_text("variant.json"))["variants"])))
    "#}), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    openblas_v2
    "###);
    uv_snapshot!(context.filters(), context.pip_check()
        .env_remove("UV_VARIANT_LOCK").env_remove("UV_VARIANT_LOCK_INCOMPLETE")
        .env("PROVIDER_CPU_LEVEL", "invalid"), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Checked 1 package in [TIME]
    All installed packages are compatible
    "###);
    Ok(())
}

/// An excluded dependency need not have compatible wheels. Selecting an sdist with --no-binary
/// must clear variant properties without querying the wheel's runtime provider.
#[test]
fn pep825_project_excluded_variant_dependency() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let mut scenario = Scenario::from_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/scenarios/variants/variants-marker.toml"),
    )?;
    let variant_package = scenario
        .packages
        .get_mut(&"a".parse()?)
        .expect("fixture package");
    let version = variant_package
        .versions
        .get_mut(&"1.0.0".parse()?)
        .expect("fixture version");
    version.sdist = Some(ArtifactMetadata::default());
    version
        .variants
        .as_mut()
        .expect("fixture variants")
        .non_variant_wheel = false;
    let excluded = scenario
        .packages
        .get_mut(&"cpu-v2".parse()?)
        .expect("fixture dependency")
        .versions
        .get_mut(&"1.0.0".parse()?)
        .expect("fixture version");
    excluded.sdist = None;
    excluded.wheel_tags = vec!["py3-none-win_amd64".parse().expect("valid wheel tag")];
    let server = PackseServer::from_scenario(&scenario);
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["a"]
    "#})?;
    uv_snapshot!(context.filters(), context.sync()
        .env_remove("UV_VARIANT_LOCK").env_remove("UV_VARIANT_LOCK_INCOMPLETE")
        .arg("--preview-features").arg("wheel-variants")
        .arg("--python-platform").arg("x86_64-unknown-linux-gnu")
        .arg("--index-url").arg(server.index_url())
        .env("PROVIDER_CPU_LEVEL", "3"), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 9 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + a==1.0.0
     + cpu-v1==1.0.0
     + cpu-v3==1.0.0
     + feature-cpu-level==1.0.0
     + namespace-cpu==1.0.0
    "###);
    uv_snapshot!(context.filters(), context.sync()
        .env_remove("UV_VARIANT_LOCK").env_remove("UV_VARIANT_LOCK_INCOMPLETE")
        .arg("--preview-features").arg("wheel-variants")
        .arg("--python-platform").arg("x86_64-unknown-linux-gnu")
        .arg("--index-url").arg(server.index_url())
        .arg("--frozen").arg("--dry-run").arg("--reinstall")
        .arg("--no-binary-package").arg("a")
        .env("PROVIDER_CPU_LEVEL", "invalid"), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Would use project environment at: .venv
    Would download 1 package
    Would uninstall 5 packages
    Would install 1 package
     - a==1.0.0
     + a==1.0.0
     - cpu-v1==1.0.0
     - cpu-v3==1.0.0
     - feature-cpu-level==1.0.0
     - namespace-cpu==1.0.0
    "###);
    // Unsupported variant properties also fall back to the sdist, clearing wheel markers.
    uv_snapshot!(context.filters(), context.sync()
        .env_remove("UV_VARIANT_LOCK").env_remove("UV_VARIANT_LOCK_INCOMPLETE")
        .arg("--preview-features").arg("wheel-variants")
        .arg("--python-platform").arg("x86_64-unknown-linux-gnu")
        .arg("--index-url").arg(server.index_url())
        .arg("--frozen").arg("--dry-run").arg("--reinstall")
        .env("PROVIDER_CPU_LEVEL", "0"), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Would use project environment at: .venv
    Would download 1 package
    Would uninstall 5 packages
    Would install 1 package
     - a==1.0.0
     + a==1.0.0
     - cpu-v1==1.0.0
     - cpu-v3==1.0.0
     - feature-cpu-level==1.0.0
     - namespace-cpu==1.0.0
    "###);
    uv_snapshot!(context.filters(), context.sync()
        .env_remove("UV_VARIANT_LOCK").env_remove("UV_VARIANT_LOCK_INCOMPLETE")
        .arg("--preview-features").arg("wheel-variants")
        .arg("--python-platform").arg("x86_64-unknown-linux-gnu")
        .arg("--index-url").arg(server.index_url())
        .arg("--frozen").arg("--dry-run").arg("--reinstall")
        .arg("--no-build-package").arg("a")
        .env("PROVIDER_CPU_LEVEL", "0"), @r###"
    exit_code: 2 (failure)
    ----- stderr -----
    Would use project environment at: .venv
    error: Distribution `a==1.0.0 @ registry+http://[LOCALHOST]/simple/` can't be installed because it is marked as `--no-build` but has no binary distribution
    "###);
    // Omitting the parent wheel's installation must preserve its selected dependencies.
    uv_snapshot!(context.filters(), context.sync()
        .env_remove("UV_VARIANT_LOCK").env_remove("UV_VARIANT_LOCK_INCOMPLETE")
        .arg("--preview-features").arg("wheel-variants")
        .arg("--python-platform").arg("x86_64-unknown-linux-gnu")
        .arg("--index-url").arg(server.index_url())
        .arg("--frozen").arg("--no-install-package").arg("a")
        .env("PROVIDER_CPU_LEVEL", "3"), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - a==1.0.0
    "###);
    Ok(())
}

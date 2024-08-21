#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use insta::assert_snapshot;

use common::TestContext;

mod common;

// These tests just run `uv lock` on an assorted of ecosystem
// projects.
//
// The idea here is to provide a body of ecosystem projects that
// let us very easily observe any changes to the actual resolution
// produced in the lock file.

/// We use a different exclude newer here because, at the time of
/// creating these benchmarks, the `pyproject.toml` files from the
/// projects wouldn't work with the exclude-newer value we use
/// elsewhere (which is 2024-03-25 at time of writing). So Instead of
/// bumping everything else, we just use our own here.
static EXCLUDE_NEWER: &str = "2024-08-08T00:00:00Z";

// Source: https://github.com/astral-sh/packse/blob/737bc7008fa7825669ee50e90d9d0c26df32a016/pyproject.toml
#[test]
fn packse() -> Result<()> {
    lock_ecosystem_package("3.12", "packse")
}

// Source: https://github.com/konstin/github-wikidata-bot/blob/8218d20985eb480cb8633026f9dabc9e5ec4b5e3/pyproject.toml
#[test]
fn github_wikidata_bot() -> Result<()> {
    lock_ecosystem_package("3.12", "github-wikidata-bot")
}

// Source: https://github.com/psf/black/blob/9ff047a9575f105f659043f28573e1941e9cdfb3/pyproject.toml
#[test]
fn black() -> Result<()> {
    lock_ecosystem_package("3.12", "black")
}

// Source: https://github.com/home-assistant/core/blob/7c5fcec062e1d2cfaa794a169fafa629a70bbc9e/pyproject.toml
#[test]
fn home_assistant_core() -> Result<()> {
    lock_ecosystem_package("3.12", "home-assistant-core")
}

// Source: https://github.com/konstin/transformers/blob/da3c00433d93e43bf1e7360b1057e8c160e7978e/pyproject.toml
#[test]
fn transformers() -> Result<()> {
    // Takes too long on non-Linux in CI.
    if !cfg!(target_os = "linux") && std::env::var_os("CI").is_some() {
        return Ok(());
    }
    lock_ecosystem_package("3.12", "transformers")
}

// Source: https://github.com/konstin/warehouse/blob/baae127d90417104c8dee3fdd3855e2ba17aa428/pyproject.toml
#[test]
fn warehouse() -> Result<()> {
    // This build requires running `pg_config`. We could
    // probably stub it out, but for now, we just skip the
    // test if we can't run `pg_config`.
    if std::process::Command::new("pg_config").output().is_err() {
        return Ok(());
    }
    // Also, takes too long on non-Linux in CI.
    if !cfg!(target_os = "linux") && std::env::var_os("CI").is_some() {
        return Ok(());
    }
    lock_ecosystem_package("3.11", "warehouse")
}

// Currently ignored because the project doesn't build with `uv` yet.
//
// Source: https://github.com/apache/airflow/blob/c55438d9b2eb9b6680641eefdd0cbc67a28d1d29/pyproject.toml
#[ignore]
#[test]
fn airflow() -> Result<()> {
    lock_ecosystem_package("3.12", "airflow")
}

// Currently ignored because the project doesn't build with `uv` yet.
//
// Source: https://github.com/pretix/pretix/blob/a682eab18e9421dc0aff18a6ed8495aa3c75c39b/pyproject.toml
#[ignore]
#[test]
fn pretix() -> Result<()> {
    lock_ecosystem_package("3.12", "pretix")
}

/// Does a lock on the given ecosystem package for the given name. That
/// is, there should be a directory at `./ecosystem/{name}` from the
/// root of the `uv` repository.
fn lock_ecosystem_package(python_version: &str, name: &str) -> Result<()> {
    let context = TestContext::new(python_version);
    context.copy_ecosystem_project(name);

    let mut cmd = context.lock();
    cmd.env("UV_EXCLUDE_NEWER", EXCLUDE_NEWER);
    let (snapshot, _) = common::run_and_format(
        &mut cmd,
        context.filters(),
        name,
        Some(common::WindowsFilters::Platform),
    );
    insta::assert_snapshot!(format!("{name}-uv-lock-output"), snapshot);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(format!("{name}-lock-file"), lock);
    });
    Ok(())
}

use anyhow::Result;
use assert_fs::fixture::ChildPath;
use insta::assert_snapshot;
use std::path::Path;
use uv_static::EnvVars;

// These tests just run `uv lock` on an assorted of ecosystem
// projects.
//
// The idea here is to provide a body of ecosystem projects that
// let us very easily observe any changes to the actual resolution
// produced in the lock file.

/// Use a fixed cutoff so that ecosystem resolutions remain deterministic.
static EXCLUDE_NEWER: &str = "2026-06-30T00:00:00Z";

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

// Source: astral-sh/pyx at 5752f1cd9766b9df934658ceaeb10eb37986e54d.
//
// This fixture combines the external project and dependency-group requirements
// from every workspace member, while omitting the private workspace packages.
// The Python patch constraint is widened from ==3.14.5 to ==3.14.* so the test
// can use the available 3.14 patch release on every platform.
#[test]
fn pyx_external() -> Result<()> {
    lock_ecosystem_package("3.14", "pyx-external")
}

// Source: https://github.com/python-poetry/poetry/blob/811a12dae0fe81f199e3f1b88b8b8be9eed543c2/pyproject.toml
#[test]
fn poetry() -> Result<()> {
    lock_ecosystem_package("3.12", "poetry")
}

// Source: https://github.com/home-assistant/core/blob/7c5fcec062e1d2cfaa794a169fafa629a70bbc9e/pyproject.toml
#[test]
fn home_assistant_core() -> Result<()> {
    lock_ecosystem_package("3.12", "home-assistant-core")
}

// Source: https://github.com/konstin/transformers/blob/da3c00433d93e43bf1e7360b1057e8c160e7978e/pyproject.toml
#[test]
#[cfg(unix)] // deepspeed fails on windows due to missing torch
fn transformers() -> Result<()> {
    // Takes too long on non-Linux in CI.
    if skip_slow_ecosystem_test_on_non_linux_ci() {
        return Ok(());
    }
    lock_ecosystem_package("3.12", "transformers")
}

// Source: https://github.com/konstin/warehouse/blob/baae127d90417104c8dee3fdd3855e2ba17aa428/pyproject.toml
#[test]
fn warehouse() -> Result<()> {
    // Also, takes too long on non-Linux in CI.
    if skip_slow_ecosystem_test_on_non_linux_ci() {
        return Ok(());
    }
    lock_ecosystem_package("3.11", "warehouse")
}

// Source: https://github.com/saleor/saleor/blob/6e6f3eee4f6a33b64c3d05348215062ca732c1ca/pyproject.toml
#[test]
fn saleor() -> Result<()> {
    lock_ecosystem_package("3.12", "saleor")
}

// Currently ignored because the project doesn't build with `uv` yet.
//
// Source: https://github.com/apache/airflow/blob/c55438d9b2eb9b6680641eefdd0cbc67a28d1d29/pyproject.toml
#[test]
#[ignore = "Airflow doesn't build with `uv` yet"]
fn airflow() -> Result<()> {
    lock_ecosystem_package("3.12", "airflow")
}

// Source: https://github.com/pandas-dev/pandas/blob/8188eb1d65d6250c9916e54a0fa417d46af3296a/pyproject.toml
//
// The dynamically derived project version is replaced with the version from
// the pinned release. The dependency declarations are unchanged.
#[test]
fn pandas() -> Result<()> {
    if skip_slow_ecosystem_test_on_non_linux_ci() {
        return Ok(());
    }
    lock_ecosystem_package("3.14", "pandas")
}

// Source: https://github.com/jupyterlab/jupyterlab/blob/665f9b7f77fb6d720d9cfa76c38fdd1d9823cd07/pyproject.toml
//
// The dynamically derived project version is replaced with the version from
// the pinned release. The dependency declarations are unchanged.
#[test]
fn jupyterlab() -> Result<()> {
    if skip_slow_ecosystem_test_on_non_linux_ci() {
        return Ok(());
    }
    lock_ecosystem_package("3.12", "jupyterlab")
}

// Source: https://github.com/microsoft/semantic-kernel/blob/cd1b0205fa424aa75b7bc1cc8ea7c071dc5e93a9/python/pyproject.toml
//
// The dynamically derived project version is replaced with the version from
// the pinned release. The dependency declarations are unchanged.
#[test]
fn semantic_kernel() -> Result<()> {
    if skip_slow_ecosystem_test_on_non_linux_ci() {
        return Ok(());
    }
    lock_ecosystem_package("3.12", "semantic-kernel")
}

fn skip_slow_ecosystem_test_on_non_linux_ci() -> bool {
    !cfg!(target_os = "linux") && std::env::var_os(EnvVars::CI).is_some()
}

/// Does a lock on the given ecosystem package for the given name. That
/// is, there should be a directory at `./test/ecosystem/{name}` from the
/// root of the `uv` repository.
fn lock_ecosystem_package(python_version: &str, name: &str) -> Result<()> {
    let mut context = uv_test::test_context!(python_version);
    context.copy_ecosystem_project(name);

    // Cache source distribution builds to speed up the tests.
    let cache_dir =
        std::path::absolute(Path::new("../../target/ecosystem-test-caches").join(name))?;
    context.cache_dir = ChildPath::new(cache_dir);

    let mut command = context.lock();
    command.env(EnvVars::UV_EXCLUDE_NEWER, EXCLUDE_NEWER);

    let (snapshot, _) = uv_test::run_and_format(
        &mut command,
        context.filters(),
        name,
        Some(uv_test::WindowsFilters::Platform),
        None,
    );

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(format!("{name}-lock-file"), lock);
    });

    assert_snapshot!(format!("{name}-uv-lock-output"), snapshot);

    Ok(())
}

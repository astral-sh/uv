use anyhow::{Context, Result};
use insta::assert_snapshot;
use std::path::Path;
use uv_resolver::Lock;
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

// Source: https://github.com/pypa/cibuildwheel/blob/294735312765b09d24a2fbec22660ce817587d55/pyproject.toml
#[test]
fn cibuildwheel() -> Result<()> {
    lock_ecosystem_package_without_build("3.12", "cibuildwheel")
}

// Source: https://github.com/cookiecutter/cookiecutter/blob/083dd3c6104124221e2cbc3e13e0929795861ed5/pyproject.toml
#[test]
fn cookiecutter() -> Result<()> {
    lock_ecosystem_package_without_build("3.12", "cookiecutter")
}

// Source: https://github.com/pallets/flask/blob/06ea505ce2b2042af26e96d35ebf159af7c0869d/pyproject.toml
#[test]
fn flask() -> Result<()> {
    lock_ecosystem_package_without_build("3.12", "flask")
}

// Source: https://github.com/encode/httpx/blob/767cf6baa608a56d03f8fe438a39c2013904f0ae/pyproject.toml
//
// Replace the dynamically derived version with the version from the pinned
// revision so locking does not execute the build backend.
#[test]
fn httpx() -> Result<()> {
    lock_ecosystem_package_without_build("3.12", "httpx")
}

// Source: https://github.com/simonw/llm/blob/512659547241a61e30116e9ada4db34a624062ae/pyproject.toml
#[test]
fn llm() -> Result<()> {
    lock_ecosystem_package_without_build("3.12", "llm")
}

// Source: https://github.com/openai/openai-python/blob/6d9262d5c666a1e4d47f63178db907ba3087ac5d/pyproject.toml
#[test]
fn openai_python() -> Result<()> {
    lock_ecosystem_package_without_build("3.12", "openai-python")
}

// Source: https://github.com/pytest-dev/pytest-cov/blob/66c8a526b1246b5eb8fb1bc218878131bc628622/pyproject.toml
#[test]
fn pytest_cov() -> Result<()> {
    lock_ecosystem_package_without_build("3.12", "pytest-cov")
}

// Source: astral-sh/pyx at 5752f1cd9766b9df934658ceaeb10eb37986e54d.
//
// This fixture combines the external project and dependency-group requirements
// from every workspace member, while omitting the private workspace packages
// and `atlas-provider-sqlalchemy`, which requires building a transitive sdist.
// The Python patch constraint is widened from ==3.14.5 to ==3.14.* so the test
// can use the available 3.14 patch release on every platform.
#[test]
fn pyx_external() -> Result<()> {
    lock_ecosystem_package_without_build("3.14", "pyx-external")
}

// Source: astral-sh/pyx at 5752f1cd9766b9df934658ceaeb10eb37986e54d.
//
// The sdist-only `atlas-provider-sqlalchemy` dependency is omitted, and the
// exact Python patch requirement is widened to Python 3.14.
#[test]
fn pyx_workspace() -> Result<()> {
    lock_ecosystem_package_without_build("3.14", "pyx-workspace")
}

// Source: https://github.com/python-poetry/poetry/blob/811a12dae0fe81f199e3f1b88b8b8be9eed543c2/pyproject.toml
#[test]
fn poetry() -> Result<()> {
    lock_ecosystem_package_without_build("3.12", "poetry")
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

// Source: https://github.com/getsentry/sentry/blob/3d20b99264b1afa6d4d3b356c3bba0d27cd069ae/pyproject.toml
//
// Use the upstream Python 3.13 interpreter and public package index, omitting
// dependencies distributed only as source archives.
#[test]
fn sentry() -> Result<()> {
    lock_ecosystem_package_without_build("3.13", "sentry")
}

// Source: https://github.com/zulip/zulip/blob/73a1152e4b1a3dfee3c7d161ce1fb711600f95b8/pyproject.toml
//
// Keep the application's production and development dependency groups while
// omitting Git-only and source-only dependencies.
#[test]
fn zulip() -> Result<()> {
    lock_ecosystem_package_without_build("3.12", "zulip")
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
// the pinned release. The sdist-only `odfpy` dependency is omitted.
#[test]
fn pandas() -> Result<()> {
    if skip_slow_ecosystem_test_on_non_linux_ci() {
        return Ok(());
    }
    lock_ecosystem_package_without_build("3.14", "pandas")
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
    lock_ecosystem_package_without_build("3.12", "jupyterlab")
}

// Source: https://github.com/microsoft/semantic-kernel/blob/cd1b0205fa424aa75b7bc1cc8ea7c071dc5e93a9/python/pyproject.toml
//
// The dynamically derived project version is replaced with the version from
// the pinned release. The sdist-only `pybars4` dependency is omitted.
#[test]
fn semantic_kernel() -> Result<()> {
    if skip_slow_ecosystem_test_on_non_linux_ci() {
        return Ok(());
    }
    lock_ecosystem_package_without_build("3.12", "semantic-kernel")
}

fn skip_slow_ecosystem_test_on_non_linux_ci() -> bool {
    !cfg!(target_os = "linux") && std::env::var_os(EnvVars::CI).is_some()
}

/// Does a lock on the given ecosystem package for the given name. That
/// is, there should be a directory at `./test/ecosystem/{name}` from the
/// root of the `uv` repository.
fn lock_ecosystem_package(python_version: &str, name: &str) -> Result<()> {
    lock_ecosystem_package_with_args(python_version, name, &[])
}

/// Lock an ecosystem package while preventing build backend execution.
fn lock_ecosystem_package_without_build(python_version: &str, name: &str) -> Result<()> {
    lock_ecosystem_package_with_args(python_version, name, &["--no-build"])
}

fn lock_ecosystem_package_with_args(python_version: &str, name: &str, args: &[&str]) -> Result<()> {
    // Cache source distribution builds to speed up the tests.
    let cache_dir =
        std::path::absolute(Path::new("../../target/ecosystem-test-caches").join(name))?;
    let context = uv_test::test_context!(python_version).with_cache_dir(cache_dir);
    context.copy_ecosystem_project(name);

    let mut command = context.lock();
    command.env(EnvVars::UV_EXCLUDE_NEWER, EXCLUDE_NEWER);
    command.args(args);

    let (snapshot, _) = uv_test::run_and_format(
        &mut command,
        context.filters(),
        name,
        Some(uv_test::WindowsFilters::Platform),
        None,
    );

    // Ensure generated lockfiles take the canonical fast path and produce the
    // same lock as the general TOML parser.
    let lock = context.read("uv.lock");
    let expected = toml::from_str::<Lock>(&lock)
        .with_context(|| format!("failed to parse the `{name}` lockfile as TOML"))?;
    let actual = Lock::from_canonical_toml(&lock)
        .with_context(|| format!("the `{name}` lockfile did not use the canonical fast path"))?;
    assert_eq!(
        actual, expected,
        "the canonical fast path changed the `{name}` lockfile"
    );

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(format!("{name}-lock-file"), lock);
    });

    assert_snapshot!(format!("{name}-uv-lock-output"), snapshot);

    Ok(())
}

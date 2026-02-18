#![allow(clippy::disallowed_types)]

#[cfg(feature = "test-git")]
mod conditional_imports {
    pub(crate) use uv_test::{READ_ONLY_GITHUB_TOKEN, decode_token};
}

#[cfg(feature = "test-git")]
use conditional_imports::*;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;
use std::path::Path;
use url::Url;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method, matchers::path};

#[cfg(feature = "test-git-lfs")]
use uv_cache_key::{RepositoryUrl, cache_digest};
use uv_fs::Simplified;
use uv_static::EnvVars;

use uv_test::{packse_index_url, uv_snapshot, venv_bin_path};

/// Add a PyPI requirement.
#[test]
fn add_registry() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio==3.7.0",
        ]
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737, upload-time = "2023-05-27T11:12:46.688Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873, upload-time = "2023-05-27T11:12:44.474Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", specifier = "==3.7.0" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 3 packages in [TIME]
    ");

    Ok(())
}

/// Add a Git requirement.
#[test]
#[cfg(feature = "test-git")]
fn add_git() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    // Adding with an ambiguous Git reference should treat it as a revision.
    uv_snapshot!(context.filters(), context.add().arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@0.0.1"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    ");

    uv_snapshot!(context.filters(), context.add().arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage").arg("--tag=0.0.1"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Audited 4 packages in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio==3.7.0",
            "uv-public-pypackage",
        ]

        [tool.uv.sources]
        uv-public-pypackage = { git = "https://github.com/astral-test/uv-public-pypackage", tag = "0.0.1" }
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737, upload-time = "2023-05-27T11:12:46.688Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873, upload-time = "2023-05-27T11:12:44.474Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "anyio" },
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", specifier = "==3.7.0" },
            { name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage?tag=0.0.1" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?tag=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 4 packages in [TIME]
    ");

    Ok(())
}

/// Add a Git requirement from a private repository, with credentials. The resolution should
/// succeed, but the `pyproject.toml` should omit the credentials.
#[test]
#[cfg(feature = "test-git")]
fn add_git_private_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let token = decode_token(READ_ONLY_GITHUB_TOKEN);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg(format!("uv-private-pypackage @ git+https://{token}@github.com/astral-test/uv-private-pypackage")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-private-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-private-pypackage@d780faf0ac91257d4d5a4f0c5a0e4509608c0071)
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "uv-private-pypackage",
        ]

        [tool.uv.sources]
        uv-private-pypackage = { git = "https://github.com/astral-test/uv-private-pypackage" }
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "uv-private-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-private-pypackage", git = "https://github.com/astral-test/uv-private-pypackage" }]

        [[package]]
        name = "uv-private-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-private-pypackage#d780faf0ac91257d4d5a4f0c5a0e4509608c0071" }
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    ");

    Ok(())
}

/// Add a Git requirement from a private repository, with credentials. Since `--raw-sources` is
/// specified, the `pyproject.toml` should retain the credentials.
#[test]
#[cfg(feature = "test-git")]
fn add_git_private_raw() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let token = decode_token(READ_ONLY_GITHUB_TOKEN);
    let context = context.with_filter((&token, "***"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg(format!("uv-private-pypackage @ git+https://{token}@github.com/astral-test/uv-private-pypackage")).arg("--raw-sources"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-private-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-private-pypackage@d780faf0ac91257d4d5a4f0c5a0e4509608c0071)
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters()
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "uv-private-pypackage @ git+https://***@github.com/astral-test/uv-private-pypackage",
        ]
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "uv-private-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-private-pypackage", git = "https://github.com/astral-test/uv-private-pypackage" }]

        [[package]]
        name = "uv-private-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-private-pypackage#d780faf0ac91257d4d5a4f0c5a0e4509608c0071" }
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "test-git")]
async fn add_git_private_rate_limited_by_github_rest_api_403_response() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let token = decode_token(READ_ONLY_GITHUB_TOKEN);

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context
        .add()
        .arg(format!("uv-private-pypackage @ git+https://{token}@github.com/astral-test/uv-private-pypackage"))
        .env(EnvVars::UV_GITHUB_FAST_PATH_URL, server.uri()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-private-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-private-pypackage@d780faf0ac91257d4d5a4f0c5a0e4509608c0071)
    ");

    Ok(())
}

#[tokio::test]
#[cfg(feature = "test-git")]
async fn add_git_private_rate_limited_by_github_rest_api_429_response() -> Result<()> {
    use uv_client::DEFAULT_RETRIES;

    let context = uv_test::test_context!("3.12");
    let token = decode_token(READ_ONLY_GITHUB_TOKEN);

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1 + u64::from(DEFAULT_RETRIES)) // Middleware retries on 429 by default
        .mount(&server)
        .await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context
        .add()
        .arg(format!("uv-private-pypackage @ git+https://{token}@github.com/astral-test/uv-private-pypackage"))
        .env(EnvVars::UV_GITHUB_FAST_PATH_URL, server.uri())
        .env(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY, "true"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-private-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-private-pypackage@d780faf0ac91257d4d5a4f0c5a0e4509608c0071)
    ");

    Ok(())
}

#[test]
#[cfg(feature = "test-git")]
fn add_git_error() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited in [TIME]
    ");

    // Provide a tag without a Git source.
    uv_snapshot!(context.filters(), context.add().arg("flask").arg("--tag").arg("0.0.1"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `flask` did not resolve to a Git repository, but a Git reference (`--tag 0.0.1`) was provided.
    ");

    // Provide a tag with a non-Git source.
    uv_snapshot!(context.filters(), context.add().arg("flask @ https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl").arg("--branch").arg("0.0.1"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `flask` did not resolve to a Git repository, but a Git reference (`--branch 0.0.1`) was provided.
    ");

    Ok(())
}

#[test]
#[cfg(feature = "test-git")]
fn add_git_branch() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage").arg("--branch").arg("test-branch"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    ");

    Ok(())
}

#[test]
#[cfg(feature = "test-git-lfs")]
fn add_git_lfs() -> Result<()> {
    let context = uv_test::test_context!("3.13").with_git_lfs_config();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.13"
        dependencies = []
    "#})?;

    // Gather cache locations
    let git_cache = context.cache_dir.child("git-v0");
    let git_checkouts = git_cache.child("checkouts");
    let git_db = git_cache.child("db");
    let repo_url = RepositoryUrl::parse("https://github.com/astral-sh/test-lfs-repo")?;
    let lfs_db_bucket_objects = git_db
        .child(cache_digest(&repo_url))
        .child(".git")
        .child("lfs");
    let ok_checkout_file = git_checkouts
        .child(cache_digest(&repo_url.with_lfs(Some(true))))
        .child("0fe88f7")
        .child(".ok");

    uv_snapshot!(context.filters(), context.add()
        .arg("--no-cache")
        .arg("test-lfs-repo @ git+https://github.com/astral-sh/test-lfs-repo")
        .arg("--rev").arg("0fe88f7c2e2883521bf065c108d9ee8eb115674b")
        .arg("--lfs"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@0fe88f7c2e2883521bf065c108d9ee8eb115674b#lfs=true)
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.13"
        dependencies = [
            "test-lfs-repo",
        ]

        [tool.uv.sources]
        test-lfs-repo = { git = "https://github.com/astral-sh/test-lfs-repo", rev = "0fe88f7c2e2883521bf065c108d9ee8eb115674b", lfs = true }
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.13"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "test-lfs-repo" },
        ]

        [package.metadata]
        requires-dist = [{ name = "test-lfs-repo", git = "https://github.com/astral-sh/test-lfs-repo?lfs=true&rev=0fe88f7c2e2883521bf065c108d9ee8eb115674b" }]

        [[package]]
        name = "test-lfs-repo"
        version = "0.1.0"
        source = { git = "https://github.com/astral-sh/test-lfs-repo?lfs=true&rev=0fe88f7c2e2883521bf065c108d9ee8eb115674b#0fe88f7c2e2883521bf065c108d9ee8eb115674b" }
        "#
        );
    });

    // Change revision as an unnamed requirement
    uv_snapshot!(context.filters(), context.add()
        .arg("--no-cache")
        .arg("git+https://github.com/astral-sh/test-lfs-repo")
        .arg("--rev").arg("07d2c9c75c8defeebb7c5a0c26cca54b777e3bec")
        .arg("--lfs"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@0fe88f7c2e2883521bf065c108d9ee8eb115674b#lfs=true)
     + test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@07d2c9c75c8defeebb7c5a0c26cca54b777e3bec#lfs=true)
    ");

    // Test LFS not found scenario resulting in an incomplete fetch cache

    // The filters below will remove any boilerplate before what we actually want to match.
    // They help handle slightly different output in uv-distribution/src/source/mod.rs between
    // calls to `git` and `git_metadata` functions which don't have guaranteed execution order.
    // In addition, we can get different error codes depending on where the failure occurs,
    // although we know the error code cannot be 0.
    let context = context
        .with_filter((r"exit_code: -?[1-9]\d*", "exit_code: [ERROR_CODE]"))
        .with_filter((
            "(?s)(----- stderr -----).*?The source distribution `[^`]+` is missing Git LFS artifacts.*",
            "$1\n[PREFIX]The source distribution `[DISTRIBUTION]` is missing Git LFS artifacts",
        ));

    uv_snapshot!(context.filters(), context.add()
        .env(EnvVars::UV_INTERNAL__TEST_LFS_DISABLED, "1")
        .arg("git+https://github.com/astral-sh/test-lfs-repo")
        .arg("--rev").arg("0fe88f7c2e2883521bf065c108d9ee8eb115674b")
        .arg("--lfs"), @"
    success: false
    exit_code: [ERROR_CODE]
    ----- stdout -----

    ----- stderr -----
    [PREFIX]The source distribution `[DISTRIBUTION]` is missing Git LFS artifacts
    ");

    // There should be no .ok entry as LFS operations failed
    assert!(!ok_checkout_file.exists(), "Found unexpected .ok file.");

    // Test LFS recovery from an incomplete fetch cache
    uv_snapshot!(context.filters(), context.add()
        .arg("git+https://github.com/astral-sh/test-lfs-repo")
        .arg("--rev").arg("0fe88f7c2e2883521bf065c108d9ee8eb115674b")
        .arg("--lfs"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@07d2c9c75c8defeebb7c5a0c26cca54b777e3bec#lfs=true)
     + test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@0fe88f7c2e2883521bf065c108d9ee8eb115674b#lfs=true)
    ");

    // Verify that we can import the module and access LFS content
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg("import test_lfs_repo.lfs_module"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // Now let's delete some of the LFS entries from our db...
    fs_err::remove_file(&ok_checkout_file)?;
    fs_err::remove_dir_all(&lfs_db_bucket_objects)?;

    // Test LFS recovery from an incomplete db and non-fresh checkout
    uv_snapshot!(context.filters(), context.add()
        .arg("git+https://github.com/astral-sh/test-lfs-repo")
        .arg("--rev").arg("0fe88f7c2e2883521bf065c108d9ee8eb115674b")
        .arg("--reinstall")
        .arg("--lfs"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@0fe88f7c2e2883521bf065c108d9ee8eb115674b#lfs=true)
    ");

    // Verify that we can import the module and access LFS content
    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg("import test_lfs_repo.lfs_module"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // Verify our db and checkout recovered
    assert!(ok_checkout_file.exists());
    assert!(lfs_db_bucket_objects.exists());

    // Exercise the sdist cache
    uv_snapshot!(context.filters(), context.add()
        .arg("git+https://github.com/astral-sh/test-lfs-repo")
        .arg("--rev").arg("0fe88f7c2e2883521bf065c108d9ee8eb115674b")
        .arg("--lfs"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    ");

    Ok(())
}

/// Add a Git requirement using the `--raw-sources` API.
#[test]
#[cfg(feature = "test-git")]
fn add_git_raw() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    // Use an ambiguous tag reference, which would otherwise not resolve.
    uv_snapshot!(context.filters(), context.add().arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@0.0.1").arg("--raw-sources"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio==3.7.0",
            "uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@0.0.1",
        ]
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737, upload-time = "2023-05-27T11:12:46.688Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873, upload-time = "2023-05-27T11:12:44.474Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "anyio" },
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", specifier = "==3.7.0" },
            { name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage?rev=0.0.1" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?rev=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 4 packages in [TIME]
    ");

    Ok(())
}

/// Add a Git requirement without the `git+` prefix.
#[test]
#[cfg(feature = "test-git")]
fn add_git_implicit() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    // Omit the `git+` prefix.
    uv_snapshot!(context.filters(), context.add().arg("uv-public-pypackage @ https://github.com/astral-test/uv-public-pypackage.git"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage.git@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
    ");

    Ok(())
}

/// `--raw-sources` should be considered conflicting with sources-specific arguments, like `--tag`.
#[test]
#[cfg(feature = "test-git")]
fn add_raw_error() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Provide a tag without a Git source.
    uv_snapshot!(context.filters(), context.add().arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage").arg("--tag").arg("0.0.1").arg("--raw-sources"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--tag <TAG>' cannot be used with '--raw'

    Usage: uv add --cache-dir [CACHE_DIR] --tag <TAG> --exclude-newer <EXCLUDE_NEWER> <PACKAGES|--requirements <REQUIREMENTS>>

    For more information, try '--help'.
    ");

    Ok(())
}

#[test]
fn reinstall_local_source_trees() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project_1 = context.temp_dir.child("project1");
    project_1.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project1"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    let project_2 = context.temp_dir.child("project2");
    project_2.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project2"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().current_dir(&project_1).arg("../project2").arg("--editable"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project2==0.1.0 (from file://[TEMP_DIR]/project2)
    ");

    // Running `uv add` should reinstall the project.
    uv_snapshot!(context.filters(), context.add().current_dir(&project_1).arg("../project2").arg("--editable"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ project2==0.1.0 (from file://[TEMP_DIR]/project2)
    ");

    Ok(())
}

#[test]
#[cfg(feature = "test-git")]
fn add_editable_error() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Provide `--editable` with a non-source tree.
    uv_snapshot!(context.filters(), context.add().arg("flask @ https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl").arg("--editable"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `flask` did not resolve to a local directory, but the `--editable` flag was provided. Editable installs are only supported for local directories.
    ");

    Ok(())
}

/// Add an unnamed requirement.
#[test]
#[cfg(feature = "test-git")]
fn add_unnamed() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("git+https://github.com/astral-test/uv-public-pypackage").arg("--tag=0.0.1"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "uv-public-pypackage",
        ]

        [tool.uv.sources]
        uv-public-pypackage = { git = "https://github.com/astral-test/uv-public-pypackage", tag = "0.0.1" }
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage?tag=0.0.1" }]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?tag=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    ");

    Ok(())
}

/// Add and remove a development dependency.
#[test]
fn add_remove_dev() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0").arg("--dev"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = [
            "anyio==3.7.0",
        ]
        "#
        );
    });

    // `uv add` implies a full lock and sync, including development dependencies.
    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737, upload-time = "2023-05-27T11:12:46.688Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873, upload-time = "2023-05-27T11:12:44.474Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.dev-dependencies]
        dev = [
            { name = "anyio" },
        ]

        [package.metadata]

        [package.metadata.requires-dev]
        dev = [{ name = "anyio", specifier = "==3.7.0" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 3 packages in [TIME]
    ");

    // This should fail without --dev.
    uv_snapshot!(context.filters(), context.remove().arg("anyio"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    hint: `anyio` is in the `dev` group (try: `uv remove anyio --group dev`)
    error: The dependency `anyio` could not be found in `project.dependencies`
    ");

    // Remove the dependency.
    uv_snapshot!(context.filters(), context.remove().arg("anyio").arg("--dev"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Uninstalled 3 packages in [TIME]
     - anyio==3.7.0
     - idna==3.6
     - sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = []
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.metadata]

        [package.metadata.requires-dev]
        dev = []
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited in [TIME]
    ");

    Ok(())
}

/// Add and remove an optional dependency.
#[test]
fn add_remove_optional() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0").arg("--optional=io"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        io = [
            "anyio==3.7.0",
        ]
        "#
        );
    });

    // `uv add` implies a full lock and sync, including development dependencies.
    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737, upload-time = "2023-05-27T11:12:46.688Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873, upload-time = "2023-05-27T11:12:44.474Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        io = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", marker = "extra == 'io'", specifier = "==3.7.0" }]
        provides-extras = ["io"]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]
        "#
        );
    });

    // Install from the lockfile. At present, this will _uninstall_ the packages since `sync` does
    // not include extras by default.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 3 packages in [TIME]
     - anyio==3.7.0
     - idna==3.6
     - sniffio==1.3.1
    ");

    // This should fail without --optional.
    uv_snapshot!(context.filters(), context.remove().arg("anyio"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    hint: `anyio` is an optional dependency (try: `uv remove anyio --optional io`)
    error: The dependency `anyio` could not be found in `project.dependencies`
    ");

    // Remove the dependency.
    uv_snapshot!(context.filters(), context.remove().arg("anyio").arg("--optional=io"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        io = []
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.metadata]
        provides-extras = ["io"]
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited in [TIME]
    ");

    Ok(())
}

#[test]
fn add_remove_inline_optional() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        optional-dependencies = { io = [
            "anyio==3.7.0",
        ] }
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("typing-extensions").arg("--optional=types"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        optional-dependencies = { io = [
            "anyio==3.7.0",
        ], types = [
            "typing-extensions>=4.10.0",
        ] }
        "#
        );
    });

    uv_snapshot!(context.filters(), context.remove().arg("typing-extensions").arg("--optional=types"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Uninstalled 1 package in [TIME]
     - typing-extensions==4.10.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        optional-dependencies = { io = [
            "anyio==3.7.0",
        ], types = [] }
        "#
        );
    });

    Ok(())
}

/// Add and remove a workspace dependency.
#[test]
#[cfg(feature = "test-git")]
fn add_remove_workspace() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let workspace = context.temp_dir.child("pyproject.toml");
    workspace.write_str(indoc! {r#"
        [tool.uv.workspace]
        members = ["child1", "child2"]
    "#})?;

    let pyproject_toml = context.temp_dir.child("child1/pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child1"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    context
        .temp_dir
        .child("child1")
        .child("src")
        .child("child1")
        .child("__init__.py")
        .touch()?;

    let pyproject_toml = context.temp_dir.child("child2/pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child2"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    context
        .temp_dir
        .child("child2")
        .child("src")
        .child("child2")
        .child("__init__.py")
        .touch()?;

    // Adding a workspace package with a mismatched source should error.
    let mut add_cmd = context.add();
    add_cmd
        .arg("child2 @ git+https://github.com/astral-test/uv-public-pypackage")
        .arg("--package")
        .arg("child1")
        .current_dir(&context.temp_dir);

    uv_snapshot!(context.filters(), add_cmd, @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Workspace dependency `child2` must refer to local directory, not a Git repository
    ");

    // Workspace packages should be detected automatically.
    let child1 = context.temp_dir.join("child1");
    let mut add_cmd = context.add();
    add_cmd
        .arg("child2")
        .arg("--package")
        .arg("child1")
        .current_dir(&context.temp_dir);

    uv_snapshot!(context.filters(), add_cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child1==0.1.0 (from file://[TEMP_DIR]/child1)
     + child2==0.1.0 (from file://[TEMP_DIR]/child2)
    ");

    let pyproject_toml = fs_err::read_to_string(child1.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "child1"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "child2",
        ]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv.sources]
        child2 = { workspace = true }
        "#
        );
    });

    // `uv add` implies a full lock and sync, including development dependencies.
    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "child1",
            "child2",
        ]

        [[package]]
        name = "child1"
        version = "0.1.0"
        source = { editable = "child1" }
        dependencies = [
            { name = "child2" },
        ]

        [package.metadata]
        requires-dist = [{ name = "child2", editable = "child2" }]

        [[package]]
        name = "child2"
        version = "0.1.0"
        source = { editable = "child2" }
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").current_dir(&child1), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 2 packages in [TIME]
    ");

    // Remove the dependency.
    uv_snapshot!(context.filters(), context.remove().arg("child2").current_dir(&child1), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 1 package in [TIME]
     ~ child1==0.1.0 (from file://[TEMP_DIR]/child1)
     - child2==0.1.0 (from file://[TEMP_DIR]/child2)
    ");

    let pyproject_toml = fs_err::read_to_string(child1.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "child1"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "child1",
            "child2",
        ]

        [[package]]
        name = "child1"
        version = "0.1.0"
        source = { editable = "child1" }

        [[package]]
        name = "child2"
        version = "0.1.0"
        source = { editable = "child2" }
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").current_dir(&child1), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    ");

    Ok(())
}

/// `uv add --dev` should update `dev-dependencies` (rather than `dependency-groups.dev`) if a
/// dependency already exists in `dev-dependencies`.
#[test]
fn update_existing_dev() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = ["anyio"]

        [dependency-groups]
        dev = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0").arg("--dev"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `tool.uv.dev-dependencies` field (used in `pyproject.toml`) is deprecated and will be removed in a future release; use `dependency-groups.dev` instead
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = [
            "anyio==3.7.0",
        ]

        [dependency-groups]
        dev = []
        "#
        );
    });

    Ok(())
}

/// `uv add --dev` should add to `dev-dependencies` (rather than `dependency-groups.dev`) if it
/// exists.
#[test]
fn add_existing_dev() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0").arg("--dev"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `tool.uv.dev-dependencies` field (used in `pyproject.toml`) is deprecated and will be removed in a future release; use `dependency-groups.dev` instead
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = [
            "anyio==3.7.0",
        ]
        "#
        );
    });

    Ok(())
}

/// `uv add --group dev` should update `dev-dependencies` (rather than `dependency-groups.dev`) if a
/// dependency already exists.
#[test]
fn update_existing_dev_group() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = ["anyio"]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0").arg("--group").arg("dev"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `tool.uv.dev-dependencies` field (used in `pyproject.toml`) is deprecated and will be removed in a future release; use `dependency-groups.dev` instead
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = [
            "anyio==3.7.0",
        ]
        "#
        );
    });

    Ok(())
}

/// `uv add --group dev` should add to `dependency-groups` even if `dev-dependencies` exists.
#[test]
fn add_existing_dev_group() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0").arg("--group").arg("dev"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `tool.uv.dev-dependencies` field (used in `pyproject.toml`) is deprecated and will be removed in a future release; use `dependency-groups.dev` instead
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = []

        [dependency-groups]
        dev = [
            "anyio==3.7.0",
        ]
        "#
        );
    });

    Ok(())
}

/// `uv remove --dev` should remove from both `dev-dependencies` and `dependency-groups.dev`.
#[test]
fn remove_both_dev() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = ["anyio"]

        [dependency-groups]
        dev = ["anyio>=3.7.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.remove().arg("anyio").arg("--dev"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `tool.uv.dev-dependencies` field (used in `pyproject.toml`) is deprecated and will be removed in a future release; use `dependency-groups.dev` instead
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = []

        [dependency-groups]
        dev = []
        "#
        );
    });

    Ok(())
}

/// Do not allow add for groups in scripts.
#[test]
fn disallow_group_script_add() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("main.py");
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.13"
        # dependencies = []
        #
        # ///
    "#})?;

    uv_snapshot!(context.filters(), context
        .add()
        .arg("--group")
        .arg("dev")
        .arg("anyio==3.7.0")
        .arg("--script")
        .arg("main.py"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--group <GROUP>' cannot be used with '--script <SCRIPT>'

    Usage: uv add --cache-dir [CACHE_DIR] --group <GROUP> --exclude-newer <EXCLUDE_NEWER> <PACKAGES|--requirements <REQUIREMENTS>>

    For more information, try '--help'.
    ");

    Ok(())
}

/// `uv remove --group dev` should remove from both `dev-dependencies` and `dependency-groups.dev`.
#[test]
fn remove_both_dev_group() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = ["anyio"]

        [dependency-groups]
        dev = ["anyio>=3.7.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.remove().arg("anyio").arg("--group").arg("dev"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `tool.uv.dev-dependencies` field (used in `pyproject.toml`) is deprecated and will be removed in a future release; use `dependency-groups.dev` instead
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = []

        [dependency-groups]
        dev = []
        "#
        );
    });

    Ok(())
}

/// Add a workspace dependency as an editable.
#[test]
fn add_workspace_editable() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let workspace = context.temp_dir.child("pyproject.toml");
    workspace.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv.workspace]
        members = ["child1", "child2"]
    "#})?;

    let pyproject_toml = context.temp_dir.child("child1/pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child1"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    context
        .temp_dir
        .child("child1")
        .child("src")
        .child("child1")
        .child("__init__.py")
        .touch()?;

    let pyproject_toml = context.temp_dir.child("child2/pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child2"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    context
        .temp_dir
        .child("child2")
        .child("src")
        .child("child2")
        .child("__init__.py")
        .touch()?;

    let child1 = context.temp_dir.join("child1");

    // `--no-editable` should add `editable = false`.
    let mut add_cmd = context.add();
    add_cmd
        .arg("child2")
        .arg("--no-editable")
        .current_dir(&child1);

    uv_snapshot!(context.filters(), add_cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child1==0.1.0 (from file://[TEMP_DIR]/child1)
     + child2==0.1.0 (from file://[TEMP_DIR]/child2)
    ");

    let pyproject_toml = fs_err::read_to_string(child1.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "child1"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "child2",
        ]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv.sources]
        child2 = { workspace = true, editable = false }
        "#
        );
    });

    // `--editable` should not.
    let mut add_cmd = context.add();
    add_cmd.arg("child2").arg("--editable").current_dir(&child1);

    uv_snapshot!(context.filters(), add_cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     ~ child1==0.1.0 (from file://[TEMP_DIR]/child1)
     ~ child2==0.1.0 (from file://[TEMP_DIR]/child2)
    ");

    let pyproject_toml = fs_err::read_to_string(child1.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "child1"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "child2",
        ]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv.sources]
        child2 = { workspace = true, editable = true }
        "#
        );
    });

    // `uv add` implies a full lock and sync, including development dependencies.
    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "child1",
            "child2",
            "parent",
        ]

        [[package]]
        name = "child1"
        version = "0.1.0"
        source = { editable = "child1" }
        dependencies = [
            { name = "child2" },
        ]

        [package.metadata]
        requires-dist = [{ name = "child2", editable = "child2" }]

        [[package]]
        name = "child2"
        version = "0.1.0"
        source = { editable = "child2" }

        [[package]]
        name = "parent"
        version = "0.1.0"
        source = { virtual = "." }
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").current_dir(&child1), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 2 packages in [TIME]
    ");

    Ok(())
}

/// Add a workspace dependency via its path.
#[test]
fn add_workspace_path() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let workspace = context.temp_dir.child("pyproject.toml");
    workspace.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv.workspace]
        members = ["child"]
    "#})?;

    let pyproject_toml = context.temp_dir.child("child/pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    context
        .temp_dir
        .child("child")
        .child("src")
        .child("child")
        .child("__init__.py")
        .touch()?;

    uv_snapshot!(context.filters(), context.add().arg("./child"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "child",
        ]

        [tool.uv.workspace]
        members = ["child"]

        [tool.uv.sources]
        child = { workspace = true }
        "#
        );
    });

    // `uv add` implies a full lock and sync, including development dependencies.
    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "child",
            "parent",
        ]

        [[package]]
        name = "child"
        version = "0.1.0"
        source = { editable = "child" }

        [[package]]
        name = "parent"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "child" },
        ]

        [package.metadata]
        requires-dist = [{ name = "child", editable = "child" }]
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    ");

    Ok(())
}

/// Add a path dependency, which should be implicitly added to the workspace.
#[test]
fn add_path_implicit_workspace() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let workspace = context.temp_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    let child = workspace.child("packages").child("child");
    child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    workspace
        .child("packages")
        .child("child")
        .child("src")
        .child("child")
        .child("__init__.py")
        .touch()?;

    uv_snapshot!(context.filters(), context.add().arg(Path::new("packages").join("child")).current_dir(workspace.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Added `packages/child` to workspace members
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/workspace/packages/child)
    ");

    let pyproject_toml = fs_err::read_to_string(workspace.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "child",
        ]

        [tool.uv.workspace]
        members = [
            "packages/child",
        ]

        [tool.uv.sources]
        child = { workspace = true }
        "#
        );
    });

    // `uv add` implies a full lock and sync, including development dependencies.
    let lock = fs_err::read_to_string(workspace.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "child",
            "parent",
        ]

        [[package]]
        name = "child"
        version = "0.1.0"
        source = { editable = "packages/child" }

        [[package]]
        name = "parent"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "child" },
        ]

        [package.metadata]
        requires-dist = [{ name = "child", editable = "packages/child" }]
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").current_dir(workspace.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    ");

    Ok(())
}

/// Add a path dependency with `--no-workspace`, which should not be added to the workspace.
#[test]
fn add_path_no_workspace() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let workspace = context.temp_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    let child = workspace.child("packages").child("child");
    child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    workspace
        .child("packages")
        .child("child")
        .child("src")
        .child("child")
        .child("__init__.py")
        .touch()?;

    uv_snapshot!(context.filters(), context.add().arg(Path::new("packages").join("child")).current_dir(workspace.path()).arg("--no-workspace"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/workspace/packages/child)
    ");

    let pyproject_toml = fs_err::read_to_string(workspace.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "child",
        ]

        [tool.uv.sources]
        child = { path = "packages/child" }
        "#
        );
    });

    // `uv add` implies a full lock and sync, including development dependencies.
    let lock = fs_err::read_to_string(workspace.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "child"
        version = "0.1.0"
        source = { directory = "packages/child" }

        [[package]]
        name = "parent"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "child" },
        ]

        [package.metadata]
        requires-dist = [{ name = "child", directory = "packages/child" }]
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").current_dir(workspace.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    ");

    Ok(())
}

/// Add a path dependency in an adjacent directory, which should not be added to the workspace.
#[test]
fn add_path_adjacent_directory() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    let dependency = context.temp_dir.child("dependency");
    dependency.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "dependency"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    dependency
        .child("src")
        .child("dependency")
        .child("__init__.py")
        .touch()?;

    uv_snapshot!(context.filters(), context.add().arg(dependency.path()).current_dir(project.path()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dependency==0.1.0 (from file://[TEMP_DIR]/dependency)
    ");

    let pyproject_toml = fs_err::read_to_string(project.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "dependency",
        ]

        [tool.uv.sources]
        dependency = { path = "../dependency" }
        "#
        );
    });

    // `uv add` implies a full lock and sync, including development dependencies.
    let lock = fs_err::read_to_string(project.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dependency"
        version = "0.1.0"
        source = { directory = "../dependency" }

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dependency" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dependency", directory = "../dependency" }]
        "#
        );
    });

    Ok(())
}

/// Update a requirement, modifying the source and extras.
#[test]
#[cfg(feature = "test-git")]
fn update() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["requests==2.31.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    ");

    // Enable an extra (note the version specifier should be preserved).
    uv_snapshot!(context.filters(), context.add().arg("requests[security]"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Audited 5 packages in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "requests[security]==2.31.0",
        ]
        "#
        );
    });

    // Enable extras using the CLI flag and add a marker.
    uv_snapshot!(context.filters(), context.add().arg("requests; python_version > '3.7'").args(["--extra=use_chardet_on_py3", "--extra=socks"]), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + chardet==5.2.0
     + pysocks==1.7.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "requests[security]==2.31.0",
            "requests[socks,use-chardet-on-py3]>=2.31.0 ; python_full_version >= '3.8'",
        ]
        "#
        );
    });

    // Change the source by specifying a version (note the extras, markers, and version should be
    // preserved).
    uv_snapshot!(context.filters(), context.add().arg("requests @ git+https://github.com/psf/requests").arg("--tag=v2.32.3"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - requests==2.31.0
     + requests==2.32.3 (from git+https://github.com/psf/requests@0e322af87745eff34caffe4df68456ebc20d9068)
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "requests[security]==2.31.0",
            "requests[socks,use-chardet-on-py3]>=2.31.0 ; python_full_version >= '3.8'",
        ]

        [tool.uv.sources]
        requests = { git = "https://github.com/psf/requests", tag = "v2.32.3" }
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "certifi"
        version = "2024.2.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/71/da/e94e26401b62acd6d91df2b52954aceb7f561743aa5ccc32152886c76c96/certifi-2024.2.2.tar.gz", hash = "sha256:0569859f95fc761b18b45ef421b1290a0f65f147e92a1e5eb3e635f9a5e4e66f", size = 164886, upload-time = "2024-02-02T01:22:17.364Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ba/06/a07f096c664aeb9f01624f858c3add0a4e913d6c96257acb4fce61e7de14/certifi-2024.2.2-py3-none-any.whl", hash = "sha256:dc383c07b76109f368f6106eee2b593b04a011ea4d55f652c6ca24a754d1cdd1", size = 163774, upload-time = "2024-02-02T01:22:14.86Z" },
        ]

        [[package]]
        name = "chardet"
        version = "5.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/f3/0d/f7b6ab21ec75897ed80c17d79b15951a719226b9fababf1e40ea74d69079/chardet-5.2.0.tar.gz", hash = "sha256:1b3b6ff479a8c414bc3fa2c0852995695c4a026dcd6d0633b2dd092ca39c1cf7", size = 2069618, upload-time = "2023-08-01T19:23:02.662Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/6f/f5fbc992a329ee4e0f288c1fe0e2ad9485ed064cac731ed2fe47dcc38cbf/chardet-5.2.0-py3-none-any.whl", hash = "sha256:e1cf59446890a00105fe7b7912492ea04b6e6f06d4b742b2c788469e34c82970", size = 199385, upload-time = "2023-08-01T19:23:00.661Z" },
        ]

        [[package]]
        name = "charset-normalizer"
        version = "3.3.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/63/09/c1bc53dab74b1816a00d8d030de5bf98f724c52c1635e07681d312f20be8/charset-normalizer-3.3.2.tar.gz", hash = "sha256:f30c3cb33b24454a82faecaf01b19c18562b1e89558fb6c56de4d9118a032fd5", size = 104809, upload-time = "2023-11-01T04:04:59.997Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/b2/fcedc8255ec42afee97f9e6f0145c734bbe104aac28300214593eb326f1d/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:0b2b64d2bb6d3fb9112bafa732def486049e63de9618b5843bcdd081d8144cd8", size = 192892, upload-time = "2023-11-01T04:03:24.135Z" },
            { url = "https://files.pythonhosted.org/packages/2e/7d/2259318c202f3d17f3fe6438149b3b9e706d1070fe3fcbb28049730bb25c/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:ddbb2551d7e0102e7252db79ba445cdab71b26640817ab1e3e3648dad515003b", size = 122213, upload-time = "2023-11-01T04:03:25.66Z" },
            { url = "https://files.pythonhosted.org/packages/3a/52/9f9d17c3b54dc238de384c4cb5a2ef0e27985b42a0e5cc8e8a31d918d48d/charset_normalizer-3.3.2-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:55086ee1064215781fff39a1af09518bc9255b50d6333f2e4c74ca09fac6a8f6", size = 119404, upload-time = "2023-11-01T04:03:27.04Z" },
            { url = "https://files.pythonhosted.org/packages/99/b0/9c365f6d79a9f0f3c379ddb40a256a67aa69c59609608fe7feb6235896e1/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:8f4a014bc36d3c57402e2977dada34f9c12300af536839dc38c0beab8878f38a", size = 137275, upload-time = "2023-11-01T04:03:28.466Z" },
            { url = "https://files.pythonhosted.org/packages/91/33/749df346e93d7a30cdcb90cbfdd41a06026317bfbfb62cd68307c1a3c543/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:a10af20b82360ab00827f916a6058451b723b4e65030c5a18577c8b2de5b3389", size = 147518, upload-time = "2023-11-01T04:03:29.82Z" },
            { url = "https://files.pythonhosted.org/packages/72/1a/641d5c9f59e6af4c7b53da463d07600a695b9824e20849cb6eea8a627761/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:8d756e44e94489e49571086ef83b2bb8ce311e730092d2c34ca8f7d925cb20aa", size = 140182, upload-time = "2023-11-01T04:03:31.511Z" },
            { url = "https://files.pythonhosted.org/packages/ee/fb/14d30eb4956408ee3ae09ad34299131fb383c47df355ddb428a7331cfa1e/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:90d558489962fd4918143277a773316e56c72da56ec7aa3dc3dbbe20fdfed15b", size = 141869, upload-time = "2023-11-01T04:03:32.887Z" },
            { url = "https://files.pythonhosted.org/packages/df/3e/a06b18788ca2eb6695c9b22325b6fde7dde0f1d1838b1792a0076f58fe9d/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:6ac7ffc7ad6d040517be39eb591cac5ff87416c2537df6ba3cba3bae290c0fed", size = 144042, upload-time = "2023-11-01T04:03:34.412Z" },
            { url = "https://files.pythonhosted.org/packages/45/59/3d27019d3b447a88fe7e7d004a1e04be220227760264cc41b405e863891b/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:7ed9e526742851e8d5cc9e6cf41427dfc6068d4f5a3bb03659444b4cabf6bc26", size = 138275, upload-time = "2023-11-01T04:03:35.759Z" },
            { url = "https://files.pythonhosted.org/packages/7b/ef/5eb105530b4da8ae37d506ccfa25057961b7b63d581def6f99165ea89c7e/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:8bdb58ff7ba23002a4c5808d608e4e6c687175724f54a5dade5fa8c67b604e4d", size = 144819, upload-time = "2023-11-01T04:03:37.216Z" },
            { url = "https://files.pythonhosted.org/packages/a2/51/e5023f937d7f307c948ed3e5c29c4b7a3e42ed2ee0b8cdf8f3a706089bf0/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_ppc64le.whl", hash = "sha256:6b3251890fff30ee142c44144871185dbe13b11bab478a88887a639655be1068", size = 149415, upload-time = "2023-11-01T04:03:38.694Z" },
            { url = "https://files.pythonhosted.org/packages/24/9d/2e3ef673dfd5be0154b20363c5cdcc5606f35666544381bee15af3778239/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_s390x.whl", hash = "sha256:b4a23f61ce87adf89be746c8a8974fe1c823c891d8f86eb218bb957c924bb143", size = 141212, upload-time = "2023-11-01T04:03:40.07Z" },
            { url = "https://files.pythonhosted.org/packages/5b/ae/ce2c12fcac59cb3860b2e2d76dc405253a4475436b1861d95fe75bdea520/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:efcb3f6676480691518c177e3b465bcddf57cea040302f9f4e6e191af91174d4", size = 142167, upload-time = "2023-11-01T04:03:41.491Z" },
            { url = "https://files.pythonhosted.org/packages/ed/3a/a448bf035dce5da359daf9ae8a16b8a39623cc395a2ffb1620aa1bce62b0/charset_normalizer-3.3.2-cp312-cp312-win32.whl", hash = "sha256:d965bba47ddeec8cd560687584e88cf699fd28f192ceb452d1d7ee807c5597b7", size = 93041, upload-time = "2023-11-01T04:03:42.836Z" },
            { url = "https://files.pythonhosted.org/packages/b6/7c/8debebb4f90174074b827c63242c23851bdf00a532489fba57fef3416e40/charset_normalizer-3.3.2-cp312-cp312-win_amd64.whl", hash = "sha256:96b02a3dc4381e5494fad39be677abcb5e6634bf7b4fa83a6dd3112607547001", size = 100397, upload-time = "2023-11-01T04:03:44.467Z" },
            { url = "https://files.pythonhosted.org/packages/28/76/e6222113b83e3622caa4bb41032d0b1bf785250607392e1b778aca0b8a7d/charset_normalizer-3.3.2-py3-none-any.whl", hash = "sha256:3e4d1f6587322d2788836a99c69062fbb091331ec940e02d12d179c1d53e25fc", size = 48543, upload-time = "2023-11-01T04:04:58.622Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "requests", extra = ["socks", "use-chardet-on-py3"] },
        ]

        [package.metadata]
        requires-dist = [
            { name = "requests", extras = ["security"], git = "https://github.com/psf/requests?tag=v2.32.3" },
            { name = "requests", extras = ["socks", "use-chardet-on-py3"], marker = "python_full_version >= '3.8'", git = "https://github.com/psf/requests?tag=v2.32.3" },
        ]

        [[package]]
        name = "pysocks"
        version = "1.7.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bd/11/293dd436aea955d45fc4e8a35b6ae7270f5b8e00b53cf6c024c83b657a11/PySocks-1.7.1.tar.gz", hash = "sha256:3f8804571ebe159c380ac6de37643bb4685970655d3bba243530d6558b799aa0", size = 284429, upload-time = "2019-09-20T02:07:35.714Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/8d/59/b4572118e098ac8e46e399a1dd0f2d85403ce8bbaad9ec79373ed6badaf9/PySocks-1.7.1-py3-none-any.whl", hash = "sha256:2725bd0a9925919b9b51739eea5f9e2bae91e83288108a9ad338b2e3a4435ee5", size = 16725, upload-time = "2019-09-20T02:06:22.938Z" },
        ]

        [[package]]
        name = "requests"
        version = "2.32.3"
        source = { git = "https://github.com/psf/requests?tag=v2.32.3#0e322af87745eff34caffe4df68456ebc20d9068" }
        dependencies = [
            { name = "certifi" },
            { name = "charset-normalizer" },
            { name = "idna" },
            { name = "urllib3" },
        ]

        [package.optional-dependencies]
        socks = [
            { name = "pysocks" },
        ]
        use-chardet-on-py3 = [
            { name = "chardet" },
        ]

        [[package]]
        name = "urllib3"
        version = "2.2.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7a/50/7fd50a27caa0652cd4caf224aa87741ea41d3265ad13f010886167cfcc79/urllib3-2.2.1.tar.gz", hash = "sha256:d0570876c61ab9e520d776c38acbbb5b05a776d3f9ff98a5c8fd5162a444cf19", size = 291020, upload-time = "2024-02-18T03:55:57.539Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a2/73/a68704750a7679d0b6d3ad7aa8d4da8e14e151ae82e6fee774e6e0d05ec8/urllib3-2.2.1-py3-none-any.whl", hash = "sha256:450b20ec296a467077128bff42b73080516e71b56ff59a60a02bef2232c4fa9d", size = 121067, upload-time = "2024-02-18T03:55:54.704Z" },
        ]
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 7 packages in [TIME]
    ");

    Ok(())
}

/// Add and update a requirement, with different markers
#[test]
fn add_update_marker() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = ["requests>=2.30; python_version >= '3.11'"]
    "#})?;
    uv_snapshot!(context.filters(), context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + urllib3==2.2.1
    ");

    // Restrict the `requests` version for Python <3.11
    uv_snapshot!(context.filters(), context.add().arg("requests>=2.0,<2.29; python_version < '3.11'"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Audited 5 packages in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    // Should add a new line for the dependency since the marker does not match an existing one
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = [
            "requests>=2.0,<2.29 ; python_full_version < '3.11'",
            "requests>=2.30; python_version >= '3.11'",
        ]
        "#
        );
    });

    // Change the restricted `requests` version for Python <3.11
    uv_snapshot!(context.filters(), context.add().arg("requests>=2.0,<2.20; python_version < '3.11'"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Audited 5 packages in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    // Should mutate the existing dependency since the marker matches
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = [
            "requests>=2.0,<2.20 ; python_full_version < '3.11'",
            "requests>=2.30; python_version >= '3.11'",
        ]
        "#
        );
    });

    // Restrict the `requests` version on Windows and Python >3.11
    uv_snapshot!(context.filters(), context.add().arg("requests>=2.31 ; sys_platform == 'win32' and python_version > '3.11'"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Audited 5 packages in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    // Should add a new line for the dependency since the marker does not match an existing one
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = [
            "requests>=2.0,<2.20 ; python_full_version < '3.11'",
            "requests>=2.30; python_version >= '3.11'",
            "requests>=2.31 ; python_full_version >= '3.12' and sys_platform == 'win32'",
        ]
        "#
        );
    });

    // Restrict the `requests` version on Windows
    uv_snapshot!(context.filters(), context.add().arg("requests>=2.10 ; sys_platform == 'win32'"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    Audited 5 packages in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    // Should add a new line for the dependency since the marker does not exactly match an existing
    // one — although it is a subset of the existing marker.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = [
            "requests>=2.0,<2.20 ; python_full_version < '3.11'",
            "requests>=2.10 ; sys_platform == 'win32'",
            "requests>=2.30; python_version >= '3.11'",
            "requests>=2.31 ; python_full_version >= '3.12' and sys_platform == 'win32'",
        ]
        "#
        );
    });

    // Remove `requests`
    uv_snapshot!(context.filters(), context.remove().arg("requests"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Uninstalled 5 packages in [TIME]
     - certifi==2024.2.2
     - charset-normalizer==3.3.2
     - idna==3.6
     - requests==2.31.0
     - urllib3==2.2.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    // Should remove all variants of `requests`
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = []
        "#
        );
    });

    Ok(())
}

#[test]
#[cfg(feature = "test-git")]
fn update_source_replace_url() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "requests[security] @ https://files.pythonhosted.org/packages/f9/9b/335f9764261e915ed497fcdeb11df5dfd6f7bf257d4a6a2a686d80da4d54/requests-2.32.3-py3-none-any.whl"
        ]
    "#})?;

    // Change the source. The existing URL should be removed.
    uv_snapshot!(context.filters(), context.add().arg("requests @ git+https://github.com/psf/requests").arg("--tag=v2.32.3"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.32.3 (from git+https://github.com/psf/requests@0e322af87745eff34caffe4df68456ebc20d9068)
     + urllib3==2.2.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "requests[security]",
        ]

        [tool.uv.sources]
        requests = { git = "https://github.com/psf/requests", tag = "v2.32.3" }
        "#
        );
    });

    // Change the source again. The existing source should be replaced.
    uv_snapshot!(context.filters(), context.add().arg("requests @ git+https://github.com/psf/requests").arg("--tag=v2.32.2"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - requests==2.32.3 (from git+https://github.com/psf/requests@0e322af87745eff34caffe4df68456ebc20d9068)
     + requests==2.32.2 (from git+https://github.com/psf/requests@88dce9d854797c05d0ff296b70e0430535ef8aaf)
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "requests[security]",
        ]

        [tool.uv.sources]
        requests = { git = "https://github.com/psf/requests", tag = "v2.32.2" }
        "#
        );
    });

    Ok(())
}

/// If a source defined in `tool.uv.sources` but its name is not normalized, `uv add` should not
/// add the same source again.
#[test]
#[cfg(feature = "test-git")]
fn add_non_normalized_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "uv-public-pypackage"
        ]

        [tool.uv.sources]
        uv_public_pypackage = { git = "https://github.com/astral-test/uv-public-pypackage", tag = "0.0.1" }
        "#})?;

    uv_snapshot!(context.filters(), context.add().arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@0.0.1"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "uv-public-pypackage",
        ]

        [tool.uv.sources]
        uv-public-pypackage = { git = "https://github.com/astral-test/uv-public-pypackage", rev = "0.0.1" }
        "#
        );
    });

    Ok(())
}

/// Test updating an existing Git reference with branch/tag/rev options without re- specifying the
/// URL.
#[test]
#[cfg(feature = "test-git")]
fn add_update_git_reference_project() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("https://github.com/astral-test/uv-public-pypackage.git"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage.git@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
    ");

    uv_snapshot!(context.filters(), context.add().arg("uv-public-pypackage").arg("--tag=0.0.1"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage.git@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage.git@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    ");

    uv_snapshot!(context.filters(), context.add().arg("uv-public-pypackage").arg("--branch=main"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage.git@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage.git@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
    ");

    uv_snapshot!(context.filters(), context.add().arg("uv-public-pypackage").arg("--rev=2005223fcad0e2c06daf2e14b93b790604868e1e"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage.git@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage.git@2005223fcad0e2c06daf2e14b93b790604868e1e)
    ");

    Ok(())
}

/// Test updating an existing Git reference with branch/tag/rev options without re-specifying the
/// URL in a script.
#[test]
#[cfg(feature = "test-git")]
fn add_update_git_reference_script() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {
        r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [ ]
        # ///

        import time
        time.sleep(5)
        "#
    })?;

    uv_snapshot!(context.filters(), context.add().arg("--script=script.py").arg("https://github.com/astral-test/uv-public-pypackage.git"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    "
    );

    let script_content = context.read("script.py");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #  "uv-public-pypackage",
        # ]
        #
        # [tool.uv.sources]
        # uv-public-pypackage = { git = "https://github.com/astral-test/uv-public-pypackage.git" }
        # ///

        import time
        time.sleep(5)
        "#
        );
    });

    uv_snapshot!(context.filters(), context.add().arg("--script=script.py").arg("uv-public-pypackage").arg("--branch=test-branch"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    "
    );

    let script_content = context.read("script.py");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #  "uv-public-pypackage",
        # ]
        #
        # [tool.uv.sources]
        # uv-public-pypackage = { git = "https://github.com/astral-test/uv-public-pypackage.git", branch = "test-branch" }
        # ///

        import time
        time.sleep(5)
        "#
        );
    });

    Ok(())
}

/// If a source defined in `tool.uv.sources` but its name is not normalized, `uv remove` should
/// remove the source.
#[test]
#[cfg(feature = "test-git")]
fn remove_non_normalized_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "uv-public-pypackage"
        ]

        [tool.uv.sources]
        uv_public_pypackage = { git = "https://github.com/astral-test/uv-public-pypackage", tag = "0.0.1" }
        "#})?;

    uv_snapshot!(context.filters(), context.remove().arg("uv-public-pypackage"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        "#
        );
    });

    Ok(())
}

/// Adding a dependency does not remove untracked dependencies from the environment.
#[test]
fn add_inexact() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio == 3.7.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    // Manually remove a dependency.
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("iniconfig==2.0.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig==2.0.0",
        ]
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = "==2.0.0" }]
        "#
        );
    });

    // Install from the lockfile without removing extraneous packages from the environment.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--inexact"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    ");

    // Install from the lockfile, performing an exact sync.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 3 packages in [TIME]
     - anyio==3.7.0
     - idna==3.6
     - sniffio==1.3.1
    ");

    Ok(())
}

/// Remove a PyPI requirement.
#[test]
fn remove_registry() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    uv_snapshot!(context.filters(), context.remove().arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Uninstalled 3 packages in [TIME]
     - anyio==3.7.0
     - idna==3.6
     - sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited in [TIME]
    ");

    Ok(())
}

#[test]
fn add_preserves_indentation_in_pyproject_toml() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("requests==2.31.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + anyio==3.7.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + sniffio==1.3.1
     + urllib3==2.2.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio==3.7.0",
            "requests==2.31.0",
        ]
        "#
        );
    });
    Ok(())
}

#[test]
fn add_puts_default_indentation_in_pyproject_toml_if_not_observed() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("requests==2.31.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + anyio==3.7.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + sniffio==1.3.1
     + urllib3==2.2.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio==3.7.0",
            "requests==2.31.0",
        ]
        "#
        );
    });
    Ok(())
}

/// Add a requirement without updating the lockfile.
#[test]
fn add_frozen() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Remove the virtual environment.
    fs_err::remove_dir_all(&context.venv)?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0").arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio==3.7.0",
        ]
        "#
        );
    });

    assert!(!context.temp_dir.join("uv.lock").exists());
    assert!(!context.venv.exists());

    Ok(())
}

/// Add a requirement without updating the environment.
#[test]
fn add_no_sync() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Remove the virtual environment.
    fs_err::remove_dir_all(&context.venv)?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0").arg("--no-sync"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio==3.7.0",
        ]
        "#
        );
    });

    assert!(context.temp_dir.join("uv.lock").exists());
    assert!(!context.venv.exists());

    Ok(())
}

#[test]
fn add_reject_multiple_git_ref_flags() {
    let context = uv_test::test_context!("3.12");

    // --tag and --branch
    uv_snapshot!(context.filters(), context
        .add()
        .arg("foo")
        .arg("--tag")
        .arg("0.0.1")
        .arg("--branch")
        .arg("test"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--tag <TAG>' cannot be used with '--branch <BRANCH>'

    Usage: uv add --cache-dir [CACHE_DIR] --tag <TAG> --exclude-newer <EXCLUDE_NEWER> <PACKAGES|--requirements <REQUIREMENTS>>

    For more information, try '--help'.
    "
    );

    // --tag and --rev
    uv_snapshot!(context.filters(), context
        .add()
        .arg("foo")
        .arg("--tag")
        .arg("0.0.1")
        .arg("--rev")
        .arg("326b943"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--tag <TAG>' cannot be used with '--rev <REV>'

    Usage: uv add --cache-dir [CACHE_DIR] --tag <TAG> --exclude-newer <EXCLUDE_NEWER> <PACKAGES|--requirements <REQUIREMENTS>>

    For more information, try '--help'.
    "
    );

    // --tag and --tag
    uv_snapshot!(context.filters(), context
        .add()
        .arg("foo")
        .arg("--tag")
        .arg("0.0.1")
        .arg("--tag")
        .arg("0.0.2"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--tag <TAG>' cannot be used multiple times

    Usage: uv add [OPTIONS] <PACKAGES|--requirements <REQUIREMENTS>>

    For more information, try '--help'.
    "
    );
}

/// Avoiding persisting `add` calls when resolution fails.
#[test]
fn add_error() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("xyz"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because there are no versions of xyz and your project depends on xyz, we can conclude that your project's requirements are unsatisfiable.
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    ");

    uv_snapshot!(context.filters(), context.add().arg("xyz").arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    let lock = context.temp_dir.join("uv.lock");
    assert!(!lock.exists());

    Ok(())
}

/// Emit dedicated error message when adding Conda `environment.yml`
#[test]
fn add_environment_yml_error() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    let environment_yml = context.temp_dir.child("environment.yml");
    environment_yml.write_str(indoc! {r"
        name: test-env
        channels:
          - conda-forge
        dependencies:
          - python>=3.12
    "})?;

    uv_snapshot!(context.filters(), context.add().arg("-r").arg("environment.yml"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Conda environment files (i.e., `environment.yml`) are not supported
    ");

    Ok(())
}

/// Set a lower bound when adding unconstrained dependencies.
#[test]
fn add_lower_bound() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Adding `anyio` should include a lower-bound.
    uv_snapshot!(context.filters(), context.add().arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio>=4.3.0",
        ]
        "#
        );
    });

    Ok(())
}

/// Avoid setting a lower bound when updating existing dependencies.
#[test]
fn add_lower_bound_existing() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]
    "#})?;

    // Adding `anyio` should _not_ set a lower-bound, since it's already present (even if
    // unconstrained).
    uv_snapshot!(context.filters(), context.add().arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio",
        ]
        "#
        );
    });

    Ok(())
}

/// Avoid setting a lower bound with `--raw`.
#[test]
fn add_lower_bound_raw() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]
    "#})?;

    // Adding `anyio` should _not_ set a lower-bound when using `--raw`.
    uv_snapshot!(context.filters(), context.add().arg("anyio").arg("--raw"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio",
        ]
        "#
        );
    });

    Ok(())
}

/// Set a lower bound when adding unconstrained dev dependencies.
#[test]
fn add_lower_bound_dev() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Adding `anyio` should include a lower-bound.
    uv_snapshot!(context.filters(), context.add().arg("anyio").arg("--dev"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = [
            "anyio>=4.3.0",
        ]
        "#
        );
    });

    Ok(())
}

/// Set a lower bound when adding unconstrained optional dependencies.
#[test]
fn add_lower_bound_optional() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Adding `anyio` should include a lower-bound.
    uv_snapshot!(context.filters(), context.add().arg("anyio").arg("--optional=io"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        io = [
            "anyio>=4.3.0",
        ]
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642, upload-time = "2024-02-19T08:36:28.641Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584, upload-time = "2024-02-19T08:36:26.842Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        io = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", marker = "extra == 'io'", specifier = ">=4.3.0" }]
        provides-extras = ["io"]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]
        "#
        );
    });

    Ok(())
}

/// Omit the local segment when adding dependencies (since `>=1.2.3+local` is invalid).
#[test]
fn add_lower_bound_local() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Adding `torch` should include a lower-bound, but no local segment.
    uv_snapshot!(context.filters(), context.add().arg("local-simple-a").arg("--index").arg(packse_index_url()).env_remove(EnvVars::UV_EXCLUDE_NEWER), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + local-simple-a==1.2.3+foo
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "local-simple-a>=1.2.3",
        ]

        [[tool.uv.index]]
        url = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/"
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "local-simple-a"
        version = "1.2.3+foo"
        source = { registry = "https://astral-sh.github.io/packse/PACKSE_VERSION/simple-html/" }
        sdist = { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/local_simple_a-1.2.3+foo.tar.gz", hash = "sha256:cd1855a98a7b0dce1f4617f2f2089906936344392d4bdd7720503e9f3c0b1544" }
        wheels = [
            { url = "https://astral-sh.github.io/packse/PACKSE_VERSION/files/local_simple_a-1.2.3+foo-py3-none-any.whl", hash = "sha256:9a430e6d5e9cd3ab906ea412b00ea8a1bad7c59fd64df2278a2527a60a665751" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "local-simple-a" },
        ]

        [package.metadata]
        requires-dist = [{ name = "local-simple-a", specifier = ">=1.2.3" }]
        "#
        );
    });

    Ok(())
}

/// Add dependencies to a non-project workspace root.
#[test]
fn add_non_project() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r"
        [tool.uv.workspace]
        members = []
    "})?;

    // Adding `iniconfig` should fail, since virtual workspace roots don't support production
    // dependencies.
    uv_snapshot!(context.filters(), context.add().arg("iniconfig"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project is missing a `[project]` table; add a `[project]` table to use production dependencies, or run `uv add --dev` instead
    ");

    // Adding `iniconfig` as optional should fail, since virtual workspace roots don't support
    // optional dependencies.
    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--optional").arg("async"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project is missing a `[project]` table; add a `[project]` table to use optional dependencies, or run `uv add --dev` instead
    ");

    // Adding `iniconfig` as a dev dependency should succeed.
    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--dev"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [tool.uv.workspace]
        members = []

        [dependency-groups]
        dev = [
            "iniconfig>=2.0.0",
        ]
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]

        [manifest.dependency-groups]
        dev = [{ name = "iniconfig", specifier = ">=2.0.0" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]
        "#
        );
    });

    Ok(())
}

#[test]
fn add_virtual_empty() -> Result<()> {
    // testing how `uv add` reacts to a pyproject with no `[project]` and nothing useful to it
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [tool.mycooltool]
        wow = "someconfig"
    "#})?;

    // Add normal dep (doesn't make sense)
    uv_snapshot!(context.filters(), context.add()
        .arg("sortedcontainers"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Project is missing a `[project]` table; add a `[project]` table to use production dependencies, or run `uv add --dev` instead
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [tool.mycooltool]
        wow = "someconfig"
        "#
        );
    });

    // Add dependency-group (can make sense!)
    uv_snapshot!(context.filters(), context.add()
        .arg("sortedcontainers")
        .arg("--group").arg("dev"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sortedcontainers==2.4.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [tool.mycooltool]
        wow = "someconfig"

        [dependency-groups]
        dev = [
            "sortedcontainers>=2.4.0",
        ]
        "#
        );
    });

    Ok(())
}

#[test]
fn add_virtual_dependency_group() -> Result<()> {
    // testing basic `uv add --group` functionality
    // when the pyproject.toml is fully virtual (no `[project]`)
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [dependency-groups]
        foo = ["sortedcontainers"]
        bar = ["iniconfig"]
        dev = ["sniffio"]
    "#})?;

    // Add to existing group
    uv_snapshot!(context.filters(), context.add()
        .arg("sortedcontainers")
        .arg("--group").arg("dev"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + sniffio==1.3.1
     + sortedcontainers==2.4.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [dependency-groups]
        foo = ["sortedcontainers"]
        bar = ["iniconfig"]
        dev = [
            "sniffio",
            "sortedcontainers>=2.4.0",
        ]
        "#
        );
    });

    // Add to new group
    uv_snapshot!(context.filters(), context.add()
        .arg("sortedcontainers")
        .arg("--group").arg("baz"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 3 packages in [TIME]
    Audited 2 packages in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [dependency-groups]
        foo = ["sortedcontainers"]
        bar = ["iniconfig"]
        dev = [
            "sniffio",
            "sortedcontainers>=2.4.0",
        ]
        baz = [
            "sortedcontainers>=2.4.0",
        ]
        "#
        );
    });

    Ok(())
}

#[test]
fn add_empty_requirements_group() -> Result<()> {
    // Test that `uv add -r requirements.txt --group <name>` creates an empty group
    // when the requirements file is empty
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("")?;

    uv_snapshot!(context.filters(), context.add()
        .arg("-r").arg("requirements.txt")
        .arg("--group").arg("user"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Requirements file `requirements.txt` does not contain any dependencies
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        user = []
        "#
        );
    });

    Ok(())
}

#[test]
fn add_empty_requirements_optional() -> Result<()> {
    // Test that `uv add -r requirements.txt --optional <extra>` creates an empty extra
    // when the requirements file is empty
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("")?;

    uv_snapshot!(context.filters(), context.add()
        .arg("-r").arg("requirements.txt")
        .arg("--optional").arg("extra"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Requirements file `requirements.txt` does not contain any dependencies
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        extra = []
        "#
        );
    });

    Ok(())
}

#[test]
fn remove_virtual_empty() -> Result<()> {
    // testing how `uv remove` reacts to a pyproject with no `[project]` and nothing useful to it
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [tool.mycooltool]
        wow = "someconfig"

        "#,
    )?;

    // Remove normal dep (doesn't make sense)
    uv_snapshot!(context.filters(), context.remove()
        .arg("sortedcontainers"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The dependency `sortedcontainers` could not be found in `project.dependencies`
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"

        [tool.mycooltool]
        wow = "someconfig"
        "#
        );
    });

    // Remove dependency-group (can make sense, but nothing there!)
    uv_snapshot!(context.filters(), context.remove()
        .arg("sortedcontainers")
        .arg("--group").arg("dev"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The dependency `sortedcontainers` could not be found in `tool.uv.dev-dependencies` or `tool.uv.dependency-groups.dev`
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"

        [tool.mycooltool]
        wow = "someconfig"
        "#
        );
    });

    Ok(())
}

#[test]
fn remove_virtual_dependency_group() -> Result<()> {
    // testing basic `uv remove --group` functionality
    // when the pyproject.toml is fully virtual (no `[project]`)
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [dependency-groups]
        foo = ["sortedcontainers"]
        bar = ["iniconfig"]
        dev = ["sniffio"]
    "#})?;

    // Remove from group
    uv_snapshot!(context.filters(), context.remove()
        .arg("sortedcontainers")
        .arg("--group").arg("foo"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [dependency-groups]
        foo = []
        bar = ["iniconfig"]
        dev = ["sniffio"]
        "#
        );
    });

    // Remove from non-existent group
    uv_snapshot!(context.filters(), context.remove()
        .arg("sortedcontainers")
        .arg("--group").arg("baz"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The dependency `sortedcontainers` could not be found in `dependency-groups.baz`
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [dependency-groups]
        foo = []
        bar = ["iniconfig"]
        dev = ["sniffio"]
        "#
        );
    });

    Ok(())
}

/// Add the same requirement multiple times.
#[test]
fn add_repeat() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio>=4.3.0",
        ]
        "#
        );
    });

    uv_snapshot!(context.filters(), context.add().arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Audited 3 packages in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio>=4.3.0",
        ]
        "#
        );
    });

    Ok(())
}

/// Add from requirement file.
#[test]
#[cfg(feature = "test-git")]
fn add_requirements_file() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str("Flask==2.3.2\nanyio @ git+https://github.com/agronholm/anyio.git@4.4.0")?;

    uv_snapshot!(context.filters(), context.add().arg("-r").arg("requirements.txt"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + anyio==4.4.0 (from git+https://github.com/agronholm/anyio.git@053e8f0a0f7b0f4a47a012eb5c6b1d9d84344e6a)
     + blinker==1.7.0
     + click==8.1.7
     + flask==2.3.2
     + idna==3.6
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + sniffio==1.3.1
     + werkzeug==3.0.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio",
            "flask==2.3.2",
        ]

        [tool.uv.sources]
        anyio = { git = "https://github.com/agronholm/anyio.git", rev = "4.4.0" }
        "#
        );
    });

    // Passing stdin should succeed
    uv_snapshot!(context.filters(), context.add().arg("-r").arg("-").stdin(std::fs::File::open(requirements_txt)?), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Audited [N] packages in [TIME]
    ");

    // Passing a `setup.py` should fail.
    uv_snapshot!(context.filters(), context.add().arg("-r").arg("setup.py"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Adding requirements from a `setup.py` is not supported in `uv add`
    ");

    // Passing nothing should fail.
    uv_snapshot!(context.filters(), context.add(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <PACKAGES|--requirements <REQUIREMENTS>>

    Usage: uv add --cache-dir [CACHE_DIR] --exclude-newer <EXCLUDE_NEWER> <PACKAGES|--requirements <REQUIREMENTS>>

    For more information, try '--help'.
    ");

    Ok(())
}

/// Add a path dependency from a requirements file, respecting the lack of a `-e` flag.
#[test]
fn add_requirements_file_non_editable() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create a peer package.
    let child = context.temp_dir.child("packages").child("child");
    child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    child
        .child("src")
        .child("child")
        .child("__init__.py")
        .touch()?;

    // Without `-e`, the package should not be listed as editable.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("./packages/child")?;

    uv_snapshot!(context.filters(), context.add().arg("-r").arg("requirements.txt").arg("--no-workspace"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/packages/child)
    ");

    let pyproject_toml_content = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml_content, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "child",
        ]

        [tool.uv.sources]
        child = { path = "packages/child" }
        "#
        );
    });

    Ok(())
}

/// Add a path dependency from a requirements file, respecting `-e` for editable.
#[test]
fn add_requirements_file_editable() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create a peer package.
    let child = context.temp_dir.child("packages").child("child");
    child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    child
        .child("src")
        .child("child")
        .child("__init__.py")
        .touch()?;

    // With `-e`, the package should be listed as editable.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("-e ./packages/child")?;

    uv_snapshot!(context.filters(), context.add().arg("-r").arg("requirements.txt").arg("--no-workspace"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/packages/child)
    ");

    let pyproject_toml_content = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml_content, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "child",
        ]

        [tool.uv.sources]
        child = { path = "packages/child", editable = true }
        "#
        );
    });

    Ok(())
}

/// Add a path dependency from a requirements file, overriding the `-e` flag.
#[test]
fn add_requirements_file_editable_override() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create a peer package.
    let child = context.temp_dir.child("packages").child("child");
    child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    child
        .child("src")
        .child("child")
        .child("__init__.py")
        .touch()?;

    // With `-e`, the package should be listed as editable, but the `--no-editable` flag should
    // override it.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("-e ./packages/child")?;

    uv_snapshot!(context.filters(), context.add().arg("-r").arg("requirements.txt").arg("--no-workspace").arg("--no-editable"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/packages/child)
    ");

    let pyproject_toml_content = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml_content, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "child",
        ]

        [tool.uv.sources]
        child = { path = "packages/child", editable = false }
        "#
        );
    });

    Ok(())
}

/// Add requirements from a file with a marker flag.
///
/// We test that:
/// * Adding requirements from a file applies the marker to all of them
/// * We combine the marker with existing markers
/// * We only sync the packages applicable under this marker
#[test]
fn add_requirements_file_with_marker_flag() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_win_txt = context.temp_dir.child("requirements.win.txt");
    requirements_win_txt.write_str("anyio>=2.31.0\niniconfig>=2; sys_platform != 'fantasy_os'\nnumpy>1.9; sys_platform == 'fantasy_os'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    let base_pyproject_toml = indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#};
    pyproject_toml.write_str(base_pyproject_toml)?;

    // Add dependencies with a marker that does not apply for the current target.
    uv_snapshot!(context.filters(), context.add().arg("-r").arg("requirements.win.txt").arg("-m").arg("python_version == '3.11'"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");
    let edited_pyproject_toml = context.read("pyproject.toml");

    // TODO(konsti): We should output `python_version == '3.12'` instead of lowering to
    // `python_full_version`.
    assert_snapshot!(
        edited_pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "anyio>=2.31.0 ; python_full_version == '3.11.*'",
        "iniconfig>=2 ; python_full_version == '3.11.*' and sys_platform != 'fantasy_os'",
        "numpy>1.9 ; python_full_version == '3.11.*' and sys_platform == 'fantasy_os'",
    ]
    "#
    );

    // Reset the project.
    pyproject_toml.write_str(base_pyproject_toml)?;
    fs_err::remove_file(context.temp_dir.join("uv.lock"))?;

    // Add dependencies with a marker that applies for the current target.
    uv_snapshot!(context.filters(), context.add().arg("-r").arg("requirements.win.txt").arg("-m").arg("python_version == '3.12'"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
    ");
    let edited_pyproject_toml = context.read("pyproject.toml");

    assert_snapshot!(
        edited_pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "anyio>=2.31.0 ; python_full_version == '3.12.*'",
        "iniconfig>=2 ; python_full_version == '3.12.*' and sys_platform != 'fantasy_os'",
        "numpy>1.9 ; python_full_version == '3.12.*' and sys_platform == 'fantasy_os'",
    ]
    "#
    );

    Ok(())
}

/// Add from requirement file, with additional, external constraints.
///
/// The constraints should be respected, but they should _not_ be recorded in the `pyproject.toml`
/// or `uv.lock` file.
#[test]
fn add_requirements_file_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    let requirements_in = context.temp_dir.child("requirements.in");
    requirements_in.write_str(indoc! {r"
            flask
            anyio
        "})?;

    // Write a set of valid, but outdated compiled requirements.
    //
    // For reference, these are generated by compiling:
    // ```txt
    // flask
    // anyio<4
    // click<7
    // ```
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(indoc! {r"
            # This file was autogenerated by uv via the following command:
            #    uv pip compile --cache-dir [CACHE_DIR] requirements.in -o requirements.txt
            anyio==3.7.1
                # via -r requirements.in
            click==6.7
                # via
                #   -r requirements.in
                #   flask
            flask==1.1.4
                # via -r requirements.in
            idna==3.6
                # via anyio
            itsdangerous==1.1.0
                # via flask
            jinja2==2.11.3
                # via flask
            markupsafe==2.1.5
                # via jinja2
            sniffio==1.3.1
                # via anyio
            werkzeug==1.0.1
                # via flask
        "})?;

    // Pass the input requirements as constraints.
    uv_snapshot!(context.filters(), context.add().arg("-r").arg("requirements.in").arg("-c").arg("requirements.txt"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + anyio==3.7.1
     + click==6.7
     + flask==1.1.4
     + idna==3.6
     + itsdangerous==1.1.0
     + jinja2==2.11.3
     + markupsafe==2.1.5
     + sniffio==1.3.1
     + werkzeug==1.0.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio>=3.7.1",
            "flask>=1.1.4",
        ]
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "3.7.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/28/99/2dfd53fd55ce9838e6ff2d4dac20ce58263798bd1a0dbe18b3a9af3fcfce/anyio-3.7.1.tar.gz", hash = "sha256:44a3c9aba0f5defa43261a8b3efb97891f2bd7d804e0e1f56419befa1adfc780", size = 142927, upload-time = "2023-07-05T16:45:02.294Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/19/24/44299477fe7dcc9cb58d0a57d5a7588d6af2ff403fdd2d47a246c91a3246/anyio-3.7.1-py3-none-any.whl", hash = "sha256:91dee416e570e92c64041bd18b900d1d6fa78dff7048769ce5ac5ddad004fbb5", size = 80896, upload-time = "2023-07-05T16:44:59.805Z" },
        ]

        [[package]]
        name = "click"
        version = "6.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/95/d9/c3336b6b5711c3ab9d1d3a80f1a3e2afeb9d8c02a7166462f6cc96570897/click-6.7.tar.gz", hash = "sha256:f15516df478d5a56180fbf80e68f206010e6d160fc39fa508b65e035fd75130b", size = 279019, upload-time = "2017-01-06T22:41:13.209Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/34/c1/8806f99713ddb993c5366c362b2f908f18269f8d792aff1abfd700775a77/click-6.7-py2.py3-none-any.whl", hash = "sha256:29f99fc6125fbc931b758dc053b3114e55c77a6e4c6c3a2674a2dc986016381d", size = 71175, upload-time = "2017-01-06T22:41:15.466Z" },
        ]

        [[package]]
        name = "flask"
        version = "1.1.4"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "itsdangerous" },
            { name = "jinja2" },
            { name = "werkzeug" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/2d145f5fe718b2f15ebe69240538f06faa8bbb76488bf962091db1f7a26d/Flask-1.1.4.tar.gz", hash = "sha256:0fbeb6180d383a9186d0d6ed954e0042ad9f18e0e8de088b2b419d526927d196", size = 635920, upload-time = "2021-05-14T01:45:58.328Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e8/6d/994208daa354f68fd89a34a8bafbeaab26fda84e7af1e35bdaed02b667e6/Flask-1.1.4-py2.py3-none-any.whl", hash = "sha256:c34f04500f2cbbea882b1acb02002ad6fe6b7ffa64a6164577995657f50aed22", size = 94591, upload-time = "2021-05-14T01:45:55.061Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "itsdangerous"
        version = "1.1.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/68/1a/f27de07a8a304ad5fa817bbe383d1238ac4396da447fa11ed937039fa04b/itsdangerous-1.1.0.tar.gz", hash = "sha256:321b033d07f2a4136d3ec762eac9f16a10ccd60f53c0c91af90217ace7ba1f19", size = 53219, upload-time = "2018-10-27T00:17:37.922Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/76/ae/44b03b253d6fade317f32c24d100b3b35c2239807046a4c953c7b89fa49e/itsdangerous-1.1.0-py2.py3-none-any.whl", hash = "sha256:b12271b2047cb23eeb98c8b5622e2e5c5e9abd9784a153e9d8ef9cb4dd09d749", size = 16743, upload-time = "2018-10-27T00:17:35.954Z" },
        ]

        [[package]]
        name = "jinja2"
        version = "2.11.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4f/e7/65300e6b32e69768ded990494809106f87da1d436418d5f1367ed3966fd7/Jinja2-2.11.3.tar.gz", hash = "sha256:a6d58433de0ae800347cab1fa3043cebbabe8baa9d29e668f1c768cb87a333c6", size = 257589, upload-time = "2021-01-31T16:33:09.175Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7e/c2/1eece8c95ddbc9b1aeb64f5783a9e07a286de42191b7204d67b7496ddf35/Jinja2-2.11.3-py2.py3-none-any.whl", hash = "sha256:03e47ad063331dd6a3f04a43eddca8a966a26ba0c5b7207a9a9e4e08f1b29419", size = 125699, upload-time = "2021-01-31T16:33:07.289Z" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384, upload-time = "2024-02-02T16:31:22.863Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215, upload-time = "2024-02-02T16:30:33.081Z" },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069, upload-time = "2024-02-02T16:30:34.148Z" },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452, upload-time = "2024-02-02T16:30:35.149Z" },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462, upload-time = "2024-02-02T16:30:36.166Z" },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869, upload-time = "2024-02-02T16:30:37.834Z" },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906, upload-time = "2024-02-02T16:30:39.366Z" },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296, upload-time = "2024-02-02T16:30:40.413Z" },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038, upload-time = "2024-02-02T16:30:42.243Z" },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572, upload-time = "2024-02-02T16:30:43.326Z" },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127, upload-time = "2024-02-02T16:30:44.418Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "anyio" },
            { name = "flask" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", specifier = ">=3.7.1" },
            { name = "flask", specifier = ">=1.1.4" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]

        [[package]]
        name = "werkzeug"
        version = "1.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/10/27/a33329150147594eff0ea4c33c2036c0eadd933141055be0ff911f7f8d04/Werkzeug-1.0.1.tar.gz", hash = "sha256:6c80b1e5ad3665290ea39320b91e1be1e0d5f60652b964a3070216de83d2e47c", size = 904455, upload-time = "2020-03-31T18:03:37.943Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/94/5f7079a0e00bd6863ef8f1da638721e9da21e5bacee597595b318f71d62e/Werkzeug-1.0.1-py2.py3-none-any.whl", hash = "sha256:2de2a5db0baeae7b2d2664949077c2ac63fbd16d98da0ff71837f7d1dea3fd43", size = 298631, upload-time = "2020-03-31T18:03:34.839Z" },
        ]
        "#
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    ");

    Ok(())
}

/// Add a requirement to a dependency group.
#[test]
fn add_group() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0").arg("--group").arg("test"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    assert_snapshot!(pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = []

    [dependency-groups]
    test = [
        "anyio==3.7.0",
    ]
    "#
    );

    uv_snapshot!(context.filters(), context.add().arg("requests").arg("--group").arg("test"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + requests==2.31.0
     + urllib3==2.2.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    assert_snapshot!(pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = []

    [dependency-groups]
    test = [
        "anyio==3.7.0",
        "requests>=2.31.0",
    ]
    "#
    );

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0").arg("--group").arg("second"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Audited 3 packages in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    assert_snapshot!(pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = []

    [dependency-groups]
    second = [
        "anyio==3.7.0",
    ]
    test = [
        "anyio==3.7.0",
        "requests>=2.31.0",
    ]
    "#
    );

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0").arg("--group").arg("alpha"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Audited 3 packages in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    assert_snapshot!(pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = []

    [dependency-groups]
    alpha = [
        "anyio==3.7.0",
    ]
    second = [
        "anyio==3.7.0",
    ]
    test = [
        "anyio==3.7.0",
        "requests>=2.31.0",
    ]
    "#
    );

    assert!(context.temp_dir.join("uv.lock").exists());

    Ok(())
}

/// Normalize group names when adding or removing.
#[test]
fn add_group_normalize() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        cloud_export_to_parquet = [
            "anyio==3.7.0",
        ]
    "#})?;

    // Add with a non-normalized group name.
    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--group").arg("cloud_export_to_parquet"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    assert_snapshot!(pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = []

    [dependency-groups]
    cloud_export_to_parquet = [
        "anyio==3.7.0",
        "iniconfig>=2.0.0",
    ]
    "#
    );

    // Add with a normalized group name (which doesn't match the `pyproject.toml`).
    uv_snapshot!(context.filters(), context.add().arg("typing-extensions").arg("--group").arg("cloud-export-to-parquet"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    assert_snapshot!(pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = []

    [dependency-groups]
    cloud_export_to_parquet = [
        "anyio==3.7.0",
        "iniconfig>=2.0.0",
        "typing-extensions>=4.10.0",
    ]
    "#
    );

    // Remove with a non-normalized group name.
    uv_snapshot!(context.filters(), context.remove().arg("iniconfig").arg("--group").arg("cloud_export_to_parquet"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Uninstalled 5 packages in [TIME]
     - anyio==3.7.0
     - idna==3.6
     - iniconfig==2.0.0
     - sniffio==1.3.1
     - typing-extensions==4.10.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    assert_snapshot!(pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = []

    [dependency-groups]
    cloud_export_to_parquet = [
        "anyio==3.7.0",
        "typing-extensions>=4.10.0",
    ]
    "#
    );

    // Remove with a normalized group name (which doesn't match the `pyproject.toml`).
    uv_snapshot!(context.filters(), context.remove().arg("typing-extensions").arg("--group").arg("cloud-export-to-parquet"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Audited in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    assert_snapshot!(pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = []

    [dependency-groups]
    cloud_export_to_parquet = [
        "anyio==3.7.0",
    ]
    "#
    );

    Ok(())
}

/// Add a requirement to a dependency group (sorted before the other groups).
#[test]
fn add_group_before_commented_groups() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        # This is our dev group
        dev = [
            "anyio==3.7.0",
        ]
        # This is our test group
        test = [
            "anyio==3.7.0",
            "requests>=2.31.0",
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0").arg("--group").arg("alpha"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    assert!(context.temp_dir.join("uv.lock").exists());

    assert_snapshot!(pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = []

    [dependency-groups]
    alpha = [
        "anyio==3.7.0",
    ]
    # This is our dev group
    dev = [
        "anyio==3.7.0",
    ]
    # This is our test group
    test = [
        "anyio==3.7.0",
        "requests>=2.31.0",
    ]
    "#
    );

    Ok(())
}

/// Add a requirement to dependency group (sorted between the other groups).
#[test]
fn add_group_between_commented_groups() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        # This is our dev group
        dev = [
            "anyio==3.7.0",
        ]
        # This is our test group
        test = [
            "anyio==3.7.0",
            "requests>=2.31.0",
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0").arg("--group").arg("eta"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    assert!(context.temp_dir.join("uv.lock").exists());

    assert_snapshot!(pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = []

    [dependency-groups]
    # This is our dev group
    dev = [
        "anyio==3.7.0",
    ]
    eta = [
        "anyio==3.7.0",
    ]
    # This is our test group
    test = [
        "anyio==3.7.0",
        "requests>=2.31.0",
    ]
    "#
    );

    Ok(())
}

/// Add a requirement to a dependency group when existing dependency group
/// keys are not sorted.
#[test]
fn add_group_to_unsorted() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        test = [
            "anyio==3.7.0",
            "requests>=2.31.0",
        ]
        second = [
            "anyio==3.7.0",
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0").arg("--group").arg("alpha"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    assert_snapshot!(pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = []

    [dependency-groups]
    test = [
        "anyio==3.7.0",
        "requests>=2.31.0",
    ]
    second = [
        "anyio==3.7.0",
    ]
    alpha = [
        "anyio==3.7.0",
    ]
    "#
    );

    assert!(context.temp_dir.join("uv.lock").exists());

    Ok(())
}

/// Remove a requirement from a dependency group.
#[test]
fn remove_group() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        test = [
            "anyio==3.7.0",
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.remove().arg("anyio").arg("--group").arg("test"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        test = []
        "#
        );
    });

    uv_snapshot!(context.filters(), context.remove().arg("anyio").arg("--group").arg("test"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The dependency `anyio` could not be found in `dependency-groups.test`
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        test = []
        "#
        );
    });

    uv_snapshot!(context.filters(), context.remove().arg("anyio").arg("--group").arg("test"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The dependency `anyio` could not be found in `dependency-groups.test`
    ");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]
    "#})?;

    uv_snapshot!(context.filters(), context.remove().arg("anyio").arg("--group").arg("test"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    hint: `anyio` is a production dependency
    error: The dependency `anyio` could not be found in `dependency-groups.test`
    ");

    Ok(())
}

/// Add to a PEP 732 script.
#[test]
fn add_script() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "requests<3",
        #   "rich",
        # ]
        # ///

        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio").arg("--script").arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    ");

    let script_content = context.read("script.py");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio>=4.3.0",
        #   "requests<3",
        #   "rich",
        # ]
        # ///

        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
        "#
        );
    });

    // Adding to a script without a lockfile shouldn't create a lockfile.
    assert!(!context.temp_dir.join("script.py.lock").exists());

    Ok(())
}

/// Test that `--bounds` is respected when adding to a script without a lockfile.
#[test]
fn add_script_bounds() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        print("Hello, world!")
    "#})?;

    // Add `anyio` with `--bounds minor` to the script.
    uv_snapshot!(context.filters(), context.add().arg("anyio").arg("--bounds").arg("minor").arg("--script").arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let script_content = context.read("script.py");

    // The script should have bounds with minor version constraint (e.g., `>=4.3.0,<4.4.0`).
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = [
        #     "anyio>=4.3.0,<4.4.0",
        # ]
        # ///
        print("Hello, world!")
        "#
        );
    });

    // Adding to a script without a lockfile shouldn't create a lockfile.
    assert!(!context.temp_dir.join("script.py.lock").exists());

    Ok(())
}

#[test]
fn add_script_relative_path() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let project = context.temp_dir.child("project");
    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        print("Hello, world!")
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("./project").arg("--editable").arg("--script").arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    let script_content = context.read("script.py");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = [
        #     "project",
        # ]
        #
        # [tool.uv.sources]
        # project = { path = "project", editable = true }
        # ///
        print("Hello, world!")
        "#
        );
    });
    Ok(())
}

/// Respect inline settings when adding to a PEP 732 script.
#[test]
fn add_script_settings() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "requests>=2",
        #   "rich>=12",
        # ]
        #
        # [tool.uv]
        # resolution = "lowest-direct"
        # ///

        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
    "#})?;

    // Lock the script.
    uv_snapshot!(context.filters(), context.lock().arg("--script").arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    // Add `anyio` to the script.
    uv_snapshot!(context.filters(), context.add().arg("anyio>=3").arg("--script").arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    ");

    let script_content = context.read("script.py");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio>=3",
        #   "requests>=2",
        #   "rich>=12",
        # ]
        #
        # [tool.uv]
        # resolution = "lowest-direct"
        # ///

        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
        "#
        );
    });

    let lock = context.read("script.py.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.11"

        [options]
        resolution-mode = "lowest-direct"
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [
            { name = "anyio", specifier = ">=3" },
            { name = "requests", specifier = ">=2" },
            { name = "rich", specifier = ">=12" },
        ]

        [[package]]
        name = "anyio"
        version = "3.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/99/0d/65165f99e5f4f3b4c43a5ed9db0fb7aa655f5a58f290727a30528a87eb45/anyio-3.0.0.tar.gz", hash = "sha256:b553598332c050af19f7d41f73a7790142f5bc3d5eb8bd82f5e515ec22019bd9", size = 116952, upload-time = "2021-04-20T14:02:14.75Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/3b/49/ebee263b69fe243bd1fd0a88bc6bb0f7732bf1794ba3273cb446351f9482/anyio-3.0.0-py3-none-any.whl", hash = "sha256:e71c3d9d72291d12056c0265d07c6bbedf92332f78573e278aeb116f24f30395", size = 72182, upload-time = "2021-04-20T14:02:13.663Z" },
        ]

        [[package]]
        name = "commonmark"
        version = "0.9.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/60/48/a60f593447e8f0894ebb7f6e6c1f25dafc5e89c5879fdc9360ae93ff83f0/commonmark-0.9.1.tar.gz", hash = "sha256:452f9dc859be7f06631ddcb328b6919c67984aca654e5fefb3914d54691aed60", size = 95764, upload-time = "2019-10-04T15:37:39.817Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b1/92/dfd892312d822f36c55366118b95d914e5f16de11044a27cf10a7d71bbbf/commonmark-0.9.1-py2.py3-none-any.whl", hash = "sha256:da2f38c92590f83de410ba1a3cbceafbc74fee9def35f9251ba9a971d6d66fd9", size = 51068, upload-time = "2019-10-04T15:37:37.674Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "pygments"
        version = "2.17.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/55/59/8bccf4157baf25e4aa5a0bb7fa3ba8600907de105ebc22b0c78cfbf6f565/pygments-2.17.2.tar.gz", hash = "sha256:da46cec9fd2de5be3a8a784f434e4c4ab670b4ff54d605c4c2717e9d49c4c367", size = 4827772, upload-time = "2023-11-21T20:43:53.875Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/97/9c/372fef8377a6e340b1704768d20daaded98bf13282b5327beb2e2fe2c7ef/pygments-2.17.2-py3-none-any.whl", hash = "sha256:b27c2826c47d0f3219f29554824c30c5e8945175d888647acd804ddd04af846c", size = 1179756, upload-time = "2023-11-21T20:43:49.423Z" },
        ]

        [[package]]
        name = "requests"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/8e/88/102742c48605aef8d39fa719d932c67783d789679628fa1433cb4b2c7a2a/requests-2.0.0.tar.gz", hash = "sha256:78536038f54cff6ade3be6863403146665b5a3923dd61108c98d8b64141f9d70", size = 362994, upload-time = "2013-09-24T18:39:33.154Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bf/78/be2b4c440ea767336d8448fe671fe1d78ca499e49d77dac90f92191cca0e/requests-2.0.0-py2.py3-none-any.whl", hash = "sha256:2ef65639cb9600443f85451df487818c31f993ab288f313d29cc9db4f3cbe6ed", size = 391141, upload-time = "2013-11-15T19:09:51.527Z" },
        ]

        [[package]]
        name = "rich"
        version = "12.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "commonmark" },
            { name = "pygments" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/88/fd/13ee53f8bdf538b64c8ed505a92d3bc1f2c0d1071173d65216af61e0e88b/rich-12.0.0.tar.gz", hash = "sha256:14bfd0507edc633e021b02c45cbf7ca22e33b513817627b8de3412f047a3e798", size = 206045, upload-time = "2022-03-10T15:31:16.385Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/93/ac/463598448d578f1f9168eb17405b2faebb4e76123c986b2c4ab64d387d5e/rich-12.0.0-py3-none-any.whl", hash = "sha256:fdcd2f8d416e152bcf35c659987038d1ae5a7bd336e821ca7551858a4c7e38a9", size = 224015, upload-time = "2022-03-10T15:31:14.82Z" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]
        "#
        );
    });

    Ok(())
}

#[test]
fn add_script_trailing_comment_lines() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "requests<3",
        #   "rich",
        # ]
        # ///
        #
        # Additional description

        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio").arg("--script").arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    ");

    let script_content = context.read("script.py");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio>=4.3.0",
        #   "requests<3",
        #   "rich",
        # ]
        # ///
        #
        # Additional description

        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
        "#
        );
    });

    // Adding to a script without a lockfile shouldn't create a lockfile.
    assert!(!context.temp_dir.join("script.py.lock").exists());

    Ok(())
}

/// Add to a script without an existing metadata table.
#[test]
fn add_script_without_metadata_table() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
    "#})?;

    uv_snapshot!(context.filters(), context.add().args(["rich", "requests<3"]).arg("--script").arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    ");

    let script_content = context.read("script.py");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = [
        #     "requests<3",
        #     "rich>=13.7.1",
        # ]
        # ///
        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
        "#
        );
    });
    Ok(())
}

/// Add to a script without an existing metadata table, but with a shebang.
#[test]
fn add_script_without_metadata_table_with_shebang() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        #!/usr/bin/env python3
        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
    "#})?;

    uv_snapshot!(context.filters(), context.add().args(["rich", "requests<3"]).arg("--script").arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    ");

    let script_content = context.read("script.py");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        #!/usr/bin/env python3
        # /// script
        # requires-python = ">=3.12"
        # dependencies = [
        #     "requests<3",
        #     "rich>=13.7.1",
        # ]
        # ///
        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
        "#
        );
    });
    Ok(())
}

/// Add to a script with a metadata table and a shebang.
#[test]
fn add_script_with_metadata_table_and_shebang() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        #!/usr/bin/env python3
        # /// script
        # requires-python = ">=3.12"
        # dependencies = []
        # ///
        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
    "#})?;

    uv_snapshot!(context.filters(), context.add().args(["rich", "requests<3"]).arg("--script").arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    ");

    let script_content = context.read("script.py");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        #!/usr/bin/env python3
        # /// script
        # requires-python = ">=3.12"
        # dependencies = [
        #     "requests<3",
        #     "rich>=13.7.1",
        # ]
        # ///
        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
        "#
        );
    });
    Ok(())
}

/// Add to a script without a metadata table, but with a docstring.
#[test]
fn add_script_without_metadata_table_with_docstring() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        """This is a script."""
        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
    "#})?;

    uv_snapshot!(context.filters(), context.add().args(["rich", "requests<3"]).arg("--script").arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    ");

    let script_content = context.read("script.py");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = [
        #     "requests<3",
        #     "rich>=13.7.1",
        # ]
        # ///
        """This is a script."""
        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
        "#
        );
    });
    Ok(())
}

/// Add to a script without a `.py` extension.
#[test]
fn add_extensionless_script() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script");
    script.write_str(indoc! {r#"
        #!/usr/bin/env python3
        # /// script
        # requires-python = ">=3.12"
        # dependencies = []
        # ///
        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
    "#})?;

    uv_snapshot!(context.filters(), context.add().args(["rich", "requests<3"]).arg("--script").arg("script"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    ");

    let script_content = context.read("script");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        #!/usr/bin/env python3
        # /// script
        # requires-python = ">=3.12"
        # dependencies = [
        #     "requests<3",
        #     "rich>=13.7.1",
        # ]
        # ///
        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
        "#
        );
    });
    Ok(())
}

/// Add from a remote PEP 723 script via `-r`.
#[tokio::test]
async fn add_requirements_from_remote_script() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create a mock server that serves a PEP 723 script.
    let server = MockServer::start().await;
    let script_content = indoc! {r#"
        # /// script
        # requires-python = ">=3.12"
        # dependencies = [
        #     "anyio>=4",
        #     "rich",
        # ]
        # ///
        import anyio
        from rich.pretty import pprint
        pprint("Hello, world!")
    "#};

    Mock::given(method("GET"))
        .and(path("/script"))
        .respond_with(ResponseTemplate::new(200).set_body_string(script_content))
        .mount(&server)
        .await;

    let script_url = format!("{}/script", server.uri());

    uv_snapshot!(context.filters(), context.add().arg("-r").arg(&script_url), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + markdown-it-py==3.0.0
     + mdurl==0.1.2
     + pygments==2.17.2
     + rich==13.7.1
     + sniffio==1.3.1
    ");

    let pyproject_toml_content = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml_content, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio>=4",
            "rich>=13.7.1",
        ]
        "#
        );
    });

    Ok(())
}

/// Remove a dependency that is present in multiple places.
#[test]
fn remove_repeated() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let anyio_local = context.workspace_root.join("test/packages/anyio_local");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [project.optional-dependencies]
        foo = ["anyio"]

        [tool.uv]
        dev-dependencies = ["anyio"]

        [tool.uv.sources]
        anyio = {{ path = "{anyio_local}" }}
    "#,
        anyio_local = anyio_local.portable_display(),
    })?;

    uv_snapshot!(context.filters(), context.remove().arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `tool.uv.dev-dependencies` field (used in `pyproject.toml`) is deprecated and will be removed in a future release; use `dependency-groups.dev` instead
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + anyio==4.3.0+foo (from file://[WORKSPACE]/test/packages/anyio_local)
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        foo = ["anyio"]

        [tool.uv]
        dev-dependencies = ["anyio"]

        [tool.uv.sources]
        anyio = { path = "[WORKSPACE]/test/packages/anyio_local" }
        "#
        );
    });

    uv_snapshot!(context.filters(), context.remove().arg("anyio").arg("--optional").arg("foo"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `tool.uv.dev-dependencies` field (used in `pyproject.toml`) is deprecated and will be removed in a future release; use `dependency-groups.dev` instead
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        foo = []

        [tool.uv]
        dev-dependencies = ["anyio"]

        [tool.uv.sources]
        anyio = { path = "[WORKSPACE]/test/packages/anyio_local" }
        "#
        );
    });

    uv_snapshot!(context.filters(), context.remove().arg("anyio").arg("--dev"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The `tool.uv.dev-dependencies` field (used in `pyproject.toml`) is deprecated and will be removed in a future release; use `dependency-groups.dev` instead
    Resolved 1 package in [TIME]
    Uninstalled 1 package in [TIME]
     - anyio==4.3.0+foo (from file://[WORKSPACE]/test/packages/anyio_local)
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        foo = []

        [tool.uv]
        dev-dependencies = []
        "#
        );
    });
    Ok(())
}

/// Add to (and remove from) a PEP 732 script with a lockfile.
#[test]
fn add_remove_script_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "requests<3",
        #   "rich",
        # ]
        # ///

        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
    "#})?;

    // Explicitly lock the script.
    uv_snapshot!(context.filters(), context.lock().arg("--script").arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    ");

    let lock = context.read("script.py.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.11"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [
            { name = "requests", specifier = "<3" },
            { name = "rich" },
        ]

        [[package]]
        name = "certifi"
        version = "2024.2.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/71/da/e94e26401b62acd6d91df2b52954aceb7f561743aa5ccc32152886c76c96/certifi-2024.2.2.tar.gz", hash = "sha256:0569859f95fc761b18b45ef421b1290a0f65f147e92a1e5eb3e635f9a5e4e66f", size = 164886, upload-time = "2024-02-02T01:22:17.364Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ba/06/a07f096c664aeb9f01624f858c3add0a4e913d6c96257acb4fce61e7de14/certifi-2024.2.2-py3-none-any.whl", hash = "sha256:dc383c07b76109f368f6106eee2b593b04a011ea4d55f652c6ca24a754d1cdd1", size = 163774, upload-time = "2024-02-02T01:22:14.86Z" },
        ]

        [[package]]
        name = "charset-normalizer"
        version = "3.3.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/63/09/c1bc53dab74b1816a00d8d030de5bf98f724c52c1635e07681d312f20be8/charset-normalizer-3.3.2.tar.gz", hash = "sha256:f30c3cb33b24454a82faecaf01b19c18562b1e89558fb6c56de4d9118a032fd5", size = 104809, upload-time = "2023-11-01T04:04:59.997Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/77/02839016f6fbbf808e8b38601df6e0e66c17bbab76dff4613f7511413597/charset_normalizer-3.3.2-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:802fe99cca7457642125a8a88a084cef28ff0cf9407060f7b93dca5aa25480db", size = 191647, upload-time = "2023-11-01T04:02:55.329Z" },
            { url = "https://files.pythonhosted.org/packages/3e/33/21a875a61057165e92227466e54ee076b73af1e21fe1b31f1e292251aa1e/charset_normalizer-3.3.2-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:573f6eac48f4769d667c4442081b1794f52919e7edada77495aaed9236d13a96", size = 121434, upload-time = "2023-11-01T04:02:57.173Z" },
            { url = "https://files.pythonhosted.org/packages/dd/51/68b61b90b24ca35495956b718f35a9756ef7d3dd4b3c1508056fa98d1a1b/charset_normalizer-3.3.2-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:549a3a73da901d5bc3ce8d24e0600d1fa85524c10287f6004fbab87672bf3e1e", size = 118979, upload-time = "2023-11-01T04:02:58.442Z" },
            { url = "https://files.pythonhosted.org/packages/e4/a6/7ee57823d46331ddc37dd00749c95b0edec2c79b15fc0d6e6efb532e89ac/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:f27273b60488abe721a075bcca6d7f3964f9f6f067c8c4c605743023d7d3944f", size = 136582, upload-time = "2023-11-01T04:02:59.776Z" },
            { url = "https://files.pythonhosted.org/packages/74/f1/0d9fe69ac441467b737ba7f48c68241487df2f4522dd7246d9426e7c690e/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:1ceae2f17a9c33cb48e3263960dc5fc8005351ee19db217e9b1bb15d28c02574", size = 146645, upload-time = "2023-11-01T04:03:02.186Z" },
            { url = "https://files.pythonhosted.org/packages/05/31/e1f51c76db7be1d4aef220d29fbfa5dbb4a99165d9833dcbf166753b6dc0/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:65f6f63034100ead094b8744b3b97965785388f308a64cf8d7c34f2f2e5be0c4", size = 139398, upload-time = "2023-11-01T04:03:04.255Z" },
            { url = "https://files.pythonhosted.org/packages/40/26/f35951c45070edc957ba40a5b1db3cf60a9dbb1b350c2d5bef03e01e61de/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:753f10e867343b4511128c6ed8c82f7bec3bd026875576dfd88483c5c73b2fd8", size = 140273, upload-time = "2023-11-01T04:03:05.983Z" },
            { url = "https://files.pythonhosted.org/packages/07/07/7e554f2bbce3295e191f7e653ff15d55309a9ca40d0362fcdab36f01063c/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:4a78b2b446bd7c934f5dcedc588903fb2f5eec172f3d29e52a9096a43722adfc", size = 142577, upload-time = "2023-11-01T04:03:07.567Z" },
            { url = "https://files.pythonhosted.org/packages/d8/b5/eb705c313100defa57da79277d9207dc8d8e45931035862fa64b625bfead/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_aarch64.whl", hash = "sha256:e537484df0d8f426ce2afb2d0f8e1c3d0b114b83f8850e5f2fbea0e797bd82ae", size = 137747, upload-time = "2023-11-01T04:03:08.886Z" },
            { url = "https://files.pythonhosted.org/packages/19/28/573147271fd041d351b438a5665be8223f1dd92f273713cb882ddafe214c/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_i686.whl", hash = "sha256:eb6904c354526e758fda7167b33005998fb68c46fbc10e013ca97f21ca5c8887", size = 143375, upload-time = "2023-11-01T04:03:10.613Z" },
            { url = "https://files.pythonhosted.org/packages/cf/7c/f3b682fa053cc21373c9a839e6beba7705857075686a05c72e0f8c4980ca/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_ppc64le.whl", hash = "sha256:deb6be0ac38ece9ba87dea880e438f25ca3eddfac8b002a2ec3d9183a454e8ae", size = 148474, upload-time = "2023-11-01T04:03:11.973Z" },
            { url = "https://files.pythonhosted.org/packages/1e/49/7ab74d4ac537ece3bc3334ee08645e231f39f7d6df6347b29a74b0537103/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_s390x.whl", hash = "sha256:4ab2fe47fae9e0f9dee8c04187ce5d09f48eabe611be8259444906793ab7cbce", size = 140232, upload-time = "2023-11-01T04:03:13.505Z" },
            { url = "https://files.pythonhosted.org/packages/2d/dc/9dacba68c9ac0ae781d40e1a0c0058e26302ea0660e574ddf6797a0347f7/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:80402cd6ee291dcb72644d6eac93785fe2c8b9cb30893c1af5b8fdd753b9d40f", size = 140859, upload-time = "2023-11-01T04:03:17.362Z" },
            { url = "https://files.pythonhosted.org/packages/6c/c2/4a583f800c0708dd22096298e49f887b49d9746d0e78bfc1d7e29816614c/charset_normalizer-3.3.2-cp311-cp311-win32.whl", hash = "sha256:7cd13a2e3ddeed6913a65e66e94b51d80a041145a026c27e6bb76c31a853c6ab", size = 92509, upload-time = "2023-11-01T04:03:21.453Z" },
            { url = "https://files.pythonhosted.org/packages/57/ec/80c8d48ac8b1741d5b963797b7c0c869335619e13d4744ca2f67fc11c6fc/charset_normalizer-3.3.2-cp311-cp311-win_amd64.whl", hash = "sha256:663946639d296df6a2bb2aa51b60a2454ca1cb29835324c640dafb5ff2131a77", size = 99870, upload-time = "2023-11-01T04:03:22.723Z" },
            { url = "https://files.pythonhosted.org/packages/d1/b2/fcedc8255ec42afee97f9e6f0145c734bbe104aac28300214593eb326f1d/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:0b2b64d2bb6d3fb9112bafa732def486049e63de9618b5843bcdd081d8144cd8", size = 192892, upload-time = "2023-11-01T04:03:24.135Z" },
            { url = "https://files.pythonhosted.org/packages/2e/7d/2259318c202f3d17f3fe6438149b3b9e706d1070fe3fcbb28049730bb25c/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:ddbb2551d7e0102e7252db79ba445cdab71b26640817ab1e3e3648dad515003b", size = 122213, upload-time = "2023-11-01T04:03:25.66Z" },
            { url = "https://files.pythonhosted.org/packages/3a/52/9f9d17c3b54dc238de384c4cb5a2ef0e27985b42a0e5cc8e8a31d918d48d/charset_normalizer-3.3.2-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:55086ee1064215781fff39a1af09518bc9255b50d6333f2e4c74ca09fac6a8f6", size = 119404, upload-time = "2023-11-01T04:03:27.04Z" },
            { url = "https://files.pythonhosted.org/packages/99/b0/9c365f6d79a9f0f3c379ddb40a256a67aa69c59609608fe7feb6235896e1/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:8f4a014bc36d3c57402e2977dada34f9c12300af536839dc38c0beab8878f38a", size = 137275, upload-time = "2023-11-01T04:03:28.466Z" },
            { url = "https://files.pythonhosted.org/packages/91/33/749df346e93d7a30cdcb90cbfdd41a06026317bfbfb62cd68307c1a3c543/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:a10af20b82360ab00827f916a6058451b723b4e65030c5a18577c8b2de5b3389", size = 147518, upload-time = "2023-11-01T04:03:29.82Z" },
            { url = "https://files.pythonhosted.org/packages/72/1a/641d5c9f59e6af4c7b53da463d07600a695b9824e20849cb6eea8a627761/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:8d756e44e94489e49571086ef83b2bb8ce311e730092d2c34ca8f7d925cb20aa", size = 140182, upload-time = "2023-11-01T04:03:31.511Z" },
            { url = "https://files.pythonhosted.org/packages/ee/fb/14d30eb4956408ee3ae09ad34299131fb383c47df355ddb428a7331cfa1e/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:90d558489962fd4918143277a773316e56c72da56ec7aa3dc3dbbe20fdfed15b", size = 141869, upload-time = "2023-11-01T04:03:32.887Z" },
            { url = "https://files.pythonhosted.org/packages/df/3e/a06b18788ca2eb6695c9b22325b6fde7dde0f1d1838b1792a0076f58fe9d/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:6ac7ffc7ad6d040517be39eb591cac5ff87416c2537df6ba3cba3bae290c0fed", size = 144042, upload-time = "2023-11-01T04:03:34.412Z" },
            { url = "https://files.pythonhosted.org/packages/45/59/3d27019d3b447a88fe7e7d004a1e04be220227760264cc41b405e863891b/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:7ed9e526742851e8d5cc9e6cf41427dfc6068d4f5a3bb03659444b4cabf6bc26", size = 138275, upload-time = "2023-11-01T04:03:35.759Z" },
            { url = "https://files.pythonhosted.org/packages/7b/ef/5eb105530b4da8ae37d506ccfa25057961b7b63d581def6f99165ea89c7e/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:8bdb58ff7ba23002a4c5808d608e4e6c687175724f54a5dade5fa8c67b604e4d", size = 144819, upload-time = "2023-11-01T04:03:37.216Z" },
            { url = "https://files.pythonhosted.org/packages/a2/51/e5023f937d7f307c948ed3e5c29c4b7a3e42ed2ee0b8cdf8f3a706089bf0/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_ppc64le.whl", hash = "sha256:6b3251890fff30ee142c44144871185dbe13b11bab478a88887a639655be1068", size = 149415, upload-time = "2023-11-01T04:03:38.694Z" },
            { url = "https://files.pythonhosted.org/packages/24/9d/2e3ef673dfd5be0154b20363c5cdcc5606f35666544381bee15af3778239/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_s390x.whl", hash = "sha256:b4a23f61ce87adf89be746c8a8974fe1c823c891d8f86eb218bb957c924bb143", size = 141212, upload-time = "2023-11-01T04:03:40.07Z" },
            { url = "https://files.pythonhosted.org/packages/5b/ae/ce2c12fcac59cb3860b2e2d76dc405253a4475436b1861d95fe75bdea520/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:efcb3f6676480691518c177e3b465bcddf57cea040302f9f4e6e191af91174d4", size = 142167, upload-time = "2023-11-01T04:03:41.491Z" },
            { url = "https://files.pythonhosted.org/packages/ed/3a/a448bf035dce5da359daf9ae8a16b8a39623cc395a2ffb1620aa1bce62b0/charset_normalizer-3.3.2-cp312-cp312-win32.whl", hash = "sha256:d965bba47ddeec8cd560687584e88cf699fd28f192ceb452d1d7ee807c5597b7", size = 93041, upload-time = "2023-11-01T04:03:42.836Z" },
            { url = "https://files.pythonhosted.org/packages/b6/7c/8debebb4f90174074b827c63242c23851bdf00a532489fba57fef3416e40/charset_normalizer-3.3.2-cp312-cp312-win_amd64.whl", hash = "sha256:96b02a3dc4381e5494fad39be677abcb5e6634bf7b4fa83a6dd3112607547001", size = 100397, upload-time = "2023-11-01T04:03:44.467Z" },
            { url = "https://files.pythonhosted.org/packages/28/76/e6222113b83e3622caa4bb41032d0b1bf785250607392e1b778aca0b8a7d/charset_normalizer-3.3.2-py3-none-any.whl", hash = "sha256:3e4d1f6587322d2788836a99c69062fbb091331ec940e02d12d179c1d53e25fc", size = 48543, upload-time = "2023-11-01T04:04:58.622Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "markdown-it-py"
        version = "3.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "mdurl" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/38/71/3b932df36c1a044d397a1f92d1cf91ee0a503d91e470cbd670aa66b07ed0/markdown-it-py-3.0.0.tar.gz", hash = "sha256:e3f60a94fa066dc52ec76661e37c851cb232d92f9886b15cb560aaada2df8feb", size = 74596, upload-time = "2023-06-03T06:41:14.443Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/42/d7/1ec15b46af6af88f19b8e5ffea08fa375d433c998b8a7639e76935c14f1f/markdown_it_py-3.0.0-py3-none-any.whl", hash = "sha256:355216845c60bd96232cd8d8c40e8f9765cc86f46880e43a8fd22dc1a1a8cab1", size = 87528, upload-time = "2023-06-03T06:41:11.019Z" },
        ]

        [[package]]
        name = "mdurl"
        version = "0.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d6/54/cfe61301667036ec958cb99bd3efefba235e65cdeb9c84d24a8293ba1d90/mdurl-0.1.2.tar.gz", hash = "sha256:bb413d29f5eea38f31dd4754dd7377d4465116fb207585f97bf925588687c1ba", size = 8729, upload-time = "2022-08-14T12:40:10.846Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b3/38/89ba8ad64ae25be8de66a6d463314cf1eb366222074cfda9ee839c56a4b4/mdurl-0.1.2-py3-none-any.whl", hash = "sha256:84008a41e51615a49fc9966191ff91509e3c40b939176e643fd50a5c2196b8f8", size = 9979, upload-time = "2022-08-14T12:40:09.779Z" },
        ]

        [[package]]
        name = "pygments"
        version = "2.17.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/55/59/8bccf4157baf25e4aa5a0bb7fa3ba8600907de105ebc22b0c78cfbf6f565/pygments-2.17.2.tar.gz", hash = "sha256:da46cec9fd2de5be3a8a784f434e4c4ab670b4ff54d605c4c2717e9d49c4c367", size = 4827772, upload-time = "2023-11-21T20:43:53.875Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/97/9c/372fef8377a6e340b1704768d20daaded98bf13282b5327beb2e2fe2c7ef/pygments-2.17.2-py3-none-any.whl", hash = "sha256:b27c2826c47d0f3219f29554824c30c5e8945175d888647acd804ddd04af846c", size = 1179756, upload-time = "2023-11-21T20:43:49.423Z" },
        ]

        [[package]]
        name = "requests"
        version = "2.31.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "certifi" },
            { name = "charset-normalizer" },
            { name = "idna" },
            { name = "urllib3" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9d/be/10918a2eac4ae9f02f6cfe6414b7a155ccd8f7f9d4380d62fd5b955065c3/requests-2.31.0.tar.gz", hash = "sha256:942c5a758f98d790eaed1a29cb6eefc7ffb0d1cf7af05c3d2791656dbd6ad1e1", size = 110794, upload-time = "2023-05-22T15:12:44.175Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/70/8e/0e2d847013cb52cd35b38c009bb167a1a26b2ce6cd6965bf26b47bc0bf44/requests-2.31.0-py3-none-any.whl", hash = "sha256:58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f", size = 62574, upload-time = "2023-05-22T15:12:42.313Z" },
        ]

        [[package]]
        name = "rich"
        version = "13.7.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markdown-it-py" },
            { name = "pygments" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b3/01/c954e134dc440ab5f96952fe52b4fdc64225530320a910473c1fe270d9aa/rich-13.7.1.tar.gz", hash = "sha256:9be308cb1fe2f1f57d67ce99e95af38a1e2bc71ad9813b0e247cf7ffbcc3a432", size = 221248, upload-time = "2024-02-28T14:51:19.472Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/87/67/a37f6214d0e9fe57f6ae54b2956d550ca8365857f42a1ce0392bb21d9410/rich-13.7.1-py3-none-any.whl", hash = "sha256:4edbae314f59eb482f54e9e30bf00d33350aaa94f4bfcd4e9e3110e64d0d7222", size = 240681, upload-time = "2024-02-28T14:51:14.353Z" },
        ]

        [[package]]
        name = "urllib3"
        version = "2.2.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7a/50/7fd50a27caa0652cd4caf224aa87741ea41d3265ad13f010886167cfcc79/urllib3-2.2.1.tar.gz", hash = "sha256:d0570876c61ab9e520d776c38acbbb5b05a776d3f9ff98a5c8fd5162a444cf19", size = 291020, upload-time = "2024-02-18T03:55:57.539Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a2/73/a68704750a7679d0b6d3ad7aa8d4da8e14e151ae82e6fee774e6e0d05ec8/urllib3-2.2.1-py3-none-any.whl", hash = "sha256:450b20ec296a467077128bff42b73080516e71b56ff59a60a02bef2232c4fa9d", size = 121067, upload-time = "2024-02-18T03:55:54.704Z" },
        ]
        "#
        );
    });

    // Adding to a locked script should update the lockfile.
    uv_snapshot!(context.filters(), context.add().arg("anyio").arg("--script").arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    ");

    let script_content = context.read("script.py");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio>=4.3.0",
        #   "requests<3",
        #   "rich",
        # ]
        # ///

        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
        "#
        );
    });

    let lock = context.read("script.py.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.11"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [
            { name = "anyio", specifier = ">=4.3.0" },
            { name = "requests", specifier = "<3" },
            { name = "rich" },
        ]

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642, upload-time = "2024-02-19T08:36:28.641Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584, upload-time = "2024-02-19T08:36:26.842Z" },
        ]

        [[package]]
        name = "certifi"
        version = "2024.2.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/71/da/e94e26401b62acd6d91df2b52954aceb7f561743aa5ccc32152886c76c96/certifi-2024.2.2.tar.gz", hash = "sha256:0569859f95fc761b18b45ef421b1290a0f65f147e92a1e5eb3e635f9a5e4e66f", size = 164886, upload-time = "2024-02-02T01:22:17.364Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ba/06/a07f096c664aeb9f01624f858c3add0a4e913d6c96257acb4fce61e7de14/certifi-2024.2.2-py3-none-any.whl", hash = "sha256:dc383c07b76109f368f6106eee2b593b04a011ea4d55f652c6ca24a754d1cdd1", size = 163774, upload-time = "2024-02-02T01:22:14.86Z" },
        ]

        [[package]]
        name = "charset-normalizer"
        version = "3.3.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/63/09/c1bc53dab74b1816a00d8d030de5bf98f724c52c1635e07681d312f20be8/charset-normalizer-3.3.2.tar.gz", hash = "sha256:f30c3cb33b24454a82faecaf01b19c18562b1e89558fb6c56de4d9118a032fd5", size = 104809, upload-time = "2023-11-01T04:04:59.997Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/77/02839016f6fbbf808e8b38601df6e0e66c17bbab76dff4613f7511413597/charset_normalizer-3.3.2-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:802fe99cca7457642125a8a88a084cef28ff0cf9407060f7b93dca5aa25480db", size = 191647, upload-time = "2023-11-01T04:02:55.329Z" },
            { url = "https://files.pythonhosted.org/packages/3e/33/21a875a61057165e92227466e54ee076b73af1e21fe1b31f1e292251aa1e/charset_normalizer-3.3.2-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:573f6eac48f4769d667c4442081b1794f52919e7edada77495aaed9236d13a96", size = 121434, upload-time = "2023-11-01T04:02:57.173Z" },
            { url = "https://files.pythonhosted.org/packages/dd/51/68b61b90b24ca35495956b718f35a9756ef7d3dd4b3c1508056fa98d1a1b/charset_normalizer-3.3.2-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:549a3a73da901d5bc3ce8d24e0600d1fa85524c10287f6004fbab87672bf3e1e", size = 118979, upload-time = "2023-11-01T04:02:58.442Z" },
            { url = "https://files.pythonhosted.org/packages/e4/a6/7ee57823d46331ddc37dd00749c95b0edec2c79b15fc0d6e6efb532e89ac/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:f27273b60488abe721a075bcca6d7f3964f9f6f067c8c4c605743023d7d3944f", size = 136582, upload-time = "2023-11-01T04:02:59.776Z" },
            { url = "https://files.pythonhosted.org/packages/74/f1/0d9fe69ac441467b737ba7f48c68241487df2f4522dd7246d9426e7c690e/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:1ceae2f17a9c33cb48e3263960dc5fc8005351ee19db217e9b1bb15d28c02574", size = 146645, upload-time = "2023-11-01T04:03:02.186Z" },
            { url = "https://files.pythonhosted.org/packages/05/31/e1f51c76db7be1d4aef220d29fbfa5dbb4a99165d9833dcbf166753b6dc0/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:65f6f63034100ead094b8744b3b97965785388f308a64cf8d7c34f2f2e5be0c4", size = 139398, upload-time = "2023-11-01T04:03:04.255Z" },
            { url = "https://files.pythonhosted.org/packages/40/26/f35951c45070edc957ba40a5b1db3cf60a9dbb1b350c2d5bef03e01e61de/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:753f10e867343b4511128c6ed8c82f7bec3bd026875576dfd88483c5c73b2fd8", size = 140273, upload-time = "2023-11-01T04:03:05.983Z" },
            { url = "https://files.pythonhosted.org/packages/07/07/7e554f2bbce3295e191f7e653ff15d55309a9ca40d0362fcdab36f01063c/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:4a78b2b446bd7c934f5dcedc588903fb2f5eec172f3d29e52a9096a43722adfc", size = 142577, upload-time = "2023-11-01T04:03:07.567Z" },
            { url = "https://files.pythonhosted.org/packages/d8/b5/eb705c313100defa57da79277d9207dc8d8e45931035862fa64b625bfead/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_aarch64.whl", hash = "sha256:e537484df0d8f426ce2afb2d0f8e1c3d0b114b83f8850e5f2fbea0e797bd82ae", size = 137747, upload-time = "2023-11-01T04:03:08.886Z" },
            { url = "https://files.pythonhosted.org/packages/19/28/573147271fd041d351b438a5665be8223f1dd92f273713cb882ddafe214c/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_i686.whl", hash = "sha256:eb6904c354526e758fda7167b33005998fb68c46fbc10e013ca97f21ca5c8887", size = 143375, upload-time = "2023-11-01T04:03:10.613Z" },
            { url = "https://files.pythonhosted.org/packages/cf/7c/f3b682fa053cc21373c9a839e6beba7705857075686a05c72e0f8c4980ca/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_ppc64le.whl", hash = "sha256:deb6be0ac38ece9ba87dea880e438f25ca3eddfac8b002a2ec3d9183a454e8ae", size = 148474, upload-time = "2023-11-01T04:03:11.973Z" },
            { url = "https://files.pythonhosted.org/packages/1e/49/7ab74d4ac537ece3bc3334ee08645e231f39f7d6df6347b29a74b0537103/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_s390x.whl", hash = "sha256:4ab2fe47fae9e0f9dee8c04187ce5d09f48eabe611be8259444906793ab7cbce", size = 140232, upload-time = "2023-11-01T04:03:13.505Z" },
            { url = "https://files.pythonhosted.org/packages/2d/dc/9dacba68c9ac0ae781d40e1a0c0058e26302ea0660e574ddf6797a0347f7/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:80402cd6ee291dcb72644d6eac93785fe2c8b9cb30893c1af5b8fdd753b9d40f", size = 140859, upload-time = "2023-11-01T04:03:17.362Z" },
            { url = "https://files.pythonhosted.org/packages/6c/c2/4a583f800c0708dd22096298e49f887b49d9746d0e78bfc1d7e29816614c/charset_normalizer-3.3.2-cp311-cp311-win32.whl", hash = "sha256:7cd13a2e3ddeed6913a65e66e94b51d80a041145a026c27e6bb76c31a853c6ab", size = 92509, upload-time = "2023-11-01T04:03:21.453Z" },
            { url = "https://files.pythonhosted.org/packages/57/ec/80c8d48ac8b1741d5b963797b7c0c869335619e13d4744ca2f67fc11c6fc/charset_normalizer-3.3.2-cp311-cp311-win_amd64.whl", hash = "sha256:663946639d296df6a2bb2aa51b60a2454ca1cb29835324c640dafb5ff2131a77", size = 99870, upload-time = "2023-11-01T04:03:22.723Z" },
            { url = "https://files.pythonhosted.org/packages/d1/b2/fcedc8255ec42afee97f9e6f0145c734bbe104aac28300214593eb326f1d/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:0b2b64d2bb6d3fb9112bafa732def486049e63de9618b5843bcdd081d8144cd8", size = 192892, upload-time = "2023-11-01T04:03:24.135Z" },
            { url = "https://files.pythonhosted.org/packages/2e/7d/2259318c202f3d17f3fe6438149b3b9e706d1070fe3fcbb28049730bb25c/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:ddbb2551d7e0102e7252db79ba445cdab71b26640817ab1e3e3648dad515003b", size = 122213, upload-time = "2023-11-01T04:03:25.66Z" },
            { url = "https://files.pythonhosted.org/packages/3a/52/9f9d17c3b54dc238de384c4cb5a2ef0e27985b42a0e5cc8e8a31d918d48d/charset_normalizer-3.3.2-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:55086ee1064215781fff39a1af09518bc9255b50d6333f2e4c74ca09fac6a8f6", size = 119404, upload-time = "2023-11-01T04:03:27.04Z" },
            { url = "https://files.pythonhosted.org/packages/99/b0/9c365f6d79a9f0f3c379ddb40a256a67aa69c59609608fe7feb6235896e1/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:8f4a014bc36d3c57402e2977dada34f9c12300af536839dc38c0beab8878f38a", size = 137275, upload-time = "2023-11-01T04:03:28.466Z" },
            { url = "https://files.pythonhosted.org/packages/91/33/749df346e93d7a30cdcb90cbfdd41a06026317bfbfb62cd68307c1a3c543/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:a10af20b82360ab00827f916a6058451b723b4e65030c5a18577c8b2de5b3389", size = 147518, upload-time = "2023-11-01T04:03:29.82Z" },
            { url = "https://files.pythonhosted.org/packages/72/1a/641d5c9f59e6af4c7b53da463d07600a695b9824e20849cb6eea8a627761/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:8d756e44e94489e49571086ef83b2bb8ce311e730092d2c34ca8f7d925cb20aa", size = 140182, upload-time = "2023-11-01T04:03:31.511Z" },
            { url = "https://files.pythonhosted.org/packages/ee/fb/14d30eb4956408ee3ae09ad34299131fb383c47df355ddb428a7331cfa1e/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:90d558489962fd4918143277a773316e56c72da56ec7aa3dc3dbbe20fdfed15b", size = 141869, upload-time = "2023-11-01T04:03:32.887Z" },
            { url = "https://files.pythonhosted.org/packages/df/3e/a06b18788ca2eb6695c9b22325b6fde7dde0f1d1838b1792a0076f58fe9d/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:6ac7ffc7ad6d040517be39eb591cac5ff87416c2537df6ba3cba3bae290c0fed", size = 144042, upload-time = "2023-11-01T04:03:34.412Z" },
            { url = "https://files.pythonhosted.org/packages/45/59/3d27019d3b447a88fe7e7d004a1e04be220227760264cc41b405e863891b/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:7ed9e526742851e8d5cc9e6cf41427dfc6068d4f5a3bb03659444b4cabf6bc26", size = 138275, upload-time = "2023-11-01T04:03:35.759Z" },
            { url = "https://files.pythonhosted.org/packages/7b/ef/5eb105530b4da8ae37d506ccfa25057961b7b63d581def6f99165ea89c7e/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:8bdb58ff7ba23002a4c5808d608e4e6c687175724f54a5dade5fa8c67b604e4d", size = 144819, upload-time = "2023-11-01T04:03:37.216Z" },
            { url = "https://files.pythonhosted.org/packages/a2/51/e5023f937d7f307c948ed3e5c29c4b7a3e42ed2ee0b8cdf8f3a706089bf0/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_ppc64le.whl", hash = "sha256:6b3251890fff30ee142c44144871185dbe13b11bab478a88887a639655be1068", size = 149415, upload-time = "2023-11-01T04:03:38.694Z" },
            { url = "https://files.pythonhosted.org/packages/24/9d/2e3ef673dfd5be0154b20363c5cdcc5606f35666544381bee15af3778239/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_s390x.whl", hash = "sha256:b4a23f61ce87adf89be746c8a8974fe1c823c891d8f86eb218bb957c924bb143", size = 141212, upload-time = "2023-11-01T04:03:40.07Z" },
            { url = "https://files.pythonhosted.org/packages/5b/ae/ce2c12fcac59cb3860b2e2d76dc405253a4475436b1861d95fe75bdea520/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:efcb3f6676480691518c177e3b465bcddf57cea040302f9f4e6e191af91174d4", size = 142167, upload-time = "2023-11-01T04:03:41.491Z" },
            { url = "https://files.pythonhosted.org/packages/ed/3a/a448bf035dce5da359daf9ae8a16b8a39623cc395a2ffb1620aa1bce62b0/charset_normalizer-3.3.2-cp312-cp312-win32.whl", hash = "sha256:d965bba47ddeec8cd560687584e88cf699fd28f192ceb452d1d7ee807c5597b7", size = 93041, upload-time = "2023-11-01T04:03:42.836Z" },
            { url = "https://files.pythonhosted.org/packages/b6/7c/8debebb4f90174074b827c63242c23851bdf00a532489fba57fef3416e40/charset_normalizer-3.3.2-cp312-cp312-win_amd64.whl", hash = "sha256:96b02a3dc4381e5494fad39be677abcb5e6634bf7b4fa83a6dd3112607547001", size = 100397, upload-time = "2023-11-01T04:03:44.467Z" },
            { url = "https://files.pythonhosted.org/packages/28/76/e6222113b83e3622caa4bb41032d0b1bf785250607392e1b778aca0b8a7d/charset_normalizer-3.3.2-py3-none-any.whl", hash = "sha256:3e4d1f6587322d2788836a99c69062fbb091331ec940e02d12d179c1d53e25fc", size = 48543, upload-time = "2023-11-01T04:04:58.622Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "markdown-it-py"
        version = "3.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "mdurl" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/38/71/3b932df36c1a044d397a1f92d1cf91ee0a503d91e470cbd670aa66b07ed0/markdown-it-py-3.0.0.tar.gz", hash = "sha256:e3f60a94fa066dc52ec76661e37c851cb232d92f9886b15cb560aaada2df8feb", size = 74596, upload-time = "2023-06-03T06:41:14.443Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/42/d7/1ec15b46af6af88f19b8e5ffea08fa375d433c998b8a7639e76935c14f1f/markdown_it_py-3.0.0-py3-none-any.whl", hash = "sha256:355216845c60bd96232cd8d8c40e8f9765cc86f46880e43a8fd22dc1a1a8cab1", size = 87528, upload-time = "2023-06-03T06:41:11.019Z" },
        ]

        [[package]]
        name = "mdurl"
        version = "0.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d6/54/cfe61301667036ec958cb99bd3efefba235e65cdeb9c84d24a8293ba1d90/mdurl-0.1.2.tar.gz", hash = "sha256:bb413d29f5eea38f31dd4754dd7377d4465116fb207585f97bf925588687c1ba", size = 8729, upload-time = "2022-08-14T12:40:10.846Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b3/38/89ba8ad64ae25be8de66a6d463314cf1eb366222074cfda9ee839c56a4b4/mdurl-0.1.2-py3-none-any.whl", hash = "sha256:84008a41e51615a49fc9966191ff91509e3c40b939176e643fd50a5c2196b8f8", size = 9979, upload-time = "2022-08-14T12:40:09.779Z" },
        ]

        [[package]]
        name = "pygments"
        version = "2.17.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/55/59/8bccf4157baf25e4aa5a0bb7fa3ba8600907de105ebc22b0c78cfbf6f565/pygments-2.17.2.tar.gz", hash = "sha256:da46cec9fd2de5be3a8a784f434e4c4ab670b4ff54d605c4c2717e9d49c4c367", size = 4827772, upload-time = "2023-11-21T20:43:53.875Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/97/9c/372fef8377a6e340b1704768d20daaded98bf13282b5327beb2e2fe2c7ef/pygments-2.17.2-py3-none-any.whl", hash = "sha256:b27c2826c47d0f3219f29554824c30c5e8945175d888647acd804ddd04af846c", size = 1179756, upload-time = "2023-11-21T20:43:49.423Z" },
        ]

        [[package]]
        name = "requests"
        version = "2.31.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "certifi" },
            { name = "charset-normalizer" },
            { name = "idna" },
            { name = "urllib3" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9d/be/10918a2eac4ae9f02f6cfe6414b7a155ccd8f7f9d4380d62fd5b955065c3/requests-2.31.0.tar.gz", hash = "sha256:942c5a758f98d790eaed1a29cb6eefc7ffb0d1cf7af05c3d2791656dbd6ad1e1", size = 110794, upload-time = "2023-05-22T15:12:44.175Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/70/8e/0e2d847013cb52cd35b38c009bb167a1a26b2ce6cd6965bf26b47bc0bf44/requests-2.31.0-py3-none-any.whl", hash = "sha256:58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f", size = 62574, upload-time = "2023-05-22T15:12:42.313Z" },
        ]

        [[package]]
        name = "rich"
        version = "13.7.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markdown-it-py" },
            { name = "pygments" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b3/01/c954e134dc440ab5f96952fe52b4fdc64225530320a910473c1fe270d9aa/rich-13.7.1.tar.gz", hash = "sha256:9be308cb1fe2f1f57d67ce99e95af38a1e2bc71ad9813b0e247cf7ffbcc3a432", size = 221248, upload-time = "2024-02-28T14:51:19.472Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/87/67/a37f6214d0e9fe57f6ae54b2956d550ca8365857f42a1ce0392bb21d9410/rich-13.7.1-py3-none-any.whl", hash = "sha256:4edbae314f59eb482f54e9e30bf00d33350aaa94f4bfcd4e9e3110e64d0d7222", size = 240681, upload-time = "2024-02-28T14:51:14.353Z" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]

        [[package]]
        name = "urllib3"
        version = "2.2.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7a/50/7fd50a27caa0652cd4caf224aa87741ea41d3265ad13f010886167cfcc79/urllib3-2.2.1.tar.gz", hash = "sha256:d0570876c61ab9e520d776c38acbbb5b05a776d3f9ff98a5c8fd5162a444cf19", size = 291020, upload-time = "2024-02-18T03:55:57.539Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a2/73/a68704750a7679d0b6d3ad7aa8d4da8e14e151ae82e6fee774e6e0d05ec8/urllib3-2.2.1-py3-none-any.whl", hash = "sha256:450b20ec296a467077128bff42b73080516e71b56ff59a60a02bef2232c4fa9d", size = 121067, upload-time = "2024-02-18T03:55:54.704Z" },
        ]
        "#
        );
    });

    // Removing from a locked script should update the lockfile.
    uv_snapshot!(context.filters(), context.remove().arg("anyio").arg("--script").arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    ");

    let script_content = context.read("script.py");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "requests<3",
        #   "rich",
        # ]
        # ///

        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
        "#
        );
    });

    let lock = context.read("script.py.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.11"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [
            { name = "requests", specifier = "<3" },
            { name = "rich" },
        ]

        [[package]]
        name = "certifi"
        version = "2024.2.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/71/da/e94e26401b62acd6d91df2b52954aceb7f561743aa5ccc32152886c76c96/certifi-2024.2.2.tar.gz", hash = "sha256:0569859f95fc761b18b45ef421b1290a0f65f147e92a1e5eb3e635f9a5e4e66f", size = 164886, upload-time = "2024-02-02T01:22:17.364Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ba/06/a07f096c664aeb9f01624f858c3add0a4e913d6c96257acb4fce61e7de14/certifi-2024.2.2-py3-none-any.whl", hash = "sha256:dc383c07b76109f368f6106eee2b593b04a011ea4d55f652c6ca24a754d1cdd1", size = 163774, upload-time = "2024-02-02T01:22:14.86Z" },
        ]

        [[package]]
        name = "charset-normalizer"
        version = "3.3.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/63/09/c1bc53dab74b1816a00d8d030de5bf98f724c52c1635e07681d312f20be8/charset-normalizer-3.3.2.tar.gz", hash = "sha256:f30c3cb33b24454a82faecaf01b19c18562b1e89558fb6c56de4d9118a032fd5", size = 104809, upload-time = "2023-11-01T04:04:59.997Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/77/02839016f6fbbf808e8b38601df6e0e66c17bbab76dff4613f7511413597/charset_normalizer-3.3.2-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:802fe99cca7457642125a8a88a084cef28ff0cf9407060f7b93dca5aa25480db", size = 191647, upload-time = "2023-11-01T04:02:55.329Z" },
            { url = "https://files.pythonhosted.org/packages/3e/33/21a875a61057165e92227466e54ee076b73af1e21fe1b31f1e292251aa1e/charset_normalizer-3.3.2-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:573f6eac48f4769d667c4442081b1794f52919e7edada77495aaed9236d13a96", size = 121434, upload-time = "2023-11-01T04:02:57.173Z" },
            { url = "https://files.pythonhosted.org/packages/dd/51/68b61b90b24ca35495956b718f35a9756ef7d3dd4b3c1508056fa98d1a1b/charset_normalizer-3.3.2-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:549a3a73da901d5bc3ce8d24e0600d1fa85524c10287f6004fbab87672bf3e1e", size = 118979, upload-time = "2023-11-01T04:02:58.442Z" },
            { url = "https://files.pythonhosted.org/packages/e4/a6/7ee57823d46331ddc37dd00749c95b0edec2c79b15fc0d6e6efb532e89ac/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:f27273b60488abe721a075bcca6d7f3964f9f6f067c8c4c605743023d7d3944f", size = 136582, upload-time = "2023-11-01T04:02:59.776Z" },
            { url = "https://files.pythonhosted.org/packages/74/f1/0d9fe69ac441467b737ba7f48c68241487df2f4522dd7246d9426e7c690e/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:1ceae2f17a9c33cb48e3263960dc5fc8005351ee19db217e9b1bb15d28c02574", size = 146645, upload-time = "2023-11-01T04:03:02.186Z" },
            { url = "https://files.pythonhosted.org/packages/05/31/e1f51c76db7be1d4aef220d29fbfa5dbb4a99165d9833dcbf166753b6dc0/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:65f6f63034100ead094b8744b3b97965785388f308a64cf8d7c34f2f2e5be0c4", size = 139398, upload-time = "2023-11-01T04:03:04.255Z" },
            { url = "https://files.pythonhosted.org/packages/40/26/f35951c45070edc957ba40a5b1db3cf60a9dbb1b350c2d5bef03e01e61de/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:753f10e867343b4511128c6ed8c82f7bec3bd026875576dfd88483c5c73b2fd8", size = 140273, upload-time = "2023-11-01T04:03:05.983Z" },
            { url = "https://files.pythonhosted.org/packages/07/07/7e554f2bbce3295e191f7e653ff15d55309a9ca40d0362fcdab36f01063c/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:4a78b2b446bd7c934f5dcedc588903fb2f5eec172f3d29e52a9096a43722adfc", size = 142577, upload-time = "2023-11-01T04:03:07.567Z" },
            { url = "https://files.pythonhosted.org/packages/d8/b5/eb705c313100defa57da79277d9207dc8d8e45931035862fa64b625bfead/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_aarch64.whl", hash = "sha256:e537484df0d8f426ce2afb2d0f8e1c3d0b114b83f8850e5f2fbea0e797bd82ae", size = 137747, upload-time = "2023-11-01T04:03:08.886Z" },
            { url = "https://files.pythonhosted.org/packages/19/28/573147271fd041d351b438a5665be8223f1dd92f273713cb882ddafe214c/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_i686.whl", hash = "sha256:eb6904c354526e758fda7167b33005998fb68c46fbc10e013ca97f21ca5c8887", size = 143375, upload-time = "2023-11-01T04:03:10.613Z" },
            { url = "https://files.pythonhosted.org/packages/cf/7c/f3b682fa053cc21373c9a839e6beba7705857075686a05c72e0f8c4980ca/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_ppc64le.whl", hash = "sha256:deb6be0ac38ece9ba87dea880e438f25ca3eddfac8b002a2ec3d9183a454e8ae", size = 148474, upload-time = "2023-11-01T04:03:11.973Z" },
            { url = "https://files.pythonhosted.org/packages/1e/49/7ab74d4ac537ece3bc3334ee08645e231f39f7d6df6347b29a74b0537103/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_s390x.whl", hash = "sha256:4ab2fe47fae9e0f9dee8c04187ce5d09f48eabe611be8259444906793ab7cbce", size = 140232, upload-time = "2023-11-01T04:03:13.505Z" },
            { url = "https://files.pythonhosted.org/packages/2d/dc/9dacba68c9ac0ae781d40e1a0c0058e26302ea0660e574ddf6797a0347f7/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:80402cd6ee291dcb72644d6eac93785fe2c8b9cb30893c1af5b8fdd753b9d40f", size = 140859, upload-time = "2023-11-01T04:03:17.362Z" },
            { url = "https://files.pythonhosted.org/packages/6c/c2/4a583f800c0708dd22096298e49f887b49d9746d0e78bfc1d7e29816614c/charset_normalizer-3.3.2-cp311-cp311-win32.whl", hash = "sha256:7cd13a2e3ddeed6913a65e66e94b51d80a041145a026c27e6bb76c31a853c6ab", size = 92509, upload-time = "2023-11-01T04:03:21.453Z" },
            { url = "https://files.pythonhosted.org/packages/57/ec/80c8d48ac8b1741d5b963797b7c0c869335619e13d4744ca2f67fc11c6fc/charset_normalizer-3.3.2-cp311-cp311-win_amd64.whl", hash = "sha256:663946639d296df6a2bb2aa51b60a2454ca1cb29835324c640dafb5ff2131a77", size = 99870, upload-time = "2023-11-01T04:03:22.723Z" },
            { url = "https://files.pythonhosted.org/packages/d1/b2/fcedc8255ec42afee97f9e6f0145c734bbe104aac28300214593eb326f1d/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:0b2b64d2bb6d3fb9112bafa732def486049e63de9618b5843bcdd081d8144cd8", size = 192892, upload-time = "2023-11-01T04:03:24.135Z" },
            { url = "https://files.pythonhosted.org/packages/2e/7d/2259318c202f3d17f3fe6438149b3b9e706d1070fe3fcbb28049730bb25c/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:ddbb2551d7e0102e7252db79ba445cdab71b26640817ab1e3e3648dad515003b", size = 122213, upload-time = "2023-11-01T04:03:25.66Z" },
            { url = "https://files.pythonhosted.org/packages/3a/52/9f9d17c3b54dc238de384c4cb5a2ef0e27985b42a0e5cc8e8a31d918d48d/charset_normalizer-3.3.2-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:55086ee1064215781fff39a1af09518bc9255b50d6333f2e4c74ca09fac6a8f6", size = 119404, upload-time = "2023-11-01T04:03:27.04Z" },
            { url = "https://files.pythonhosted.org/packages/99/b0/9c365f6d79a9f0f3c379ddb40a256a67aa69c59609608fe7feb6235896e1/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:8f4a014bc36d3c57402e2977dada34f9c12300af536839dc38c0beab8878f38a", size = 137275, upload-time = "2023-11-01T04:03:28.466Z" },
            { url = "https://files.pythonhosted.org/packages/91/33/749df346e93d7a30cdcb90cbfdd41a06026317bfbfb62cd68307c1a3c543/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:a10af20b82360ab00827f916a6058451b723b4e65030c5a18577c8b2de5b3389", size = 147518, upload-time = "2023-11-01T04:03:29.82Z" },
            { url = "https://files.pythonhosted.org/packages/72/1a/641d5c9f59e6af4c7b53da463d07600a695b9824e20849cb6eea8a627761/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:8d756e44e94489e49571086ef83b2bb8ce311e730092d2c34ca8f7d925cb20aa", size = 140182, upload-time = "2023-11-01T04:03:31.511Z" },
            { url = "https://files.pythonhosted.org/packages/ee/fb/14d30eb4956408ee3ae09ad34299131fb383c47df355ddb428a7331cfa1e/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:90d558489962fd4918143277a773316e56c72da56ec7aa3dc3dbbe20fdfed15b", size = 141869, upload-time = "2023-11-01T04:03:32.887Z" },
            { url = "https://files.pythonhosted.org/packages/df/3e/a06b18788ca2eb6695c9b22325b6fde7dde0f1d1838b1792a0076f58fe9d/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:6ac7ffc7ad6d040517be39eb591cac5ff87416c2537df6ba3cba3bae290c0fed", size = 144042, upload-time = "2023-11-01T04:03:34.412Z" },
            { url = "https://files.pythonhosted.org/packages/45/59/3d27019d3b447a88fe7e7d004a1e04be220227760264cc41b405e863891b/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:7ed9e526742851e8d5cc9e6cf41427dfc6068d4f5a3bb03659444b4cabf6bc26", size = 138275, upload-time = "2023-11-01T04:03:35.759Z" },
            { url = "https://files.pythonhosted.org/packages/7b/ef/5eb105530b4da8ae37d506ccfa25057961b7b63d581def6f99165ea89c7e/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:8bdb58ff7ba23002a4c5808d608e4e6c687175724f54a5dade5fa8c67b604e4d", size = 144819, upload-time = "2023-11-01T04:03:37.216Z" },
            { url = "https://files.pythonhosted.org/packages/a2/51/e5023f937d7f307c948ed3e5c29c4b7a3e42ed2ee0b8cdf8f3a706089bf0/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_ppc64le.whl", hash = "sha256:6b3251890fff30ee142c44144871185dbe13b11bab478a88887a639655be1068", size = 149415, upload-time = "2023-11-01T04:03:38.694Z" },
            { url = "https://files.pythonhosted.org/packages/24/9d/2e3ef673dfd5be0154b20363c5cdcc5606f35666544381bee15af3778239/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_s390x.whl", hash = "sha256:b4a23f61ce87adf89be746c8a8974fe1c823c891d8f86eb218bb957c924bb143", size = 141212, upload-time = "2023-11-01T04:03:40.07Z" },
            { url = "https://files.pythonhosted.org/packages/5b/ae/ce2c12fcac59cb3860b2e2d76dc405253a4475436b1861d95fe75bdea520/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:efcb3f6676480691518c177e3b465bcddf57cea040302f9f4e6e191af91174d4", size = 142167, upload-time = "2023-11-01T04:03:41.491Z" },
            { url = "https://files.pythonhosted.org/packages/ed/3a/a448bf035dce5da359daf9ae8a16b8a39623cc395a2ffb1620aa1bce62b0/charset_normalizer-3.3.2-cp312-cp312-win32.whl", hash = "sha256:d965bba47ddeec8cd560687584e88cf699fd28f192ceb452d1d7ee807c5597b7", size = 93041, upload-time = "2023-11-01T04:03:42.836Z" },
            { url = "https://files.pythonhosted.org/packages/b6/7c/8debebb4f90174074b827c63242c23851bdf00a532489fba57fef3416e40/charset_normalizer-3.3.2-cp312-cp312-win_amd64.whl", hash = "sha256:96b02a3dc4381e5494fad39be677abcb5e6634bf7b4fa83a6dd3112607547001", size = 100397, upload-time = "2023-11-01T04:03:44.467Z" },
            { url = "https://files.pythonhosted.org/packages/28/76/e6222113b83e3622caa4bb41032d0b1bf785250607392e1b778aca0b8a7d/charset_normalizer-3.3.2-py3-none-any.whl", hash = "sha256:3e4d1f6587322d2788836a99c69062fbb091331ec940e02d12d179c1d53e25fc", size = 48543, upload-time = "2023-11-01T04:04:58.622Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "markdown-it-py"
        version = "3.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "mdurl" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/38/71/3b932df36c1a044d397a1f92d1cf91ee0a503d91e470cbd670aa66b07ed0/markdown-it-py-3.0.0.tar.gz", hash = "sha256:e3f60a94fa066dc52ec76661e37c851cb232d92f9886b15cb560aaada2df8feb", size = 74596, upload-time = "2023-06-03T06:41:14.443Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/42/d7/1ec15b46af6af88f19b8e5ffea08fa375d433c998b8a7639e76935c14f1f/markdown_it_py-3.0.0-py3-none-any.whl", hash = "sha256:355216845c60bd96232cd8d8c40e8f9765cc86f46880e43a8fd22dc1a1a8cab1", size = 87528, upload-time = "2023-06-03T06:41:11.019Z" },
        ]

        [[package]]
        name = "mdurl"
        version = "0.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d6/54/cfe61301667036ec958cb99bd3efefba235e65cdeb9c84d24a8293ba1d90/mdurl-0.1.2.tar.gz", hash = "sha256:bb413d29f5eea38f31dd4754dd7377d4465116fb207585f97bf925588687c1ba", size = 8729, upload-time = "2022-08-14T12:40:10.846Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b3/38/89ba8ad64ae25be8de66a6d463314cf1eb366222074cfda9ee839c56a4b4/mdurl-0.1.2-py3-none-any.whl", hash = "sha256:84008a41e51615a49fc9966191ff91509e3c40b939176e643fd50a5c2196b8f8", size = 9979, upload-time = "2022-08-14T12:40:09.779Z" },
        ]

        [[package]]
        name = "pygments"
        version = "2.17.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/55/59/8bccf4157baf25e4aa5a0bb7fa3ba8600907de105ebc22b0c78cfbf6f565/pygments-2.17.2.tar.gz", hash = "sha256:da46cec9fd2de5be3a8a784f434e4c4ab670b4ff54d605c4c2717e9d49c4c367", size = 4827772, upload-time = "2023-11-21T20:43:53.875Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/97/9c/372fef8377a6e340b1704768d20daaded98bf13282b5327beb2e2fe2c7ef/pygments-2.17.2-py3-none-any.whl", hash = "sha256:b27c2826c47d0f3219f29554824c30c5e8945175d888647acd804ddd04af846c", size = 1179756, upload-time = "2023-11-21T20:43:49.423Z" },
        ]

        [[package]]
        name = "requests"
        version = "2.31.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "certifi" },
            { name = "charset-normalizer" },
            { name = "idna" },
            { name = "urllib3" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9d/be/10918a2eac4ae9f02f6cfe6414b7a155ccd8f7f9d4380d62fd5b955065c3/requests-2.31.0.tar.gz", hash = "sha256:942c5a758f98d790eaed1a29cb6eefc7ffb0d1cf7af05c3d2791656dbd6ad1e1", size = 110794, upload-time = "2023-05-22T15:12:44.175Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/70/8e/0e2d847013cb52cd35b38c009bb167a1a26b2ce6cd6965bf26b47bc0bf44/requests-2.31.0-py3-none-any.whl", hash = "sha256:58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f", size = 62574, upload-time = "2023-05-22T15:12:42.313Z" },
        ]

        [[package]]
        name = "rich"
        version = "13.7.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markdown-it-py" },
            { name = "pygments" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b3/01/c954e134dc440ab5f96952fe52b4fdc64225530320a910473c1fe270d9aa/rich-13.7.1.tar.gz", hash = "sha256:9be308cb1fe2f1f57d67ce99e95af38a1e2bc71ad9813b0e247cf7ffbcc3a432", size = 221248, upload-time = "2024-02-28T14:51:19.472Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/87/67/a37f6214d0e9fe57f6ae54b2956d550ca8365857f42a1ce0392bb21d9410/rich-13.7.1-py3-none-any.whl", hash = "sha256:4edbae314f59eb482f54e9e30bf00d33350aaa94f4bfcd4e9e3110e64d0d7222", size = 240681, upload-time = "2024-02-28T14:51:14.353Z" },
        ]

        [[package]]
        name = "urllib3"
        version = "2.2.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7a/50/7fd50a27caa0652cd4caf224aa87741ea41d3265ad13f010886167cfcc79/urllib3-2.2.1.tar.gz", hash = "sha256:d0570876c61ab9e520d776c38acbbb5b05a776d3f9ff98a5c8fd5162a444cf19", size = 291020, upload-time = "2024-02-18T03:55:57.539Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a2/73/a68704750a7679d0b6d3ad7aa8d4da8e14e151ae82e6fee774e6e0d05ec8/urllib3-2.2.1-py3-none-any.whl", hash = "sha256:450b20ec296a467077128bff42b73080516e71b56ff59a60a02bef2232c4fa9d", size = 121067, upload-time = "2024-02-18T03:55:54.704Z" },
        ]
        "#
        );
    });

    Ok(())
}

/// Remove from a PEP 723 script.
#[test]
fn remove_script() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "requests<3",
        #   "rich",
        #   "anyio",
        # ]
        # ///

        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
    "#})?;

    uv_snapshot!(context.filters(), context.remove().arg("anyio").arg("--script").arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updated `script.py`
    ");

    let script_content = context.read("script.py");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "requests<3",
        #   "rich",
        # ]
        # ///

        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
        "#
        );
    });
    Ok(())
}

/// Remove last dependency PEP 723 script
#[test]
fn remove_last_dep_script() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "rich",
        # ]
        # ///

        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
    "#})?;

    uv_snapshot!(context.filters(), context.remove().arg("rich").arg("--script").arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Updated `script.py`
    ");

    let script_content = context.read("script.py");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = []
        # ///

        import requests
        from rich.pretty import pprint

        resp = requests.get("https://peps.python.org/api/peps.json")
        data = resp.json()
        pprint([(k, v["title"]) for k, v in data.items()][:10])
        "#
        );
    });
    Ok(())
}

/// Add a Git requirement to PEP 723 script.
#[test]
#[cfg(feature = "test-git")]
fn add_git_to_script() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio",
        # ]
        # ///

        import anyio
        import uv_public_pypackage
    "#})?;

    uv_snapshot!(context.filters(), context
        .add()
        .arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage")
        .arg("--tag=0.0.1")
        .arg("--script")
        .arg("script.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    let script_content = context.read("script.py");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            script_content, @r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio",
        #   "uv-public-pypackage",
        # ]
        #
        # [tool.uv.sources]
        # uv-public-pypackage = { git = "https://github.com/astral-test/uv-public-pypackage", tag = "0.0.1" }
        # ///

        import anyio
        import uv_public_pypackage
        "#
        );
    });

    // Ensure that the script runs without error.
    context.run().arg("script.py").assert().success();

    Ok(())
}

#[test]
fn add_include_default_groups() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        foo = ["anyio"]

        [tool.uv]
        default-groups = ["foo"]
    "#})?;

    // add should install default groups.
    uv_snapshot!(context.filters(), context.add().arg("typing-extensions"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "typing-extensions>=4.10.0",
        ]

        [dependency-groups]
        foo = ["anyio"]

        [tool.uv]
        default-groups = ["foo"]
        "#
        );
    });

    assert!(context.temp_dir.join("uv.lock").exists());

    Ok(())
}

#[test]
fn remove_include_default_groups() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "typing-extensions>=4.10.0",
        ]

        [dependency-groups]
        dev = ["anyio"]
    "#})?;

    // remove should install default groups.
    uv_snapshot!(context.filters(), context.remove().arg("typing-extensions"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        dev = ["anyio"]
        "#
        );
    });

    assert!(context.temp_dir.join("uv.lock").exists());

    Ok(())
}

/// Revert changes to the `pyproject.toml` and `uv.lock` when the `add` operation fails.
#[test]
fn fail_to_add_revert_project() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Add a dependency on a package that declares static metadata (so can always resolve), but
    // can't be installed.
    let pyproject_toml = context.temp_dir.child("child/pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools"]
        build-backend = "setuptools.build_meta"
    "#})?;
    context
        .temp_dir
        .child("child")
        .child("setup.py")
        .write_str("1/0")?;

    uv_snapshot!(context.filters(), context.add().arg("./child").arg("--no-workspace"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
      × Failed to build `child @ file://[TEMP_DIR]/child`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta.build_wheel` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 14, in <module>
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 325, in get_requires_for_build_wheel
              return self._get_build_requires(config_settings, requirements=['wheel'])
                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 295, in _get_build_requires
              self.run_setup()
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 311, in run_setup
              exec(code, locals())
            File "<string>", line 1, in <module>
          ZeroDivisionError: division by zero

          hint: This usually indicates a problem with the package or the build environment.
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "#);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        "#
        );
    });

    // The lockfile should not exist, even though resolution succeeded.
    assert!(!context.temp_dir.join("uv.lock").exists());

    Ok(())
}

/// Revert changes to the `pyproject.toml` and `uv.lock` when the `add` operation fails.
///
/// In this case, the project has an existing lockfile.
#[test]
fn fail_to_edit_revert_project() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("iniconfig"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let before = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    // Add a dependency on a package that declares static metadata (so can always resolve), but
    // can't be installed.
    let pyproject_toml = context.temp_dir.child("child/pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools"]
        build-backend = "setuptools.build_meta"
    "#})?;
    context
        .temp_dir
        .child("child")
        .child("setup.py")
        .write_str("1/0")?;

    uv_snapshot!(context.filters(), context.add().arg("./child").arg("--no-workspace"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
      × Failed to build `child @ file://[TEMP_DIR]/child`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta.build_wheel` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 14, in <module>
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 325, in get_requires_for_build_wheel
              return self._get_build_requires(config_settings, requirements=['wheel'])
                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 295, in _get_build_requires
              self.run_setup()
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 311, in run_setup
              exec(code, locals())
            File "<string>", line 1, in <module>
          ZeroDivisionError: division by zero

          hint: This usually indicates a problem with the package or the build environment.
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "#);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig>=2.0.0",
        ]
        "#
        );
    });

    // The lockfile should exist, but be unchanged.
    let after = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    assert_eq!(before, after);

    Ok(())
}

/// Revert changes to the root `pyproject.toml` and `uv.lock` when the `add` operation fails.
#[test]
fn fail_to_add_revert_workspace_root() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Add a dependency on a package that declares static metadata (so can always resolve), but
    // can't be installed.
    let pyproject_toml = context.temp_dir.child("child/pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools"]
        build-backend = "setuptools.build_meta"
    "#})?;
    context
        .temp_dir
        .child("child")
        .child("setup.py")
        .write_str("1/0")?;

    // Add a dependency on a package that declares static metadata (so can always resolve), but
    // can't be installed.
    let pyproject_toml = context.temp_dir.child("broken").child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "broken"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools"]
        build-backend = "setuptools.build_meta"
    "#})?;
    context
        .temp_dir
        .child("broken")
        .child("setup.py")
        .write_str("1/0")?;

    uv_snapshot!(context.filters(), context.add().arg("./broken"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Added `broken` to workspace members
    Resolved 3 packages in [TIME]
      × Failed to build `broken @ file://[TEMP_DIR]/broken`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta.build_editable` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 14, in <module>
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 448, in get_requires_for_build_editable
              return self.get_requires_for_build_wheel(config_settings)
                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 325, in get_requires_for_build_wheel
              return self._get_build_requires(config_settings, requirements=['wheel'])
                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 295, in _get_build_requires
              self.run_setup()
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 311, in run_setup
              exec(code, locals())
            File "<string>", line 1, in <module>
          ZeroDivisionError: division by zero

          hint: This usually indicates a problem with the package or the build environment.
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "#);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        "#
        );
    });

    // The lockfile should not exist, even though resolution succeeded.
    assert!(!context.temp_dir.join("uv.lock").exists());

    Ok(())
}

/// Revert changes to the root `pyproject.toml` and `uv.lock` when the `add` operation fails.
#[test]
fn fail_to_add_revert_workspace_member() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [tool.uv.workspace]
        members = ["child"]

        [tool.uv.sources]
        child = { workspace = true }
    "#})?;

    // Add a workspace dependency.
    let project = context.temp_dir.child("child");
    project.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    project
        .child("src")
        .child("child")
        .child("__init__.py")
        .touch()?;

    // Add a dependency on a package that declares static metadata (so can always resolve), but
    // can't be installed.
    let pyproject_toml = context.temp_dir.child("broken/pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "broken"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools"]
        build-backend = "setuptools.build_meta"
    "#})?;
    context
        .temp_dir
        .child("broken")
        .child("setup.py")
        .write_str("1/0")?;

    uv_snapshot!(context.filters(), context.add().current_dir(&project).arg("../broken"), @r#"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Added `broken` to workspace members
    Resolved 4 packages in [TIME]
      × Failed to build `broken @ file://[TEMP_DIR]/broken`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta.build_editable` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 14, in <module>
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 448, in get_requires_for_build_editable
              return self.get_requires_for_build_wheel(config_settings)
                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 325, in get_requires_for_build_wheel
              return self._get_build_requires(config_settings, requirements=['wheel'])
                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 295, in _get_build_requires
              self.run_setup()
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 311, in run_setup
              exec(code, locals())
            File "<string>", line 1, in <module>
          ZeroDivisionError: division by zero

          hint: This usually indicates a problem with the package or the build environment.
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "#);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [tool.uv.workspace]
        members = ["child"]

        [tool.uv.sources]
        child = { workspace = true }
        "#
        );
    });

    let pyproject_toml =
        fs_err::read_to_string(context.temp_dir.join("child").join("pyproject.toml"))?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#
        );
    });

    // The lockfile should not exist, even though resolution succeeded.
    assert!(!context.temp_dir.join("uv.lock").exists());

    Ok(())
}

/// Ensure that the added dependencies are sorted if the dependency list was already sorted prior
/// to the operation.
#[test]
fn sorted_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "CacheControl[filecache]>=0.14,<0.15",
        "iniconfig",
    ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().args(["typing-extensions", "anyio"]), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 13 packages in [TIME]
    Prepared 12 packages in [TIME]
    Installed 12 packages in [TIME]
     + anyio==4.3.0
     + cachecontrol==0.14.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + filelock==3.13.1
     + idna==3.6
     + iniconfig==2.0.0
     + msgpack==1.0.8
     + requests==2.31.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
     + urllib3==2.2.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio>=4.3.0",
            "CacheControl[filecache]>=0.14,<0.15",
            "iniconfig",
            "typing-extensions>=4.10.0",
        ]
        "#
        );
    });
    Ok(())
}

/// Ensure that if the dependencies are sorted naively (i.e. by the whole
/// requirement specifier), that added dependencies are sorted in the same way.
#[test]
fn naive_sorted_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "pytest-mock>=3.14",
        "pytest>=8.1.1",
    ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().args(["pytest-randomly"]), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
     + pytest-mock==3.14.0
     + pytest-randomly==3.15.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "pytest-mock>=3.14",
            "pytest-randomly>=3.15.0",
            "pytest>=8.1.1",
        ]
        "#
        );
    });
    Ok(())
}

/// Ensure that the added dependencies are case sensitive sorted if the dependency list was already
/// case sensitive sorted prior to the operation.
#[test]
fn case_sensitive_sorted_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "CacheControl[filecache]>=0.14,<0.15",
        "PyYAML",
        "iniconfig",
    ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().args(["typing-extensions", "anyio"]), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 14 packages in [TIME]
    Prepared 13 packages in [TIME]
    Installed 13 packages in [TIME]
     + anyio==4.3.0
     + cachecontrol==0.14.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + filelock==3.13.1
     + idna==3.6
     + iniconfig==2.0.0
     + msgpack==1.0.8
     + pyyaml==6.0.1
     + requests==2.31.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
     + urllib3==2.2.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "CacheControl[filecache]>=0.14,<0.15",
            "PyYAML",
            "anyio>=4.3.0",
            "iniconfig",
            "typing-extensions>=4.10.0",
        ]
        "#
        );
    });
    Ok(())
}

/// Ensure that if the dependencies are sorted naively (i.e. by the whole
/// requirement specifier), that added dependencies are sorted in the same way.
#[test]
fn case_sensitive_naive_sorted_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "Typing-extensions>=4.10.0",
        "pytest-mock>=3.14",
        "pytest>=8.1.1",
    ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().args(["pytest-randomly"]), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
     + pytest-mock==3.14.0
     + pytest-randomly==3.15.0
     + typing-extensions==4.10.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "Typing-extensions>=4.10.0",
            "pytest-mock>=3.14",
            "pytest-randomly>=3.15.0",
            "pytest>=8.1.1",
        ]
        "#
        );
    });
    Ok(())
}

/// Ensure that sorting is based on the name, rather than the combined name-and-specifiers.
#[test]
fn sorted_dependencies_name_specifiers() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12.0"
        dependencies = [
            "pytest>=8",
            "typing-extensions>=4.10.0",
        ]
    "#})?;

    uv_snapshot!(context.filters(), universal_windows_filters=true, context.add().args(["pytest-mock"]), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
     + pytest-mock==3.14.0
     + typing-extensions==4.10.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12.[X]"
        dependencies = [
            "pytest>=8",
            "pytest-mock>=3.14.0",
            "typing-extensions>=4.10.0",
        ]
        "#
        );
    });

    uv_snapshot!(context.filters(), universal_windows_filters=true, context.add().args(["pytest-randomly"]), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + pytest-randomly==3.15.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12.[X]"
        dependencies = [
            "pytest>=8",
            "pytest-mock>=3.14.0",
            "pytest-randomly>=3.15.0",
            "typing-extensions>=4.10.0",
        ]
        "#
        );
    });

    Ok(())
}

#[test]
fn sorted_dependencies_with_include_group() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"

    [dependency-groups]
    dev = [
        { include-group = "coverage" },
        "pytest-mock>=3.14",
        "pytest>=8.1.1",
    ]
    coverage = [
        "coverage>=7.4.4",
    ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().args(["--dev", "pytest-randomly"]), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + coverage==7.4.4
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
     + pytest-mock==3.14.0
     + pytest-randomly==3.15.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [dependency-groups]
        dev = [
            { include-group = "coverage" },
            "pytest-mock>=3.14",
            "pytest-randomly>=3.15.0",
            "pytest>=8.1.1",
        ]
        coverage = [
            "coverage>=7.4.4",
        ]
        "#
        );
    });
    Ok(())
}

#[test]
fn sorted_dependencies_new_dependency_after_include_group() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"

    [dependency-groups]
    dev = [
        { include-group = "coverage" },
    ]
    coverage = [
        "coverage>=7.4.4",
    ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().args(["--dev", "pytest"]), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + coverage==7.4.4
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [dependency-groups]
        dev = [
            { include-group = "coverage" },
            "pytest>=8.1.1",
        ]
        coverage = [
            "coverage>=7.4.4",
        ]
        "#
        );
    });
    Ok(())
}

#[test]
fn sorted_dependencies_include_group_kept_at_bottom() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"

    [dependency-groups]
    dev = [
        "pytest>=8.1.1",
        { include-group = "coverage" },
    ]
    coverage = [
        "coverage>=7.4.4",
    ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().args(["--dev", "pytest-randomly"]), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + coverage==7.4.4
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
     + pytest-randomly==3.15.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [dependency-groups]
        dev = [
            "pytest>=8.1.1",
            "pytest-randomly>=3.15.0",
            { include-group = "coverage" },
        ]
        coverage = [
            "coverage>=7.4.4",
        ]
        "#
        );
    });
    Ok(())
}

/// Ensure that the custom ordering of the dependencies is preserved
/// after adding a package.
#[test]
fn custom_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "yarl",
        "CacheControl[filecache]>=0.14,<0.15",
        "mwparserfromhell",
        "pywikibot",
        "sentry-sdk",
    ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("pydantic").arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "yarl",
            "CacheControl[filecache]>=0.14,<0.15",
            "mwparserfromhell",
            "pywikibot",
            "sentry-sdk",
            "pydantic",
        ]
        "#
        );
    });
    Ok(())
}

/// Regression test for: <https://github.com/astral-sh/uv/issues/7259>
#[test]
fn update_offset() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig",
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().args(["typing-extensions", "iniconfig"]), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + typing-extensions==4.10.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig",
            "typing-extensions>=4.10.0",
        ]
        "#
        );
    });

    Ok(())
}

/// Check hint for <https://github.com/astral-sh/uv/issues/7329>
/// if there is a broken cyclic dependency on a local package.
#[test]
fn add_shadowed_name() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "dagster"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Pinned constrained, check for a direct dependency loop.
    uv_snapshot!(context.filters(), context.add().arg("dagster-webserver==1.6.13"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because dagster-webserver==1.6.13 depends on your project and your project depends on dagster-webserver==1.6.13, we can conclude that your project's requirements are unsatisfiable.

          hint: The package `dagster-webserver` depends on the package `dagster` but the name is shadowed by your project. Consider changing the name of the project.
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    ");

    // Constraint with several available versions, check for an indirect dependency loop.
    uv_snapshot!(context.filters(), context.add().arg("dagster-webserver>=1.6.11,<1.7.0"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only the following versions of dagster-webserver are available:
              dagster-webserver<=1.6.11
              dagster-webserver==1.6.12
              dagster-webserver==1.6.13
          and dagster-webserver==1.6.11 depends on your project, we can conclude that dagster-webserver>=1.6.11,<1.6.12 depends on your project.
          And because dagster-webserver==1.6.12 depends on your project, we can conclude that dagster-webserver>=1.6.11,<1.6.13 depends on your project.
          And because dagster-webserver==1.6.13 depends on your project and your project depends on dagster-webserver>=1.6.11, we can conclude that your project's requirements are unsatisfiable.

          hint: The package `dagster-webserver` depends on the package `dagster` but the name is shadowed by your project. Consider changing the name of the project.
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    ");

    Ok(())
}

/// Warn when a user provides an index via `--index-url` or `--extra-index-url`.
#[test]
fn add_warn_index_url() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("idna").arg("--index-url").arg("https://pypi.org/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Indexes specified via `--index-url` will not be persisted to the `pyproject.toml` file; use `--default-index` instead.
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==3.6
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "idna>=3.6",
        ]
        "#
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "idna" },
        ]

        [package.metadata]
        requires-dist = [{ name = "idna", specifier = ">=3.6" }]
        "#
        );
    });

    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--extra-index-url").arg("https://test.pypi.org/simple"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: Indexes specified via `--extra-index-url` will not be persisted to the `pyproject.toml` file; use `--index` instead.
      × No solution found when resolving dependencies:
      ╰─▶ Because only idna==2.7 is available and your project depends on idna>=3.6, we can conclude that your project's requirements are unsatisfiable.

          hint: `idna` was found on https://test.pypi.org/simple, but not at the requested version (idna>=3.6). A compatible version may be available on a subsequent index (e.g., https://pypi.org/simple). By default, uv will only consider versions that are published on the first index that contains a given package, to avoid dependency confusion attacks. If all indexes are equally trusted, use `--index-strategy unsafe-best-match` to consider all versions from all indexes, regardless of the order in which they were defined.
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    ");

    Ok(())
}

/// Don't warn if the user provides an index via `index-url` in `pyproject.toml`.
#[test]
fn add_no_warn_index_url() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        [tool.uv]
        index-url = "https://test.pypi.org/simple"
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("iniconfig"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig>=2.0.0",
        ]
        [tool.uv]
        index-url = "https://test.pypi.org/simple"
        "#
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://test.pypi.org/simple" }
        sdist = { url = "https://test-files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:16.826Z" }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:14.843Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = ">=2.0.0" }]
        "#
        );
    });

    Ok(())
}

/// Add an index provided via `--index`.
#[test]
fn add_index() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_exclude_newer("2025-01-30T00:00Z");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("iniconfig==2.0.0").arg("--index").arg("https://pypi.org/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig==2.0.0",
        ]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]

        [[tool.uv.index]]
        url = "https://pypi.org/simple"
        "#
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2025-01-30T00:00:00Z"

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = "==2.0.0" }]
        "#
        );
    });

    // Adding a subsequent index should put it _above_ the existing index.
    uv_snapshot!(context.filters(), context.add().arg("jinja2").arg("--index").arg("pytorch=https://astral-sh.github.io/pytorch-mirror/whl/cu121"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + jinja2==3.1.4
     + markupsafe==2.1.5
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig==2.0.0",
            "jinja2>=3.1.4",
        ]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]

        [[tool.uv.index]]
        name = "pytorch"
        url = "https://astral-sh.github.io/pytorch-mirror/whl/cu121"

        [tool.uv.sources]
        jinja2 = { index = "pytorch" }

        [[tool.uv.index]]
        url = "https://pypi.org/simple"
        "#
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2025-01-30T00:00:00Z"

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.4"
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu121" }
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.4-py3-none-any.whl", upload-time = "2025-01-29T22:50:57.275Z" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cu121" }
        sdist = { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5.tar.gz", upload-time = "2025-01-29T22:50:57.539Z" }
        wheels = [
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", upload-time = "2025-01-29T22:50:57.538Z" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", upload-time = "2025-01-29T22:50:57.539Z" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", upload-time = "2025-01-29T22:50:57.539Z" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", upload-time = "2025-01-29T22:50:57.539Z" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", upload-time = "2025-01-29T22:50:57.539Z" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", upload-time = "2025-01-29T22:50:57.539Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
            { name = "jinja2" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "iniconfig", specifier = "==2.0.0" },
            { name = "jinja2", specifier = ">=3.1.4", index = "https://astral-sh.github.io/pytorch-mirror/whl/cu121" },
        ]
        "#
        );
    });

    // Adding a subsequent index with the same name should replace it.
    uv_snapshot!(context.filters(), context.add().arg("jinja2").arg("--index").arg("pytorch=https://test.pypi.org/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Audited 3 packages in [TIME]
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig==2.0.0",
            "jinja2>=3.1.4",
        ]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]

        [tool.uv.sources]
        jinja2 = { index = "pytorch" }

        [[tool.uv.index]]
        name = "pytorch"
        url = "https://test.pypi.org/simple"

        [[tool.uv.index]]
        url = "https://pypi.org/simple"
        "#
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2025-01-30T00:00:00Z"

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://test.pypi.org/simple" }
        sdist = { url = "https://test-files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:16.826Z" }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:14.843Z" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.4"
        source = { registry = "https://test.pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://test-files.pythonhosted.org/packages/ed/55/39036716d19cab0747a5020fc7e907f362fbf48c984b14e62127f7e68e5d/jinja2-3.1.4.tar.gz", hash = "sha256:4a3aee7acbbe7303aede8e9648d13b8bf88a429282aa6122a993f0ac800cb369", size = 240245, upload-time = "2024-05-05T23:41:55.462Z" }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/31/80/3a54838c3fb461f6fec263ebf3a3a41771bd05190238de3486aae8540c36/jinja2-3.1.4-py3-none-any.whl", hash = "sha256:bc5dd2abb727a5319567b7a813e6a2e7318c39f4f487cfe6c89c6f9c7d25197d", size = 133271, upload-time = "2024-05-05T23:41:53.978Z" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://test.pypi.org/simple" }
        sdist = { url = "https://test-files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384, upload-time = "2024-02-02T16:29:58.692Z" }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215, upload-time = "2024-02-02T16:29:16.627Z" },
            { url = "https://test-files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069, upload-time = "2024-02-02T16:29:17.7Z" },
            { url = "https://test-files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452, upload-time = "2024-02-02T16:29:19.023Z" },
            { url = "https://test-files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462, upload-time = "2024-02-02T16:29:20.405Z" },
            { url = "https://test-files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869, upload-time = "2024-02-02T16:29:21.362Z" },
            { url = "https://test-files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906, upload-time = "2024-02-02T16:29:22.336Z" },
            { url = "https://test-files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296, upload-time = "2024-02-02T16:29:23.308Z" },
            { url = "https://test-files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038, upload-time = "2024-02-02T16:29:24.323Z" },
            { url = "https://test-files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572, upload-time = "2024-02-02T16:29:25.527Z" },
            { url = "https://test-files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127, upload-time = "2024-02-02T16:29:26.487Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
            { name = "jinja2" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "iniconfig", specifier = "==2.0.0" },
            { name = "jinja2", specifier = ">=3.1.4", index = "https://test.pypi.org/simple" },
        ]
        "#
        );
    });

    // Adding a subsequent index with the same URL should bump it to the top.
    uv_snapshot!(context.filters(), context.add().arg("typing-extensions").arg("--index").arg("https://pypi.org/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.12.2
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig==2.0.0",
            "jinja2>=3.1.4",
            "typing-extensions>=4.12.2",
        ]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]

        [tool.uv.sources]
        jinja2 = { index = "pytorch" }

        [[tool.uv.index]]
        url = "https://pypi.org/simple"

        [[tool.uv.index]]
        name = "pytorch"
        url = "https://test.pypi.org/simple"
        "#
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2025-01-30T00:00:00Z"

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.4"
        source = { registry = "https://test.pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://test-files.pythonhosted.org/packages/ed/55/39036716d19cab0747a5020fc7e907f362fbf48c984b14e62127f7e68e5d/jinja2-3.1.4.tar.gz", hash = "sha256:4a3aee7acbbe7303aede8e9648d13b8bf88a429282aa6122a993f0ac800cb369", size = 240245, upload-time = "2024-05-05T23:41:55.462Z" }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/31/80/3a54838c3fb461f6fec263ebf3a3a41771bd05190238de3486aae8540c36/jinja2-3.1.4-py3-none-any.whl", hash = "sha256:bc5dd2abb727a5319567b7a813e6a2e7318c39f4f487cfe6c89c6f9c7d25197d", size = 133271, upload-time = "2024-05-05T23:41:53.978Z" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384, upload-time = "2024-02-02T16:31:22.863Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215, upload-time = "2024-02-02T16:30:33.081Z" },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069, upload-time = "2024-02-02T16:30:34.148Z" },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452, upload-time = "2024-02-02T16:30:35.149Z" },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462, upload-time = "2024-02-02T16:30:36.166Z" },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869, upload-time = "2024-02-02T16:30:37.834Z" },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906, upload-time = "2024-02-02T16:30:39.366Z" },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296, upload-time = "2024-02-02T16:30:40.413Z" },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038, upload-time = "2024-02-02T16:30:42.243Z" },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572, upload-time = "2024-02-02T16:30:43.326Z" },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127, upload-time = "2024-02-02T16:30:44.418Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
            { name = "jinja2" },
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "iniconfig", specifier = "==2.0.0" },
            { name = "jinja2", specifier = ">=3.1.4", index = "https://test.pypi.org/simple" },
            { name = "typing-extensions", specifier = ">=4.12.2" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.12.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/df/db/f35a00659bc03fec321ba8bce9420de607a1d37f8342eee1863174c69557/typing_extensions-4.12.2.tar.gz", hash = "sha256:1a7ead55c7e559dd4dee8856e3a88b41225abfe1ce8df57b7c13915fe121ffb8", size = 85321, upload-time = "2024-06-07T18:52:15.995Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/26/9f/ad63fc0248c5379346306f8668cda6e2e2e9c95e01216d2b8ffd9ff037d0/typing_extensions-4.12.2-py3-none-any.whl", hash = "sha256:04e5ca0351e0f3f85c6853954072df659d0d13fac324d0072316b67d7794700d", size = 37438, upload-time = "2024-06-07T18:52:13.582Z" },
        ]
        "#
        );
    });

    // Adding a subsequent index with the same URL should bump it to the top, but retain the name.
    uv_snapshot!(context.filters(), context.add().arg("typing-extensions").arg("--index").arg("https://test.pypi.org/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Audited 4 packages in [TIME]
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig==2.0.0",
            "jinja2>=3.1.4",
            "typing-extensions>=4.12.2",
        ]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]

        [tool.uv.sources]
        jinja2 = { index = "pytorch" }

        [[tool.uv.index]]
        name = "pytorch"
        url = "https://test.pypi.org/simple"

        [[tool.uv.index]]
        url = "https://pypi.org/simple"
        "#
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2025-01-30T00:00:00Z"

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.4"
        source = { registry = "https://test.pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://test-files.pythonhosted.org/packages/ed/55/39036716d19cab0747a5020fc7e907f362fbf48c984b14e62127f7e68e5d/jinja2-3.1.4.tar.gz", hash = "sha256:4a3aee7acbbe7303aede8e9648d13b8bf88a429282aa6122a993f0ac800cb369", size = 240245, upload-time = "2024-05-05T23:41:55.462Z" }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/31/80/3a54838c3fb461f6fec263ebf3a3a41771bd05190238de3486aae8540c36/jinja2-3.1.4-py3-none-any.whl", hash = "sha256:bc5dd2abb727a5319567b7a813e6a2e7318c39f4f487cfe6c89c6f9c7d25197d", size = 133271, upload-time = "2024-05-05T23:41:53.978Z" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384, upload-time = "2024-02-02T16:31:22.863Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215, upload-time = "2024-02-02T16:30:33.081Z" },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069, upload-time = "2024-02-02T16:30:34.148Z" },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452, upload-time = "2024-02-02T16:30:35.149Z" },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462, upload-time = "2024-02-02T16:30:36.166Z" },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869, upload-time = "2024-02-02T16:30:37.834Z" },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906, upload-time = "2024-02-02T16:30:39.366Z" },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296, upload-time = "2024-02-02T16:30:40.413Z" },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038, upload-time = "2024-02-02T16:30:42.243Z" },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572, upload-time = "2024-02-02T16:30:43.326Z" },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127, upload-time = "2024-02-02T16:30:44.418Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
            { name = "jinja2" },
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "iniconfig", specifier = "==2.0.0" },
            { name = "jinja2", specifier = ">=3.1.4", index = "https://test.pypi.org/simple" },
            { name = "typing-extensions", specifier = ">=4.12.2" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.12.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/df/db/f35a00659bc03fec321ba8bce9420de607a1d37f8342eee1863174c69557/typing_extensions-4.12.2.tar.gz", hash = "sha256:1a7ead55c7e559dd4dee8856e3a88b41225abfe1ce8df57b7c13915fe121ffb8", size = 85321, upload-time = "2024-06-07T18:52:15.995Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/26/9f/ad63fc0248c5379346306f8668cda6e2e2e9c95e01216d2b8ffd9ff037d0/typing_extensions-4.12.2-py3-none-any.whl", hash = "sha256:04e5ca0351e0f3f85c6853954072df659d0d13fac324d0072316b67d7794700d", size = 37438, upload-time = "2024-06-07T18:52:13.582Z" },
        ]
        "#
        );
    });

    Ok(())
}

/// Add an index provided via `--default-index`.
#[test]
fn add_default_index_url() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--default-index").arg("https://test.pypi.org/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig>=2.0.0",
        ]

        [[tool.uv.index]]
        url = "https://test.pypi.org/simple"
        default = true
        "#
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://test.pypi.org/simple" }
        sdist = { url = "https://test-files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:16.826Z" }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:14.843Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = ">=2.0.0" }]
        "#
        );
    });

    // Adding another `--default-index` replaces the current default.
    uv_snapshot!(context.filters(), context.add().arg("typing-extensions").arg("--default-index").arg("https://pypi.org/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig>=2.0.0",
            "typing-extensions>=4.10.0",
        ]

        [[tool.uv.index]]
        url = "https://pypi.org/simple"
        default = true
        "#
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "iniconfig", specifier = ">=2.0.0" },
            { name = "typing-extensions", specifier = ">=4.10.0" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-25T22:12:49.693Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-25T22:12:47.72Z" },
        ]
        "#
        );
    });

    Ok(())
}

#[tokio::test]
async fn add_index_credentials() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Provide credentials for the index via the environment variable.
    uv_snapshot!(context.filters(), context.add().arg("iniconfig==2.0.0").env(EnvVars::UV_DEFAULT_INDEX, proxy.authenticated_url("public", "heron", "/basic-auth/simple")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig==2.0.0",
        ]

        [[tool.uv.index]]
        url = "http://[LOCALHOST]/basic-auth/simple"
        default = true
        "#
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/basic-auth/simple" }
        sdist = { url = "http://[LOCALHOST]/basic-auth/files/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "http://[LOCALHOST]/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = "==2.0.0" }]
        "#
        );
    });

    Ok(())
}

#[tokio::test]
async fn existing_index_credentials() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc!(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        # Set an internal index as the default, without credentials.
        [[tool.uv.index]]
        name = "internal"
        url = "{proxy_uri}/basic-auth/simple"
        default = true
    "#,
        proxy_uri = proxy.uri()
    ))?;

    // Provide credentials for the index via the environment variable.
    uv_snapshot!(context.filters(), context.add().arg("iniconfig==2.0.0").env(EnvVars::UV_DEFAULT_INDEX, proxy.authenticated_url("public", "heron", "/basic-auth/simple")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig==2.0.0",
        ]

        # Set an internal index as the default, without credentials.
        [[tool.uv.index]]
        name = "internal"
        url = "http://[LOCALHOST]/basic-auth/simple"
        default = true
        "#
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/basic-auth/simple" }
        sdist = { url = "http://[LOCALHOST]/basic-auth/files/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "http://[LOCALHOST]/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = "==2.0.0" }]
        "#
        );
    });

    Ok(())
}

/// Add an index with a trailing slash.
#[test]
fn add_index_with_trailing_slash() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_exclude_newer("2025-01-30T00:00Z");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("iniconfig==2.0.0").arg("--index").arg("https://pypi.org/simple/"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig==2.0.0",
        ]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]

        [[tool.uv.index]]
        url = "https://pypi.org/simple/"
        "#
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2025-01-30T00:00:00Z"

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple/" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = "==2.0.0" }]
        "#
        );
    });

    Ok(())
}

/// Add an index without a trailing slash.
#[test]
fn add_index_without_trailing_slash() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_exclude_newer("2025-01-30T00:00Z");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("iniconfig==2.0.0").arg("--index").arg("https://pypi.org/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig==2.0.0",
        ]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]

        [[tool.uv.index]]
        url = "https://pypi.org/simple"
        "#
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2025-01-30T00:00:00Z"

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = "==2.0.0" }]
        "#
        );
    });

    Ok(())
}

/// Add an index with an existing relative path.
#[test]
fn add_index_with_existing_relative_path_index() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create test-index/ subdirectory and copy our "offline" tqdm wheel there
    let packages = context.temp_dir.child("test-index");
    packages.create_dir_all()?;

    let wheel_src = context
        .workspace_root
        .join("test/links/ok-1.0.0-py3-none-any.whl");
    let wheel_dst = packages.child("ok-1.0.0-py3-none-any.whl");
    fs_err::copy(&wheel_src, &wheel_dst)?;

    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--index").arg("./test-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    Ok(())
}

/// Add an index with a non-existent relative path.
#[test]
fn add_index_with_non_existent_relative_path() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--index").arg("./test-index"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Directory not found for index: file://[TEMP_DIR]/test-index
    ");

    Ok(())
}

/// Add an index with a non-existent relative path with the same name as a defined index.
#[tokio::test]
async fn add_index_with_non_existent_relative_path_with_same_name_as_index() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc!(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [[tool.uv.index]]
        name = "test-index"
        url = "{proxy_uri}/simple"
    "#,
        proxy_uri = proxy.uri()
    ))?;

    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--index").arg("./test-index"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Directory not found for index: file://[TEMP_DIR]/test-index
    ");

    Ok(())
}

#[tokio::test]
async fn add_index_empty_directory() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc!(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [[tool.uv.index]]
        name = "test-index"
        url = "{proxy_uri}/simple"
    "#,
        proxy_uri = proxy.uri()
    ))?;

    let packages = context.temp_dir.child("test-index");
    packages.create_dir_all()?;

    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--index").arg("./test-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Index directory `file://[TEMP_DIR]/test-index` is empty, skipping
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    Ok(())
}

#[test]
fn add_index_with_ambiguous_relative_path() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let context = context.with_filter((r"\./|\.\\", r"[PREFIX]"));

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    #[cfg(unix)]
    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--index").arg("test-index"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: Relative paths passed to `--index` or `--default-index` should be disambiguated from index names (use `[PREFIX]test-index`). Support for ambiguous values will be removed in the future
    error: Directory not found for index: file://[TEMP_DIR]/test-index
    ");

    #[cfg(windows)]
    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--index").arg("test-index"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: Relative paths passed to `--index` or `--default-index` should be disambiguated from index names (use `[PREFIX]test-index` or `[PREFIX]test-index`). Support for ambiguous values will be removed in the future
    error: Directory not found for index: file://[TEMP_DIR]/test-index
    ");

    Ok(())
}

/// Add a PyPI requirement.
#[test]
fn add_group_comment() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "myproject"
        version = "0.1.0"
        description = "Add your description here"
        requires-python = ">=3.11"
        [dependency-groups]
        # These are our dev dependencies
        dev = [
            "typing-extensions",
        ]
        # These are our test dependencies
        test = [
            "iniconfig"
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("--group").arg("dev").arg("sniffio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "myproject"
        version = "0.1.0"
        description = "Add your description here"
        requires-python = ">=3.11"
        [dependency-groups]
        # These are our dev dependencies
        dev = [
            "sniffio>=1.3.1",
            "typing-extensions",
        ]
        # These are our test dependencies
        test = [
            "iniconfig"
        ]
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.11"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "myproject"
        version = "0.1.0"
        source = { virtual = "." }

        [package.dev-dependencies]
        dev = [
            { name = "sniffio" },
            { name = "typing-extensions" },
        ]
        test = [
            { name = "iniconfig" },
        ]

        [package.metadata]

        [package.metadata.requires-dev]
        dev = [
            { name = "sniffio", specifier = ">=1.3.1" },
            { name = "typing-extensions" },
        ]
        test = [{ name = "iniconfig" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-25T22:12:49.693Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-25T22:12:47.72Z" },
        ]
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 2 packages in [TIME]
    ");

    Ok(())
}

#[test]
fn add_index_comments() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [[tool.uv.index]]
        name = "internal"
        url = "https://test.pypi.org/simple"  # This is a test index.
        default = true
    "#})?;

    // Preserve the comment on the index URL, despite replacing it.
    uv_snapshot!(context.filters(), context.add().arg("iniconfig==2.0.0").env(EnvVars::UV_DEFAULT_INDEX, "https://pypi.org/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig==2.0.0",
        ]

        [[tool.uv.index]]
        name = "internal"
        url = "https://pypi.org/simple"  # This is a test index.
        default = true
        "#
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = "==2.0.0" }]
        "#
        );
    });

    Ok(())
}

/// Accidentally add a dependency on the project itself.
#[test]
fn add_self() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "anyio"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Requirement name `anyio` matches project name `anyio`, but self-dependencies are not permitted without the `--dev` or `--optional` flags. If your project name (`anyio`) is shadowing that of a third-party dependency, consider renaming the project.
    ");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "anyio"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        types = ["typing-extensions>=4"]
    "#})?;

    // However, recursive extras are fine.
    uv_snapshot!(context.filters(), context.add().arg("anyio[types]").arg("--optional").arg("all"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "anyio"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        types = ["typing-extensions>=4"]
        all = [
            "anyio[types]",
        ]

        [tool.uv.sources]
        anyio = { workspace = true }
        "#
        );
    });

    // And recursive development dependencies
    uv_snapshot!(context.filters(), context.add().arg("anyio[types]").arg("--dev"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Audited 1 package in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "anyio"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        types = ["typing-extensions>=4"]
        all = [
            "anyio[types]",
        ]

        [tool.uv.sources]
        anyio = { workspace = true }

        [dependency-groups]
        dev = [
            "anyio[types]",
        ]
        "#
        );
    });

    Ok(())
}

#[test]
fn add_preserves_end_of_line_comments() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            # comment 0
            "anyio==3.7.0", # comment 1
            # comment 2
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("requests==2.31.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + anyio==3.7.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + sniffio==1.3.1
     + urllib3==2.2.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            # comment 0
            "anyio==3.7.0", # comment 1
            # comment 2
            "requests==2.31.0",
        ]
        "#
        );
    });
    Ok(())
}

#[test]
fn add_preserves_end_of_line_comment_on_non_last_deps() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [ # comment
            "anyio==3.7.0", # comment 1
            "sniffio==1.3.1",
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("requests==2.31.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + anyio==3.7.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + sniffio==1.3.1
     + urllib3==2.2.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [ # comment
            "anyio==3.7.0", # comment 1
            "requests==2.31.0",
            "sniffio==1.3.1",
        ]
        "#
        );
    });
    Ok(())
}

#[test]
fn add_direct_url_subdirectory() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("root @ https://github.com/user-attachments/files/18216295/subdirectory-test.tar.gz#subdirectory=packages/root"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + root==0.0.1 (from https://github.com/user-attachments/files/18216295/subdirectory-test.tar.gz#subdirectory=packages/root)
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "root",
        ]

        [tool.uv.sources]
        root = { url = "https://github.com/user-attachments/files/18216295/subdirectory-test.tar.gz", subdirectory = "packages/root" }
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642, upload-time = "2024-02-19T08:36:28.641Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584, upload-time = "2024-02-19T08:36:26.842Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "root" },
        ]

        [package.metadata]
        requires-dist = [{ name = "root", url = "https://github.com/user-attachments/files/18216295/subdirectory-test.tar.gz", subdirectory = "packages/root" }]

        [[package]]
        name = "root"
        version = "0.0.1"
        source = { url = "https://github.com/user-attachments/files/18216295/subdirectory-test.tar.gz", subdirectory = "packages/root" }
        dependencies = [
            { name = "anyio" },
        ]
        sdist = { hash = "sha256:24b55efee28d08ad3cdc58903e359e820601baa6a4a4b3424311541ebcfb09d3" }

        [package.metadata]
        requires-dist = [{ name = "anyio" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 4 packages in [TIME]
    ");

    Ok(())
}

#[test]
fn add_direct_url_subdirectory_raw() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("root @ https://github.com/user-attachments/files/18216295/subdirectory-test.tar.gz#subdirectory=packages/root").arg("--raw-sources"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + root==0.0.1 (from https://github.com/user-attachments/files/18216295/subdirectory-test.tar.gz#subdirectory=packages/root)
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "root @ https://github.com/user-attachments/files/18216295/subdirectory-test.tar.gz#subdirectory=packages/root",
        ]
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642, upload-time = "2024-02-19T08:36:28.641Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584, upload-time = "2024-02-19T08:36:26.842Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "root" },
        ]

        [package.metadata]
        requires-dist = [{ name = "root", url = "https://github.com/user-attachments/files/18216295/subdirectory-test.tar.gz", subdirectory = "packages/root" }]

        [[package]]
        name = "root"
        version = "0.0.1"
        source = { url = "https://github.com/user-attachments/files/18216295/subdirectory-test.tar.gz", subdirectory = "packages/root" }
        dependencies = [
            { name = "anyio" },
        ]
        sdist = { hash = "sha256:24b55efee28d08ad3cdc58903e359e820601baa6a4a4b3424311541ebcfb09d3" }

        [package.metadata]
        requires-dist = [{ name = "anyio" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 4 packages in [TIME]
    ");

    Ok(())
}

#[test]
fn add_preserves_open_bracket_comment() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [ # comment 0
            # comment 1
            "anyio==3.7.0", # comment 2
            # comment 3
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("requests==2.31.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + anyio==3.7.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + requests==2.31.0
     + sniffio==1.3.1
     + urllib3==2.2.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [ # comment 0
            # comment 1
            "anyio==3.7.0", # comment 2
            # comment 3
            "requests==2.31.0",
        ]
        "#
        );
    });
    Ok(())
}

#[test]
fn add_preserves_empty_comment() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            # First line.
            # Second line.
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            # First line.
            # Second line.
            "anyio==3.7.0",
        ]
        "#
        );
    });

    Ok(())
}

#[test]
fn add_preserves_trailing_comment() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "idna",
            "iniconfig",  # Use iniconfig.
            # First line.
            # Second line.
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio==3.7.0",
            "idna",
            "iniconfig",  # Use iniconfig.
            # First line.
            # Second line.
        ]
        "#
        );
    });

    uv_snapshot!(context.filters(), context.add().arg("typing-extensions"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio==3.7.0",
            "idna",
            "iniconfig",  # Use iniconfig.
            # First line.
            # Second line.
            "typing-extensions>=4.10.0",
        ]
        "#
        );
    });

    Ok(())
}

#[test]
fn add_preserves_trailing_depth() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
          "idna",
          "iniconfig",# Use iniconfig.
            # First line.
            # Second line.
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==3.7.0"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
          "anyio==3.7.0",
          "idna",
          "iniconfig",# Use iniconfig.
          # First line.
          # Second line.
        ]
        "#
        );
    });

    Ok(())
}

#[test]
fn add_preserves_first_own_line_comment() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            # comment
            "sniffio==1.3.1",
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("charset-normalizer"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + charset-normalizer==3.3.2
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "charset-normalizer>=3.3.2",
            # comment
            "sniffio==1.3.1",
        ]
        "#
        );
    });
    Ok(())
}

#[test]
fn add_preserves_first_line_bracket_comment() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [ # comment
            "sniffio==1.3.1",
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("charset-normalizer"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + charset-normalizer==3.3.2
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [ # comment
            "charset-normalizer>=3.3.2",
            "sniffio==1.3.1",
        ]
        "#
        );
    });
    Ok(())
}

#[test]
fn add_no_indent() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
"sniffio==1.3.1"
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("charset-normalizer"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + charset-normalizer==3.3.2
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
                [project]
                name = "project"
                version = "0.1.0"
                requires-python = ">=3.12"
                dependencies = [
            "charset-normalizer>=3.3.2",
            "sniffio==1.3.1",
        ]
        "#
        );
    });
    Ok(())
}

/// Accept requirements, not just package names, in `uv remove`.
#[test]
fn remove_requirement() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["flask"]
    "#})?;

    uv_snapshot!(context.filters(), context.remove().arg("flask[dotenv]"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        "#
        );
    });

    Ok(())
}

/// Remove all dependencies with remaining comments
#[test]
fn remove_all_with_comments() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "duct",
            "minilog",
            # foo
            # bar
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.remove().arg("duct").arg("minilog"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Audited in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            # foo
            # bar
        ]
        "#
        );
    });

    Ok(())
}

/// If multiple indexes are provided on the CLI, the first-provided index should take precedence
/// during resolution, and should appear first in the `pyproject.toml` file.
///
/// See: <https://github.com/astral-sh/uv/issues/14817>
#[test]
fn multiple_index_cli() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context
        .add()
        .arg("requests")
        .arg("--index")
        .arg("https://test.pypi.org/simple")
        .arg("--index")
        .arg("https://pypi.org/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + requests==2.5.4.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "requests>=2.5.4.1",
        ]

        [[tool.uv.index]]
        url = "https://test.pypi.org/simple"

        [[tool.uv.index]]
        url = "https://pypi.org/simple"
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "requests" },
        ]

        [package.metadata]
        requires-dist = [{ name = "requests", specifier = ">=2.5.4.1" }]

        [[package]]
        name = "requests"
        version = "2.5.4.1"
        source = { registry = "https://test.pypi.org/simple" }
        sdist = { url = "https://test-files.pythonhosted.org/packages/6e/93/638dbb5f2c1f4120edaad4f3d45ffb1718e463733ad07d68f59e042901d6/requests-2.5.4.1.tar.gz", hash = "sha256:b19df51fa3e52a2bd7fc80a1ac11fb6b2f51a7c0bf31ba9ff6b5d11ea8605ae9", size = 448691, upload-time = "2015-03-13T21:30:03.228Z" }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/6d/00/8ed1b6ea43b10bfe28d08e6af29fd6aa5d8dab5e45ead9394a6268a2d2ec/requests-2.5.4.1-py2.py3-none-any.whl", hash = "sha256:0a2c98e46121e7507afb0edc89d342641a1fb9e8d56f7d592d4975ee6b685f9a", size = 468942, upload-time = "2015-03-13T21:29:55.769Z" },
        ]
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    ");

    Ok(())
}

/// If an index is repeated by the CLI and an environment variable, the CLI value should take
/// precedence.
///
/// The index that appears in the `pyproject.toml` should also be consistent with the index that
/// appears in the `uv.lock`.
///
/// See: <https://github.com/astral-sh/uv/issues/11312>
#[test]
fn repeated_index_cli_environment_variable() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context
        .add()
        .arg("iniconfig")
        // Without a trailing slash.
        .arg("--index")
        .arg("https://test.pypi.org/simple")
        // With a trailing slash.
        .env(EnvVars::UV_DEFAULT_INDEX, "https://test.pypi.org/simple/"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig>=2.0.0",
        ]

        [[tool.uv.index]]
        url = "https://test.pypi.org/simple"
        default = true
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://test.pypi.org/simple" }
        sdist = { url = "https://test-files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:16.826Z" }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:14.843Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = ">=2.0.0" }]
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    ");

    Ok(())
}

/// If an index is repeated on the CLI, the first-provided index should take precedence.
/// Newlines in `UV_INDEX` should be treated as separators.
///
/// The index that appears in the `pyproject.toml` should also be consistent with the index that
/// appears in the `uv.lock`.
///
/// See: <https://github.com/astral-sh/uv/issues/11312>
#[test]
fn repeated_index_cli_environment_variable_newline() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context
        .add()
        .env(EnvVars::UV_INDEX, "https://test.pypi.org/simple\nhttps://test.pypi.org/simple/")
        .arg("iniconfig"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig>=2.0.0",
        ]

        [[tool.uv.index]]
        url = "https://test.pypi.org/simple"
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://test.pypi.org/simple" }
        sdist = { url = "https://test-files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:16.826Z" }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:14.843Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = ">=2.0.0" }]
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    ");

    Ok(())
}

/// If an index is repeated on the CLI, the first-provided index should take precedence.
///
/// The index that appears in the `pyproject.toml` should also be consistent with the index that
/// appears in the `uv.lock`.
///
/// See: <https://github.com/astral-sh/uv/issues/11312>
#[test]
fn repeated_index_cli() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context
        .add()
        .arg("iniconfig")
        // Without a trailing slash.
        .arg("--index")
        .arg("https://test.pypi.org/simple")
        // With a trailing slash.
        .arg("--index")
        .arg("https://test.pypi.org/simple/"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig>=2.0.0",
        ]

        [[tool.uv.index]]
        url = "https://test.pypi.org/simple"
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://test.pypi.org/simple" }
        sdist = { url = "https://test-files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:16.826Z" }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:14.843Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = ">=2.0.0" }]
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    ");

    Ok(())
}

/// If an index is repeated on the CLI, the first-provided index should take precedence.
///
/// The index that appears in the `pyproject.toml` should also be consistent with the index that
/// appears in the `uv.lock`.
///
/// See: <https://github.com/astral-sh/uv/issues/11312>
#[test]
fn repeated_index_cli_reversed() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context
        .add()
        .arg("iniconfig")
        // With a trailing slash.
        .arg("--index")
        .arg("https://test.pypi.org/simple/")
        // Without a trailing slash.
        .arg("--index")
        .arg("https://test.pypi.org/simple"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig>=2.0.0",
        ]

        [[tool.uv.index]]
        url = "https://test.pypi.org/simple/"
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://test.pypi.org/simple/" }
        sdist = { url = "https://test-files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:16.826Z" }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:14.843Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = ">=2.0.0" }]
        "#
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Audited 1 package in [TIME]
    ");

    Ok(())
}

#[test]
fn add_with_build_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.9");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.8"
    dependencies = []

    [tool.uv]
    build-constraint-dependencies = ["setuptools==1"]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("requests==1.2"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `requests==1.2.0`
      ├─▶ Failed to resolve requirements from `setup.py` build
      ├─▶ No solution found when resolving: `setuptools>=40.8.0`
      ╰─▶ Because you require setuptools>=40.8.0 and setuptools==1, we can conclude that your requirements are unsatisfiable.
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    ");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.8"
    dependencies = []

    [tool.uv]
    build-constraint-dependencies = ["setuptools>=40"]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("requests==1.2"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + requests==1.2.0
    ");

    Ok(())
}

#[test]
#[cfg(feature = "test-git")]
fn add_unsupported_git_scheme() {
    let context = uv_test::test_context!("3.12");

    context.init().arg(".").assert().success();

    uv_snapshot!(context.filters(), context.add().arg("git+fantasy://ferris/dreams/of/urls@7701ffcbae245819b828dc5f885a5201158897ef"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `git+fantasy://ferris/dreams/of/urls@7701ffcbae245819b828dc5f885a5201158897ef`
      Caused by: Unsupported Git URL scheme `fantasy:` in `fantasy://ferris/dreams/of/urls` (expected one of `https:`, `ssh:`, or `file:`)
    git+fantasy://ferris/dreams/of/urls@7701ffcbae245819b828dc5f885a5201158897ef
    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    ");
}

#[tokio::test]
async fn add_index_url_in_keyring() -> Result<()> {
    let keyring_context = uv_test::test_context!("3.12");

    // Install our keyring plugin
    keyring_context
        .pip_install()
        .arg(
            keyring_context
                .workspace_root
                .join("test")
                .join("packages")
                .join("keyring_test_plugin"),
        )
        .assert()
        .success();

    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc!(
        r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []
        [tool.uv]
        keyring-provider = "subprocess"
        [[tool.uv.index]]
        name = "proxy"
        url = "{proxy_uri}/basic-auth/simple"
        default = true
        "#,
        proxy_uri = proxy.uri()
    ))?;

    uv_snapshot!(context.filters(), context.add().arg("anyio")
        .env(EnvVars::index_username("PROXY"), "public")
        .env(EnvVars::KEYRING_TEST_CREDENTIALS, format!(r#"{{"{}": {{"public": "heron"}}}}"#, proxy.url("/basic-auth/simple")))
        .env(EnvVars::PATH, venv_bin_path(&keyring_context.venv)), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Keyring request for public@http://[LOCALHOST]/basic-auth/simple
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "
    );

    context.assert_command("import anyio").success();
    Ok(())
}

#[tokio::test]
async fn add_full_url_in_keyring() -> Result<()> {
    let keyring_context = uv_test::test_context!("3.12");

    // Install our keyring plugin
    keyring_context
        .pip_install()
        .arg(
            keyring_context
                .workspace_root
                .join("test")
                .join("packages")
                .join("keyring_test_plugin"),
        )
        .assert()
        .success();

    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc!(
        r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []
        [tool.uv]
        keyring-provider = "subprocess"
        [[tool.uv.index]]
        name = "proxy"
        url = "{proxy_uri}/basic-auth/simple"
        default = true
        "#,
        proxy_uri = proxy.uri()
    ))?;

    uv_snapshot!(context.filters(), context.add().arg("anyio")
        .env(EnvVars::index_username("PROXY"), "public")
        .env(EnvVars::KEYRING_TEST_CREDENTIALS, format!(r#"{{"{}": {{"public": "heron"}}}}"#, proxy.url("/basic-auth/simple/anyio")))
        .env(EnvVars::PATH, venv_bin_path(&keyring_context.venv)), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Keyring request for public@http://[LOCALHOST]/basic-auth/simple
    Keyring request for public@[LOCALHOST]
    Keyring request for public@http://[LOCALHOST]
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the package registry and your project depends on anyio, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (http://[LOCALHOST]/basic-auth/simple) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "
    );
    Ok(())
}

/// If uv receives an authentication failure from a configured index, it
/// should not fall back to the default index.
#[tokio::test]
async fn add_stop_index_search_early_on_auth_failure() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc!(
        r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []
        [[tool.uv.index]]
        name = "my-index"
        url = "{proxy_uri}/basic-auth/simple"
        "#,
        proxy_uri = proxy.uri()
    ))?;

    uv_snapshot!(context.filters(), context.add().arg("anyio"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the package registry and your project depends on anyio, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (http://[LOCALHOST]/basic-auth/simple) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "
    );
    Ok(())
}

/// uv should continue searching the default index if it receives an
/// authentication failure that is specified in `ignore-error-codes`.
#[tokio::test]
async fn add_ignore_error_codes() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc!(
        r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []
        [[tool.uv.index]]
        name = "my-index"
        url = "{proxy_uri}/basic-auth/simple"
        ignore-error-codes = [401, 403]
        "#,
        proxy_uri = proxy.uri()
    ))?;

    uv_snapshot!(context.filters(), context.add().arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "
    );

    context.assert_command("import anyio").success();
    Ok(())
}

/// uv should only fall through on 404s if an empty list is specified
/// in `ignore-error-codes`, even for indexes that normally ignore 403s.
#[tokio::test]
async fn add_empty_ignore_error_codes() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []

        [[tool.uv.index]]
        name = "my-index"
        url = "{server_url}"
        ignore-error-codes = []
        "#,
        server_url = server.uri(),
    })?;

    // The empty `ignore-error-codes` list means 403 errors should NOT be ignored.
    uv_snapshot!(context.filters(), context.add().arg("anyio"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the package registry and your project depends on anyio, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (http://[LOCALHOST]/) returned a 403 Forbidden error. This could indicate lack of valid authentication credentials, or the package may not exist on this index.
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "
    );
    Ok(())
}

/// uv should not report a credential error on a missing package for pytorch since
/// pytorch returns 403s to indicate not found.
#[test]
fn add_missing_package_on_pytorch() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []

        [tool.uv.sources]
        fakepkg = { index = "pytorch" }

        [[tool.uv.index]]
        name = "pytorch"
        url = "https://download.pytorch.org/whl/cpu"
        "#
    })?;

    uv_snapshot!(context.add().arg("fakepkg"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because fakepkg was not found in the package registry and your project depends on fakepkg, we can conclude that your project's requirements are unsatisfiable.
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "
    );
    Ok(())
}

/// Test HTTP errors other than 401s and 403s.
#[tokio::test]
async fn add_unexpected_error_code() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []
        "#
    })?;

    uv_snapshot!(context.filters(), context.add().arg("anyio").arg("--index").arg(server.uri())
        .env(EnvVars::UV_TEST_NO_HTTP_RETRY_DELAY, "true"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Request failed after 3 retries
      Caused by: Failed to fetch: `http://[LOCALHOST]/anyio/`
      Caused by: HTTP status server error (503 Service Unavailable) for url (http://[LOCALHOST]/anyio/)
    "
    );
    Ok(())
}

/// uv should fail to parse `pyproject.toml` if `ignore-error-codes`
/// contains an invalid status code number.
#[tokio::test]
async fn add_invalid_ignore_error_code() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc!(
        r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []
        [[tool.uv.index]]
        name = "my-index"
        url = "{proxy_uri}/basic-auth/simple"
        ignore-error-codes = [401, 403, 1234]
        "#,
        proxy_uri = proxy.uri()
    ))?;

    uv_snapshot!(context.filters(), context.add().arg("anyio"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 9, column 22
        |
      9 | ignore-error-codes = [401, 403, 1234]
        |                      ^^^^^^^^^^^^^^^^
      1234 is not a valid HTTP status code

    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 9, column 22
      |
    9 | ignore-error-codes = [401, 403, 1234]
      |                      ^^^^^^^^^^^^^^^^
    1234 is not a valid HTTP status code
    "
    );

    Ok(())
}

/// uv should fail to parse `pyproject.toml` if `require-python`
/// contains an invalid specifier and try to return a helpful hint.
#[test]
fn add_invalid_requires_python() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = "3.12"
        dependencies = []
        "#
    })?;

    uv_snapshot!(context.add().arg("anyio"), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 4, column 19
      |
    4 | requires-python = "3.12"
      |                   ^^^^^^
    Failed to parse version: Unexpected end of version specifier, expected operator. Did you mean `==3.12`?:
    3.12
    ^^^^
    "#);

    Ok(())
}

/// In authentication "always", the normal authentication flow should still work.
#[tokio::test]
async fn add_auth_policy_always_with_credentials() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc!(
        r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []

        [[tool.uv.index]]
        name = "my-index"
        url = "{proxy_uri}/basic-auth/simple"
        authenticate = "always"
        default = true
        "#,
        proxy_uri = proxy.uri()
    ))?;

    uv_snapshot!(context.filters(), context.add().arg("anyio")
        .env(EnvVars::UV_INDEX_MY_INDEX_USERNAME, "public")
        .env(EnvVars::UV_INDEX_MY_INDEX_PASSWORD, "heron"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "
    );

    context.assert_command("import anyio").success();
    Ok(())
}

/// In authentication "always", unauthenticated requests to a registry that
/// doesn't require credentials will fail.
#[test]
fn add_auth_policy_always_without_credentials() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []

        [[tool.uv.index]]
        name = "my-index"
        url = "https://pypi.org/simple"
        authenticate = "always"
        default = true
        "#
    })?;

    uv_snapshot!(context.add().arg("anyio"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch: `https://pypi.org/simple/anyio/`
      Caused by: Missing credentials for https://pypi.org/simple/anyio/
    "
    );

    uv_snapshot!(context.pip_install().arg("black"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch: `https://pypi.org/simple/black/`
      Caused by: Missing credentials for https://pypi.org/simple/black/
    "
    );
    Ok(())
}

/// In authentication "always", authenticated requests with a username but
/// no discoverable password will fail.
#[test]
fn add_auth_policy_always_with_username_no_password() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []

        [[tool.uv.index]]
        name = "my-index"
        url = "https://public@pypi.org/simple"
        authenticate = "always"
        default = true
        "#
    })?;

    uv_snapshot!(context.add().arg("anyio"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch: `https://pypi.org/simple/anyio/`
      Caused by: Missing password for https://pypi.org/simple/anyio/
    "
    );
    Ok(())
}

/// In authentication "never", even if the correct credentials are supplied
/// in the URL, no authenticated requests will be allowed.
#[tokio::test]
async fn add_auth_policy_never_with_url_credentials() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc!(
        r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []

        [[tool.uv.index]]
        name = "my-index"
        url = "{proxy_auth_uri}/basic-auth/simple"
        authenticate = "never"
        default = true
        "#,
        proxy_auth_uri = proxy.authenticated_uri("public", "heron")
    ))?;

    uv_snapshot!(context.filters(), context.add().arg("anyio"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to fetch: `http://[LOCALHOST]/basic-auth/files/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl`
      Caused by: HTTP status client error (401 Unauthorized) for url (http://[LOCALHOST]/basic-auth/files/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl)
    "
    );

    Ok(())
}

/// In authentication "never", even if the correct credentials are supplied
/// via env vars, no authenticated requests will be allowed.
#[tokio::test]
async fn add_auth_policy_never_with_env_var_credentials() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc!(
        r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []

        [[tool.uv.index]]
        name = "my-index"
        url = "{proxy_uri}/basic-auth/simple"
        authenticate = "never"
        default = true
        "#,
        proxy_uri = proxy.uri()
    ))?;

    uv_snapshot!(context.filters(), context.add().arg("anyio")
        .env(EnvVars::UV_INDEX_MY_INDEX_USERNAME, "public")
        .env(EnvVars::UV_INDEX_MY_INDEX_PASSWORD, "heron"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the package registry and your project depends on anyio, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (http://[LOCALHOST]/basic-auth/simple) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "
    );

    Ok(())
}

/// In authentication "never", the normal flow for unauthenticated requests should
/// still work.
#[test]
fn add_auth_policy_never_without_credentials() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.11, <4"
        dependencies = []

        [[tool.uv.index]]
        name = "my-index"
        url = "https://pypi.org/simple"
        authenticate = "never"
        default = true
        "#
    })?;

    uv_snapshot!(context.add().arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "
    );

    context.assert_command("import anyio").success();
    Ok(())
}

/// If uv receives a 302 redirect to a cross-origin server, it should not forward
/// credentials. In the absence of a netrc entry for the new location,
/// it should fail.
#[tokio::test]
async fn add_redirect_cross_origin() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;
    let filters = context
        .filters()
        .into_iter()
        .chain([(r"127\.0\.0\.1:\d*", "[LOCALHOST]")])
        .collect::<Vec<_>>();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []
        "#
    })?;

    let redirect_server = MockServer::start().await;
    let proxy_base = proxy.url("/basic-auth/simple/");

    Mock::given(method("GET"))
        .respond_with(move |req: &wiremock::Request| {
            let redirect_url = redirect_url_to_base(req, &proxy_base);
            ResponseTemplate::new(302).insert_header("Location", &redirect_url)
        })
        .mount(&redirect_server)
        .await;

    let mut redirect_url = Url::parse(&redirect_server.uri())?;
    let _ = redirect_url.set_username("public");
    let _ = redirect_url.set_password(Some("heron"));

    uv_snapshot!(filters, context.add().arg("--default-index").arg(redirect_url.as_str()).arg("anyio"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the package registry and your project depends on anyio, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (http://[LOCALHOST]/) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "
    );

    Ok(())
}

/// If uv receives a 302 redirect to a cross-origin server with credentials
/// in the location, use those credentials for the redirect request.
#[tokio::test]
async fn add_redirect_cross_origin_credentials_in_location() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;
    let filters = context
        .filters()
        .into_iter()
        .chain([(r"127\.0\.0\.1:\d*", "[LOCALHOST]")])
        .collect::<Vec<_>>();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []
        "#
    })?;

    let redirect_server = MockServer::start().await;
    let proxy_base = proxy.authenticated_url("public", "heron", "/basic-auth/simple/");

    Mock::given(method("GET"))
        .respond_with(move |req: &wiremock::Request| {
            // Responds with credentials in the location
            let redirect_url = redirect_url_to_base(req, &proxy_base);
            ResponseTemplate::new(302).insert_header("Location", &redirect_url)
        })
        .mount(&redirect_server)
        .await;

    let redirect_url = Url::parse(&redirect_server.uri())?;

    uv_snapshot!(filters, context.add().arg("--default-index").arg(redirect_url.as_str()).arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "
    );

    Ok(())
}

/// uv currently fails to look up keyring credentials on a cross-origin redirect.
#[tokio::test]
async fn add_redirect_with_keyring_cross_origin() -> Result<()> {
    let keyring_context = uv_test::test_context!("3.12");

    // Install our keyring plugin
    keyring_context
        .pip_install()
        .arg(
            keyring_context
                .workspace_root
                .join("test")
                .join("packages")
                .join("keyring_test_plugin"),
        )
        .assert()
        .success();

    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;
    let filters = context
        .filters()
        .into_iter()
        .chain([(r"127\.0\.0\.1:\d*", "[LOCALHOST]")])
        .collect::<Vec<_>>();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! { r#"
        [project]
        name = "foo"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        keyring-provider = "subprocess"
        "#,
    })?;

    let redirect_server = MockServer::start().await;
    let proxy_base = proxy.url("/basic-auth/simple/");

    Mock::given(method("GET"))
        .respond_with(move |req: &wiremock::Request| {
            let redirect_url = redirect_url_to_base(req, &proxy_base);
            ResponseTemplate::new(302).insert_header("Location", &redirect_url)
        })
        .mount(&redirect_server)
        .await;

    let mut redirect_url = Url::parse(&redirect_server.uri())?;
    let _ = redirect_url.set_username("public");

    uv_snapshot!(filters, context.add().arg("--default-index")
        .arg(redirect_url.as_str())
        .arg("anyio")
        .env(EnvVars::KEYRING_TEST_CREDENTIALS, format!(r#"{{"{host}": {{"public": "heron"}}}}"#, host = proxy.host_port()))
        .env(EnvVars::PATH, venv_bin_path(&keyring_context.venv)), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Keyring request for public@http://[LOCALHOST]/
    Keyring request for public@[LOCALHOST]
    Keyring request for public@http://[LOCALHOST]
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the package registry and your project depends on anyio, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (http://[LOCALHOST]/) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
      help: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing.
    "
    );

    Ok(())
}

/// If uv receives a cross-origin 302 redirect, it should use credentials from netrc
/// for the new location.
#[tokio::test]
async fn pip_install_redirect_with_netrc_cross_origin() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let proxy = crate::pypi_proxy::start().await;
    let filters = context
        .filters()
        .into_iter()
        .chain([(r"127\.0\.0\.1:\d*", "[LOCALHOST]")])
        .collect::<Vec<_>>();

    let netrc = context.temp_dir.child(".netrc");
    netrc.write_str(&format!(
        "machine {} login public password heron",
        proxy.host()
    ))?;

    let redirect_server = MockServer::start().await;
    let proxy_base = proxy.url("/basic-auth/simple/");

    Mock::given(method("GET"))
        .respond_with(move |req: &wiremock::Request| {
            let redirect_url = redirect_url_to_base(req, &proxy_base);
            ResponseTemplate::new(302).insert_header("Location", &redirect_url)
        })
        .mount(&redirect_server)
        .await;

    let mut redirect_url = Url::parse(&redirect_server.uri())?;
    let _ = redirect_url.set_username("public");

    uv_snapshot!(filters, context.pip_install()
        .arg("anyio")
        .arg("--index-url")
        .arg(redirect_url.as_str())
        .env(EnvVars::NETRC, netrc.to_str().unwrap())
        .arg("--strict"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + sniffio==1.3.1
    "
    );

    context.assert_command("import anyio").success();

    Ok(())
}

fn redirect_url_to_base(req: &wiremock::Request, base: &str) -> String {
    let last_path_segment = req
        .url
        .path_segments()
        .expect("path has segments")
        .rfind(|segment| !segment.is_empty())
        .expect("path has a package segment");
    format!("{base}{last_path_segment}/")
}

/// Test the error message when adding a package with multiple existing references in
/// `pyproject.toml`.
#[test]
fn add_ambiguous() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12.0"
        dependencies = [
            "anyio>=4.0.0",
            "anyio>=4.1.0",
        ]
        [dependency-groups]
        bar = ["anyio>=4.1.0", "anyio>=4.2.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot perform ambiguous update; found multiple entries for `anyio`:
    - `anyio>=4.0.0`
    - `anyio>=4.1.0`
    ");

    uv_snapshot!(context.filters(), context.add().arg("--group").arg("bar").arg("anyio"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot perform ambiguous update; found multiple entries for `anyio`:
    - `anyio>=4.1.0`
    - `anyio>=4.2.0`
    ");

    Ok(())
}

/// Normalize extra names when adding or removing.
#[test]
fn add_optional_normalize() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        cloud_export_to_parquet = [
            "anyio==3.7.0",
        ]
    "#})?;

    // Add with a non-normalized group name.
    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--optional").arg("cloud_export_to_parquet"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");

    assert_snapshot!(pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = []

    [project.optional-dependencies]
    cloud_export_to_parquet = [
        "anyio==3.7.0",
        "iniconfig>=2.0.0",
    ]
    "#
    );

    // Add with a normalized group name (which doesn't match the `pyproject.toml`).
    uv_snapshot!(context.filters(), context.add().arg("typing-extensions").arg("--optional").arg("cloud-export-to-parquet"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    assert_snapshot!(pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = []

    [project.optional-dependencies]
    cloud_export_to_parquet = [
        "anyio==3.7.0",
        "iniconfig>=2.0.0",
        "typing-extensions>=4.10.0",
    ]
    "#
    );

    // Remove with a non-normalized group name.
    uv_snapshot!(context.filters(), context.remove().arg("iniconfig").arg("--optional").arg("cloud_export_to_parquet"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Uninstalled 5 packages in [TIME]
     - anyio==3.7.0
     - idna==3.6
     - iniconfig==2.0.0
     - sniffio==1.3.1
     - typing-extensions==4.10.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    assert_snapshot!(pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = []

    [project.optional-dependencies]
    cloud_export_to_parquet = [
        "anyio==3.7.0",
        "typing-extensions>=4.10.0",
    ]
    "#
    );

    // Remove with a normalized group name (which doesn't match the `pyproject.toml`).
    uv_snapshot!(context.filters(), context.remove().arg("typing-extensions").arg("--optional").arg("cloud-export-to-parquet"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Audited in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");

    assert_snapshot!(pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = []

    [project.optional-dependencies]
    cloud_export_to_parquet = [
        "anyio==3.7.0",
    ]
    "#
    );

    Ok(())
}

/// Test `uv add` with different kinds of bounds and constraints.
#[test]
fn add_bounds() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Set bounds in `uv.toml`
    let uv_toml = context.temp_dir.child("uv.toml");
    uv_toml.write_str(indoc! {r#"
        add-bounds = "exact"
    "#})?;
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("idna"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + idna==3.6
    ");

    let pyproject_toml = context.read("pyproject.toml");
    assert_snapshot!(
        pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "idna==3.6",
    ]
    "#
    );

    fs_err::remove_file(uv_toml)?;

    // Set bounds in `pyproject.toml`
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv]
        add-bounds = "major"
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + anyio==4.3.0
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");
    assert_snapshot!(
        pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "anyio>=4.3.0,<5.0.0",
    ]

    [tool.uv]
    add-bounds = "major"
    "#
    );

    // Existing constraints take precedence over the bounds option
    uv_snapshot!(context.filters(), context.add().arg("anyio").arg("--bounds").arg("minor"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Audited 3 packages in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");
    assert_snapshot!(
        pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "anyio>=4.3.0,<5.0.0",
    ]

    [tool.uv]
    add-bounds = "major"
    "#
    );

    // Explicit constraints take precedence over the bounds option
    uv_snapshot!(context.filters(), context.add().arg("anyio==4.2").arg("idna").arg("--bounds").arg("minor"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - anyio==4.3.0
     + anyio==4.2.0
    ");

    let pyproject_toml = context.read("pyproject.toml");
    assert_snapshot!(
        pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "anyio==4.2",
        "idna>=3.6,<3.7",
    ]

    [tool.uv]
    add-bounds = "major"
    "#
    );

    // Set bounds on the CLI.
    uv_snapshot!(context.filters(), context.add().arg("sniffio").arg("--bounds").arg("minor"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Audited 3 packages in [TIME]
    ");

    let pyproject_toml = context.read("pyproject.toml");
    assert_snapshot!(
        pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "anyio==4.2",
        "idna>=3.6,<3.7",
        "sniffio>=1.3.1,<1.4.0",
    ]

    [tool.uv]
    add-bounds = "major"
    "#
    );

    Ok(())
}

/// Hint that we're using an explicit bound over the preferred bounds.
#[test]
fn add_bounds_requirement_over_bounds_kind() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Set bounds in `uv.toml`
    let uv_toml = context.temp_dir.child("uv.toml");
    uv_toml.write_str(indoc! {r#"
        add-bounds = "exact"
    "#})?;
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("anyio==4.2").arg("idna").arg("--bounds").arg("minor"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    note: Using explicit requirement `anyio==4.2` over bounds preference `minor`
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.2.0
     + idna==3.6
     + sniffio==1.3.1
    ");

    let pyproject_toml = context.read("pyproject.toml");
    assert_snapshot!(
        pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "anyio==4.2",
        "idna>=3.6,<3.7",
    ]
    "#
    );

    Ok(())
}

/// Add a path dependency with `--workspace` flag to add it to workspace members. The root already
/// contains a workspace definition, so the package should be added to the workspace members.
#[test]
fn add_path_with_existing_workspace() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let workspace_toml = context.temp_dir.child("pyproject.toml");
    workspace_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"

        [tool.uv.workspace]
        members = ["project"]
    "#})?;

    // Create a project within the workspace.
    let project_dir = context.temp_dir.child("project");
    project_dir.create_dir_all()?;

    let project_toml = project_dir.child("pyproject.toml");
    project_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Create a dependency package outside the workspace members.
    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;

    let dep_toml = dep_dir.child("pyproject.toml");
    dep_toml.write_str(indoc! {r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Add the dependency from the project directory. It should automatically be added as a
    // workspace member, since it's in the same directory as the workspace.
    uv_snapshot!(context.filters(), context
        .add()
        .current_dir(&project_dir)
        .arg("../dep"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Added `dep` to workspace members
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    let pyproject_toml = context.read("pyproject.toml");
    assert_snapshot!(
        pyproject_toml, @r#"
    [project]
    name = "parent"
    version = "0.1.0"
    requires-python = ">=3.12"

    [tool.uv.workspace]
    members = [
        "project",
        "dep",
    ]
    "#
    );

    let pyproject_toml = context.read("project/pyproject.toml");
    assert_snapshot!(
        pyproject_toml, @r#"
    [project]
    name = "project"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "dep",
    ]

    [tool.uv.sources]
    dep = { workspace = true }
    "#
    );

    Ok(())
}

/// Add a path dependency with `--workspace` flag to add it to workspace members. The root doesn't
/// contain a workspace definition, so `uv add` should create one.
#[test]
fn add_path_with_workspace() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let workspace_toml = context.temp_dir.child("pyproject.toml");
    workspace_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
    "#})?;

    // Create a dependency package outside the workspace members.
    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;

    let dep_toml = dep_dir.child("pyproject.toml");
    dep_toml.write_str(indoc! {r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Add the dependency with `--workspace` flag from the project directory.
    uv_snapshot!(context.filters(), context
        .add()
        .arg("./dep")
        .arg("--workspace"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Added `dep` to workspace members
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    let pyproject_toml = context.read("pyproject.toml");
    assert_snapshot!(
        pyproject_toml, @r#"
    [project]
    name = "parent"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "dep",
    ]

    [tool.uv.workspace]
    members = [
        "dep",
    ]

    [tool.uv.sources]
    dep = { workspace = true }
    "#
    );

    Ok(())
}

/// Add a path dependency within the workspace directory without --workspace flag.
/// It should automatically be added as a workspace member.
#[test]
fn add_path_within_workspace_defaults_to_workspace() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let workspace_toml = context.temp_dir.child("pyproject.toml");
    workspace_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv.workspace]
        members = []
    "#})?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Add the dependency without --workspace flag - it should still be added as workspace member
    // since it's within the workspace directory.
    uv_snapshot!(context.filters(), context
        .add()
        .arg("./dep"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Added `dep` to workspace members
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    let pyproject_toml = context.read("pyproject.toml");
    assert_snapshot!(
        pyproject_toml, @r#"
    [project]
    name = "parent"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "dep",
    ]

    [tool.uv.workspace]
    members = [
        "dep",
    ]

    [tool.uv.sources]
    dep = { workspace = true }
    "#
    );

    Ok(())
}

/// Add a path dependency within the workspace directory with --no-workspace flag.
/// It should be added as a direct path dependency.
#[test]
fn add_path_with_no_workspace() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let workspace_toml = context.temp_dir.child("pyproject.toml");
    workspace_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv.workspace]
        members = []
    "#})?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Add the dependency with --no-workspace flag - it should be added as direct path dependency.
    uv_snapshot!(context.filters(), context
        .add()
        .arg("./dep")
        .arg("--no-workspace"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    let pyproject_toml = context.read("pyproject.toml");
    assert_snapshot!(
        pyproject_toml, @r#"
    [project]
    name = "parent"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "dep",
    ]

    [tool.uv.workspace]
    members = []

    [tool.uv.sources]
    dep = { path = "dep" }
    "#
    );

    Ok(())
}

/// Add a path dependency outside the workspace directory.
/// It should be added as a direct path dependency, not a workspace member.
#[test]
fn add_path_outside_workspace_no_default() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create a workspace directory
    let workspace_dir = context.temp_dir.child("workspace");
    workspace_dir.create_dir_all()?;

    let workspace_toml = workspace_dir.child("pyproject.toml");
    workspace_toml.write_str(indoc! {r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv.workspace]
        members = []
    "#})?;

    // Create a dependency outside the workspace
    let dep_dir = context.temp_dir.child("external_dep");
    dep_dir.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Add the dependency without --workspace flag - it should be a direct path dependency
    // since it's outside the workspace directory.
    uv_snapshot!(context.filters(), context
        .add()
        .current_dir(&workspace_dir)
        .arg("../external_dep"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/external_dep)
    ");

    let pyproject_toml = fs_err::read_to_string(workspace_toml)?;
    assert_snapshot!(
        pyproject_toml, @r#"
    [project]
    name = "parent"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = [
        "dep",
    ]

    [tool.uv.workspace]
    members = []

    [tool.uv.sources]
    dep = { path = "../external_dep" }
    "#
    );

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/14961>
#[test]
fn add_multiline_indentation() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [dependency-groups]
        dev = ["ruff", "typing-extensions"]
    "#})?;

    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--dev"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + iniconfig==2.0.0
     + ruff==0.3.4
     + typing-extensions==4.10.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [dependency-groups]
        dev = [
            "iniconfig>=2.0.0",
            "ruff",
            "typing-extensions",
        ]
        "#
        );
    });

    Ok(())
}

/// Add a requirement without installing the project.
#[test]
fn add_no_install_project() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    context
        .temp_dir
        .child("project")
        .child("src")
        .child("project")
        .child("__init__.py")
        .touch()?;

    uv_snapshot!(context.filters(), context.add().arg("iniconfig").arg("--no-install-project"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    ");

    let pyproject_toml = context.read("pyproject.toml");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig>=2.0.0",
        ]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#
        );
    });

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = ">=2.0.0" }]
        "#
        );
    });

    Ok(())
}

#[test]
#[cfg(feature = "test-git-lfs")]
fn add_git_lfs_error() -> Result<()> {
    let context = uv_test::test_context!("3.13").with_git_lfs_config();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.13"
        dependencies = []
    "#})?;

    // Request lfs (via arg) without a Git source.
    uv_snapshot!(context.filters(), context.add().arg("typing-extensions").arg("--lfs"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `typing-extensions` did not resolve to a Git repository, but a Git extension (`--lfs`) was provided.
    ");

    // Request lfs (via env var) without a Git source.
    uv_snapshot!(context.filters(), context.add().arg("typing-extensions").env(EnvVars::UV_GIT_LFS, "true"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    ");

    // Request lfs (both arg and env var) without a Git source.
    uv_snapshot!(context.filters(), context.add().arg("typing-extensions").env(EnvVars::UV_GIT_LFS, "true").arg("--lfs"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `typing-extensions` did not resolve to a Git repository, but a Git extension (`--lfs`) was provided.
    ");

    // Request lfs from arg and disable lfs from env var (should be ignored) without a Git source.
    uv_snapshot!(context.filters(), context.add().arg("typing-extensions").env(EnvVars::UV_GIT_LFS, "false").arg("--lfs"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `typing-extensions` did not resolve to a Git repository, but a Git extension (`--lfs`) was provided.
    ");

    Ok(())
}
